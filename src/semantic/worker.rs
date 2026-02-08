//! Semantic embedding worker process client/server.
//!
//! Provides an optional out-of-process embedding path to isolate memory spikes.

use crate::config::SemanticBackend;
use crate::semantic::EmbeddingPool;
use crate::types::SymbolId;
use crate::vector::EmbeddingRuntimeConfig;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::thread;
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

const DEFAULT_MAX_RESTARTS: u32 = 5;

#[derive(Debug, Clone)]
pub struct SemanticWorkerClientConfig {
    pub model: String,
    pub backend: SemanticBackend,
    pub max_batch_tokens: usize,
    pub max_sequence_length: usize,
    pub runtime: EmbeddingRuntimeConfig,
    pub rss_limit_mb: usize,
    pub restart_backoff_ms: u64,
    pub max_restarts: u32,
}

impl SemanticWorkerClientConfig {
    pub fn with_defaults(
        model: String,
        backend: SemanticBackend,
        max_batch_tokens: usize,
        max_sequence_length: usize,
        runtime: EmbeddingRuntimeConfig,
        rss_limit_mb: usize,
        restart_backoff_ms: u64,
    ) -> Self {
        Self {
            model,
            backend,
            max_batch_tokens,
            max_sequence_length,
            runtime,
            rss_limit_mb,
            restart_backoff_ms,
            max_restarts: DEFAULT_MAX_RESTARTS,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkerEmbedItem {
    symbol_id: u32,
    text: String,
    language: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WorkerRequest {
    Init {
        model: String,
        backend: SemanticBackend,
        max_batch_tokens: usize,
        max_sequence_length: usize,
        runtime: EmbeddingRuntimeConfig,
    },
    EmbedBatch {
        batch_id: u64,
        items: Vec<WorkerEmbedItem>,
    },
    Flush,
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WorkerResponse {
    InitOk {
        dimensions: usize,
    },
    EmbedResult {
        batch_id: u64,
        items: Vec<WorkerEmbedding>,
    },
    FlushOk,
    Ack,
    Error {
        batch_id: Option<u64>,
        message: String,
        retryable: bool,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkerEmbedding {
    symbol_id: u32,
    embedding: Vec<f32>,
    language: String,
}

fn write_message<T: Serialize>(
    writer: &mut BufWriter<ChildStdin>,
    message: &T,
) -> Result<(), String> {
    let json = serde_json::to_string(message)
        .map_err(|e| format!("Failed to serialize worker message: {e}"))?;
    writer
        .write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write worker message: {e}"))?;
    writer
        .write_all(b"\n")
        .map_err(|e| format!("Failed to write worker delimiter: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("Failed to flush worker stdin: {e}"))?;
    Ok(())
}

fn read_message(reader: &mut BufReader<ChildStdout>) -> Result<WorkerResponse, String> {
    let mut line = String::new();
    let read = reader
        .read_line(&mut line)
        .map_err(|e| format!("Failed to read worker response: {e}"))?;
    if read == 0 {
        return Err("Worker process closed stdout".to_string());
    }
    serde_json::from_str::<WorkerResponse>(line.trim())
        .map_err(|e| format!("Invalid worker response JSON: {e}"))
}

fn child_rss_mb(pid_u32: u32) -> Option<usize> {
    let pid = Pid::from_u32(pid_u32);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    let process = sys.process(pid)?;
    Some((process.memory() / (1024 * 1024)) as usize)
}

pub struct SemanticWorkerClient {
    cfg: SemanticWorkerClientConfig,
    child: Child,
    writer: BufWriter<ChildStdin>,
    reader: BufReader<ChildStdout>,
    next_batch_id: u64,
    restart_count: u32,
}

impl SemanticWorkerClient {
    pub fn new(cfg: SemanticWorkerClientConfig) -> Result<Self, String> {
        let (child, mut writer, mut reader) = Self::spawn_worker_process()?;
        Self::send_init(&cfg, &mut writer, &mut reader)?;
        tracing::info!(
            target: "semantic",
            "Semantic worker started (pid={})",
            child.id()
        );
        Ok(Self {
            cfg,
            child,
            writer,
            reader,
            next_batch_id: 1,
            restart_count: 0,
        })
    }

    fn spawn_worker_process()
    -> Result<(Child, BufWriter<ChildStdin>, BufReader<ChildStdout>), String> {
        let exe = std::env::current_exe()
            .map_err(|e| format!("Failed to resolve current executable: {e}"))?;
        let mut command = Command::new(exe);
        command
            .env("CODANNA_SEMANTIC_WORKER", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = command
            .spawn()
            .map_err(|e| format!("Failed to start semantic worker process: {e}"))?;
        let child_stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to capture semantic worker stdin".to_string())?;
        let child_stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to capture semantic worker stdout".to_string())?;
        Ok((
            child,
            BufWriter::new(child_stdin),
            BufReader::new(child_stdout),
        ))
    }

    fn send_init(
        cfg: &SemanticWorkerClientConfig,
        writer: &mut BufWriter<ChildStdin>,
        reader: &mut BufReader<ChildStdout>,
    ) -> Result<(), String> {
        write_message(
            writer,
            &WorkerRequest::Init {
                model: cfg.model.clone(),
                backend: cfg.backend,
                max_batch_tokens: cfg.max_batch_tokens,
                max_sequence_length: cfg.max_sequence_length,
                runtime: cfg.runtime.clone(),
            },
        )?;
        match read_message(reader)? {
            WorkerResponse::InitOk { dimensions } => {
                tracing::debug!(
                    target: "semantic",
                    "Semantic worker initialized (dimensions={dimensions})"
                );
                Ok(())
            }
            WorkerResponse::Error { message, .. } => {
                Err(format!("Semantic worker init failed: {message}"))
            }
            other => Err(format!(
                "Unexpected semantic worker init response: {:?}",
                other
            )),
        }
    }

    fn restart_worker(&mut self, reason: &str) -> Result<(), String> {
        if self.restart_count >= self.cfg.max_restarts {
            return Err(format!(
                "Semantic worker restart limit reached ({}): {reason}",
                self.cfg.max_restarts
            ));
        }
        self.restart_count += 1;
        tracing::warn!(
            target: "semantic",
            "Restarting semantic worker (attempt {}/{}): {}",
            self.restart_count,
            self.cfg.max_restarts,
            reason
        );

        let _ = self.child.kill();
        let _ = self.child.wait();
        if self.cfg.restart_backoff_ms > 0 {
            thread::sleep(Duration::from_millis(self.cfg.restart_backoff_ms));
        }

        let (child, mut writer, mut reader) = Self::spawn_worker_process()?;
        Self::send_init(&self.cfg, &mut writer, &mut reader)?;
        self.child = child;
        self.writer = writer;
        self.reader = reader;
        Ok(())
    }

    fn enforce_rss_limit(&mut self) -> Result<(), String> {
        if self.cfg.rss_limit_mb == 0 {
            return Ok(());
        }
        let Some(rss_mb) = child_rss_mb(self.child.id()) else {
            return Ok(());
        };
        if rss_mb > self.cfg.rss_limit_mb {
            return self.restart_worker(&format!(
                "RSS {}MB exceeded limit {}MB",
                rss_mb, self.cfg.rss_limit_mb
            ));
        }
        Ok(())
    }

    fn embed_batch_once(
        &mut self,
        batch_id: u64,
        items: &[(SymbolId, &str, &str)],
    ) -> Result<Vec<(SymbolId, Vec<f32>, String)>, String> {
        let payload: Vec<WorkerEmbedItem> = items
            .iter()
            .map(|(id, text, language)| WorkerEmbedItem {
                symbol_id: id.to_u32(),
                text: (*text).to_string(),
                language: (*language).to_string(),
            })
            .collect();

        write_message(
            &mut self.writer,
            &WorkerRequest::EmbedBatch {
                batch_id,
                items: payload,
            },
        )?;

        match read_message(&mut self.reader)? {
            WorkerResponse::EmbedResult {
                batch_id: response_id,
                items,
            } => {
                if response_id != batch_id {
                    return Err(format!(
                        "Mismatched semantic worker batch id: expected {}, got {}",
                        batch_id, response_id
                    ));
                }
                let mut result = Vec::with_capacity(items.len());
                for item in items {
                    if let Some(symbol_id) = SymbolId::new(item.symbol_id) {
                        result.push((symbol_id, item.embedding, item.language));
                    }
                }
                Ok(result)
            }
            WorkerResponse::Error { message, .. } => Err(message),
            other => Err(format!("Unexpected semantic worker response: {:?}", other)),
        }
    }

    pub fn embed_parallel(
        &mut self,
        items: &[(SymbolId, &str, &str)],
    ) -> Result<Vec<(SymbolId, Vec<f32>, String)>, String> {
        if items.is_empty() {
            return Ok(Vec::new());
        }

        self.enforce_rss_limit()?;
        let batch_id = self.next_batch_id;
        self.next_batch_id = self.next_batch_id.saturating_add(1);

        match self.embed_batch_once(batch_id, items) {
            Ok(v) => Ok(v),
            Err(first_err) => {
                self.restart_worker(&format!("Embed request failed: {first_err}"))?;
                self.embed_batch_once(batch_id, items)
                    .map_err(|second_err| {
                        format!(
                            "Semantic worker failed after restart (first: {}; second: {})",
                            first_err, second_err
                        )
                    })
            }
        }
    }
}

impl Drop for SemanticWorkerClient {
    fn drop(&mut self) {
        let _ = write_message(&mut self.writer, &WorkerRequest::Shutdown);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn write_worker_stdout(
    writer: &mut std::io::StdoutLock<'_>,
    response: &WorkerResponse,
) -> Result<(), String> {
    let json = serde_json::to_string(response)
        .map_err(|e| format!("Failed to serialize worker response: {e}"))?;
    writer
        .write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write worker response: {e}"))?;
    writer
        .write_all(b"\n")
        .map_err(|e| format!("Failed to write worker response delimiter: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("Failed to flush worker stdout: {e}"))?;
    Ok(())
}

pub fn run_worker_stdio() -> Result<(), String> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    let mut pool: Option<EmbeddingPool> = None;

    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|e| format!("Worker failed to read stdin: {e}"))?;
        if read == 0 {
            break;
        }
        let request = match serde_json::from_str::<WorkerRequest>(line.trim()) {
            Ok(req) => req,
            Err(e) => {
                write_worker_stdout(
                    &mut writer,
                    &WorkerResponse::Error {
                        batch_id: None,
                        message: format!("Invalid worker request: {e}"),
                        retryable: false,
                    },
                )?;
                continue;
            }
        };

        match request {
            WorkerRequest::Init {
                model,
                backend,
                max_batch_tokens,
                max_sequence_length,
                runtime,
            } => match EmbeddingPool::new(
                1,
                &model,
                backend,
                max_batch_tokens.max(512),
                max_sequence_length.max(256),
                Some(runtime),
            ) {
                Ok(created_pool) => {
                    let dimensions = created_pool.dimensions();
                    pool = Some(created_pool);
                    write_worker_stdout(&mut writer, &WorkerResponse::InitOk { dimensions })?;
                }
                Err(e) => {
                    write_worker_stdout(
                        &mut writer,
                        &WorkerResponse::Error {
                            batch_id: None,
                            message: format!("Worker init error: {e}"),
                            retryable: false,
                        },
                    )?;
                }
            },
            WorkerRequest::EmbedBatch { batch_id, items } => {
                let Some(ref process_pool) = pool else {
                    write_worker_stdout(
                        &mut writer,
                        &WorkerResponse::Error {
                            batch_id: Some(batch_id),
                            message: "Worker not initialized".to_string(),
                            retryable: false,
                        },
                    )?;
                    continue;
                };
                let refs: Vec<_> = items
                    .iter()
                    .filter_map(|item| {
                        SymbolId::new(item.symbol_id)
                            .map(|id| (id, item.text.as_str(), item.language.as_str()))
                    })
                    .collect();
                let embeddings = process_pool.embed_parallel(&refs);
                let payload: Vec<WorkerEmbedding> = embeddings
                    .into_iter()
                    .map(|(id, embedding, language)| WorkerEmbedding {
                        symbol_id: id.to_u32(),
                        embedding,
                        language,
                    })
                    .collect();
                write_worker_stdout(
                    &mut writer,
                    &WorkerResponse::EmbedResult {
                        batch_id,
                        items: payload,
                    },
                )?;
            }
            WorkerRequest::Flush => {
                write_worker_stdout(&mut writer, &WorkerResponse::FlushOk)?;
            }
            WorkerRequest::Shutdown => {
                write_worker_stdout(&mut writer, &WorkerResponse::Ack)?;
                break;
            }
        }
    }

    Ok(())
}
