//! MCP client for connecting to the code intelligence server

use anyhow::{Result, anyhow};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Instant;

pub struct CodeIntelligenceClient;

impl CodeIntelligenceClient {
    /// Connect to MCP server and test it (thin client - no index loading)
    pub async fn test_server(
        server_binary: PathBuf,
        config_path: Option<PathBuf>,
        tool: Option<String>,
        args: Option<String>,
        delay_before_tool_secs: Option<u64>,
        repeat: usize,
    ) -> Result<()> {
        use rmcp::{
            model::{CallToolRequestParams, ClientRequest, CustomRequest, JsonObject},
            service::ServiceExt,
            transport::{ConfigureCommandExt, TokioChildProcess},
        };
        use tokio::process::Command;
        use tokio::time::{Duration, sleep};

        println!("Starting MCP server process...");

        let client = ()
            .serve(TokioChildProcess::new(
                Command::new(&server_binary).configure(|cmd| {
                    if let Some(cfg) = &config_path {
                        cmd.arg("--config");
                        cmd.arg(cfg);
                    }

                    cmd.arg("serve");
                }),
            )?)
            .await?;

        // Get server info
        let server_info = client.peer_info();
        println!("Connected to server: {server_info:#?}");

        // List tools
        println!("\nListing available tools...");
        let tools = client.list_tools(Default::default()).await?;

        for tool in &tools.tools {
            println!(
                "  - {}: {}",
                tool.name,
                tool.description.as_deref().unwrap_or("No description")
            );
        }

        // Always call get_index_info first to verify semantic availability
        println!("\nCalling get_index_info tool...");
        let get_info_result = client
            .call_tool(CallToolRequestParams {
                meta: None,
                name: "get_index_info".into(),
                arguments: None,
                task: None,
            })
            .await?;
        Self::print_tool_output(&get_info_result);

        let repeat = repeat.max(1);

        // Optionally call a specific tool supplied by the user
        if let Some(tool_name) = tool {
            if let Some(delay) = delay_before_tool_secs {
                if delay > 0 {
                    println!("\nWaiting {delay} seconds before calling '{tool_name}'...");
                    sleep(Duration::from_secs(delay)).await;
                }
            }

            println!("\nCalling tool '{tool_name}' ({repeat} run(s))...");

            let parsed_args: Option<JsonObject> = if let Some(raw) = args.as_ref() {
                let value: Value = serde_json::from_str(raw)
                    .map_err(|e| anyhow!("Failed to parse --args as JSON object: {e}"))?;

                match value {
                    Value::Object(map) => Some(map),
                    _ => {
                        return Err(anyhow!(
                            "Tool arguments must be a JSON object (e.g. {{\"query\":\"test\"}})"
                        ));
                    }
                }
            } else {
                None
            };

            let mut durations_ms: Vec<f64> = Vec::with_capacity(repeat);
            for run_idx in 0..repeat {
                let started = Instant::now();
                let tool_result = client
                    .call_tool(CallToolRequestParams {
                        meta: None,
                        name: tool_name.clone().into(),
                        arguments: parsed_args.clone(),
                        task: None,
                    })
                    .await?;
                let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
                durations_ms.push(elapsed_ms);
                println!(
                    "\nRun {}/{} completed in {:.2} ms",
                    run_idx + 1,
                    repeat,
                    elapsed_ms
                );
                Self::print_tool_output(&tool_result);
            }

            if repeat > 1 {
                let mut sorted = durations_ms.clone();
                sorted.sort_by(|a, b| a.total_cmp(b));
                let min = sorted.first().copied().unwrap_or(0.0);
                let max = sorted.last().copied().unwrap_or(0.0);
                let median = if sorted.len() % 2 == 1 {
                    sorted[sorted.len() / 2]
                } else {
                    let hi = sorted.len() / 2;
                    let lo = hi.saturating_sub(1);
                    (sorted[lo] + sorted[hi]) / 2.0
                };
                let p95_idx = ((sorted.len() as f64 * 0.95).ceil() as usize)
                    .saturating_sub(1)
                    .min(sorted.len().saturating_sub(1));
                let p95 = sorted[p95_idx];
                println!(
                    "\nTiming summary (ms): min={min:.2}, median={median:.2}, p95={p95:.2}, max={max:.2}"
                );
            }
        }

        // Test custom requests
        println!("\n--- Testing Custom Requests ---");

        // Test index-stats custom request
        println!("\nSending custom request: requests/quarry/index-stats");
        let stats_request =
            ClientRequest::CustomRequest(CustomRequest::new("requests/quarry/index-stats", None));
        match client.peer().send_request(stats_request).await {
            Ok(rmcp::model::ServerResult::CustomResult(custom)) => {
                println!("Response: {}", serde_json::to_string_pretty(&custom.0)?);
            }
            Ok(other) => println!("Unexpected response type: {other:?}"),
            Err(e) => println!("Request failed: {e}"),
        }

        // Test force-reindex custom request (with a small path)
        println!("\nSending custom request: requests/quarry/force-reindex");
        let reindex_request = ClientRequest::CustomRequest(CustomRequest::new(
            "requests/quarry/force-reindex",
            Some(serde_json::json!({"paths": ["src/mcp/client.rs"]})),
        ));
        match client.peer().send_request(reindex_request).await {
            Ok(rmcp::model::ServerResult::CustomResult(custom)) => {
                println!("Response: {}", serde_json::to_string_pretty(&custom.0)?);
            }
            Ok(other) => println!("Unexpected response type: {other:?}"),
            Err(e) => println!("Request failed: {e}"),
        }

        println!("\n--- Custom Request Tests Complete ---");

        // Shutdown
        println!("\nShutting down...");
        client.cancel().await?;

        Ok(())
    }

    fn print_tool_output(result: &rmcp::model::CallToolResult) {
        println!("Result:");
        for annotated_content in &result.content {
            match &**annotated_content {
                rmcp::model::RawContent::Text(text) => println!("{}", text.text),
                _ => println!("(Non-text content)"),
            }
        }

        if result.is_error.unwrap_or(false) {
            println!("Tool returned an error status");
        }
    }
}
