//! MCP direct tool invocation command.

use crate::Symbol;
use crate::config::Settings;
use crate::indexing::facade::IndexFacade;
use crate::io::args::parse_positional_args;
use fs2::FileExt;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;

// MCP tool JSON output structures
#[derive(Debug, Serialize)]
struct IndexInfo {
    symbol_count: usize,
    file_count: usize,
    relationship_count: usize,
    symbol_kinds: SymbolKindBreakdown,
    semantic_search: SemanticSearchInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    documents: Option<DocumentsInfo>,
}

#[derive(Debug, Serialize)]
struct DocumentsInfo {
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    collections: Option<Vec<CollectionInfo>>,
}

#[derive(Debug, Serialize)]
struct CollectionInfo {
    name: String,
    chunk_count: usize,
    file_count: usize,
}

#[derive(Debug, Serialize)]
struct SymbolKindBreakdown {
    functions: usize,
    methods: usize,
    structs: usize,
    traits: usize,
}

#[derive(Debug, Serialize)]
struct SemanticSearchInfo {
    enabled: bool,
    model_name: Option<String>,
    embeddings: Option<usize>,
    dimensions: Option<usize>,
    created: Option<String>,
    updated: Option<String>,
}

/// Flattened call/caller info combining symbol with call site metadata.
/// Avoids tuple waste like `[[symbol, null], ...]` in JSON output.
#[derive(Debug, Serialize)]
struct CallRelation {
    #[serde(flatten)]
    symbol: Symbol,
    /// Line number of the call site (1-indexed)
    #[serde(skip_serializing_if = "Option::is_none")]
    call_line: Option<u32>,
    /// Column of the call site
    #[serde(skip_serializing_if = "Option::is_none")]
    call_column: Option<u16>,
}

/// Symbol info extracted from search result for consistent JSON shape.
/// Matches the nested `symbol: {...}` pattern used by semantic_search_docs.
#[derive(Debug, Serialize)]
struct SymbolInfo {
    id: crate::types::SymbolId,
    name: String,
    kind: crate::types::SymbolKind,
    file_path: String,
    line: u32,
    column: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    doc_comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature: Option<String>,
    module_path: String,
}

/// Search result with nested symbol for consistent JSON output.
/// Standardizes on `symbol: {...}` rather than flat `symbol_id: ...`.
#[derive(Debug, Serialize)]
struct SearchSymbolResult {
    symbol: SymbolInfo,
    score: f32,
    highlights: Vec<crate::storage::tantivy::TextHighlight>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
}

impl From<crate::storage::tantivy::SearchResult> for SearchSymbolResult {
    fn from(sr: crate::storage::tantivy::SearchResult) -> Self {
        Self {
            symbol: SymbolInfo {
                id: sr.symbol_id,
                name: sr.name,
                kind: sr.kind,
                file_path: sr.file_path,
                line: sr.line,
                column: sr.column,
                doc_comment: sr.doc_comment,
                signature: sr.signature,
                module_path: sr.module_path,
            },
            score: sr.score,
            highlights: sr.highlights,
            context: sr.context,
        }
    }
}

fn mcp_query_lock_path(config: &Settings) -> PathBuf {
    if let Some(root) = config.workspace_root.as_deref() {
        root.join(&config.index_path).join(".mcp-query.lock")
    } else {
        config.index_path.join(".mcp-query.lock")
    }
}

/// Run the MCP direct tool invocation command.
pub async fn run(
    tool: String,
    positional: Vec<String>,
    args: Option<String>,
    json: bool,
    fields: Option<Vec<String>>,
    facade: IndexFacade,
    config: &Settings,
) {
    let lock_path = mcp_query_lock_path(config);
    if let Some(parent) = lock_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!(
                "Error: failed to create MCP lock directory '{}': {e}",
                parent.display()
            );
            std::process::exit(1);
        }
    }
    let _lock_file = match OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
    {
        Ok(file) => file,
        Err(e) => {
            eprintln!(
                "Error: failed to open MCP query lock file '{}': {e}",
                lock_path.display()
            );
            std::process::exit(1);
        }
    };
    if let Err(e) = _lock_file.try_lock_exclusive() {
        eprintln!(
            "Error: another 'codanna mcp' query is already running for this workspace (lock: '{}'). Wait for it to finish and retry. ({e})",
            lock_path.display()
        );
        std::process::exit(1);
    }

    // Build arguments from both positional and --args
    let mut arguments = if let Some(args_str) = &args {
        // Parse JSON arguments if provided (backward compatibility)
        match serde_json::from_str::<serde_json::Value>(args_str) {
            Ok(serde_json::Value::Object(map)) => Some(map),
            Ok(_) => {
                eprintln!("Error: Arguments must be a JSON object");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("Error parsing arguments: {e}");
                std::process::exit(1);
            }
        }
    } else {
        // Start with empty map if no --args
        Some(serde_json::Map::new())
    };

    // Process positional arguments using unified parser
    if !positional.is_empty() {
        if let Some(ref mut args_map) = arguments {
            // Use the unified parser from args.rs
            let (first_positional, params) = parse_positional_args(&positional);

            // Handle the first positional argument based on tool type
            if let Some(pos_arg) = first_positional {
                match tool.as_str() {
                    "find_symbol" => {
                        args_map.insert(
                            "name".to_string(),
                            serde_json::Value::String(pos_arg.clone()),
                        );
                    }
                    "get_calls" | "find_callers" => {
                        args_map.insert(
                            "function_name".to_string(),
                            serde_json::Value::String(pos_arg.clone()),
                        );
                    }
                    "analyze_impact" => {
                        args_map.insert(
                            "symbol_name".to_string(),
                            serde_json::Value::String(pos_arg.clone()),
                        );
                    }
                    "get_source"
                    | "get_type_fields"
                    | "get_state_graph"
                    | "get_feature_context"
                    | "get_call_tree" => {
                        args_map.insert(
                            "symbol_id".to_string(),
                            serde_json::Value::Number(pos_arg.parse::<u64>().unwrap_or(0).into()),
                        );
                    }
                    "get_module_exports" => {
                        args_map.insert(
                            "file_path".to_string(),
                            serde_json::Value::String(pos_arg.clone()),
                        );
                    }
                    "semantic_search_docs"
                    | "semantic_search_chunks"
                    | "semantic_search_with_context"
                    | "search_documents" => {
                        args_map.insert(
                            "query".to_string(),
                            serde_json::Value::String(pos_arg.clone()),
                        );
                    }
                    "search_symbols" => {
                        args_map.insert(
                            "query".to_string(),
                            serde_json::Value::String(pos_arg.clone()),
                        );
                    }
                    _ => {
                        eprintln!("Warning: Unknown tool '{tool}', ignoring positional argument");
                    }
                }
            }

            // Special handling: find_symbol supports symbol_id:XXX as positional
            // If symbol_id is in params but name wasn't set, use it as the name
            if tool == "find_symbol" && !args_map.contains_key("name") {
                if let Some(id) = params.get("symbol_id") {
                    args_map.insert(
                        "name".to_string(),
                        serde_json::Value::String(format!("symbol_id:{id}")),
                    );
                }
            }

            // Add all key:value pairs from params
            for (key, value) in params {
                // Try to parse as number first, then boolean, fallback to string
                let json_value = if let Ok(n) = value.parse::<i64>() {
                    serde_json::Value::Number(n.into())
                } else if let Ok(f) = value.parse::<f64>() {
                    serde_json::Value::Number(
                        serde_json::Number::from_f64(f)
                            .unwrap_or_else(|| serde_json::Number::from(0)),
                    )
                } else if let Ok(b) = value.parse::<bool>() {
                    serde_json::Value::Bool(b)
                } else {
                    serde_json::Value::String(value)
                };
                args_map.insert(key, json_value);
            }
        }
    }

    // Convert to Option<Map> only if we have arguments
    let arguments = arguments.filter(|map| !map.is_empty());

    // Collect data for find_symbol if JSON output is requested
    let find_symbol_data = if json && tool == "find_symbol" {
        let name = arguments
            .as_ref()
            .and_then(|m| m.get("name"))
            .and_then(|v| v.as_str());
        let language = arguments
            .as_ref()
            .and_then(|m| m.get("lang"))
            .and_then(|v| v.as_str());

        if let Some(symbol_name) = name {
            let symbols = facade.find_symbols_by_name(symbol_name, language);
            if !symbols.is_empty() {
                use crate::symbol::context::ContextIncludes;
                let mut results = Vec::new();

                for symbol in symbols {
                    // Get full context with callers using the same approach as MCP
                    let context = facade.get_symbol_context(
                        symbol.id,
                        ContextIncludes::CALLERS
                            | ContextIncludes::IMPLEMENTATIONS
                            | ContextIncludes::DEFINITIONS,
                    );

                    // Build result with context if available
                    if let Some(ctx) = context {
                        results.push(ctx);
                    } else {
                        // Fallback: create minimal context
                        let file_path = facade
                            .get_file_path(symbol.file_id)
                            .unwrap_or_else(|| "unknown".to_string());

                        results.push(crate::symbol::context::SymbolContext {
                            symbol,
                            file_path,
                            relationships: Default::default(),
                        });
                    }
                }
                Some(results)
            } else {
                Some(Vec::new())
            }
        } else {
            None
        }
    } else {
        None
    };

    // Collect data for get_calls if JSON output is requested
    let get_calls_data = if json && tool == "get_calls" {
        let symbol_id = arguments
            .as_ref()
            .and_then(|m| m.get("symbol_id"))
            .and_then(|v| v.as_u64())
            .map(|id| id as u32);
        let function_name = arguments
            .as_ref()
            .and_then(|m| m.get("function_name"))
            .and_then(|v| v.as_str());
        let language = arguments
            .as_ref()
            .and_then(|m| m.get("lang"))
            .and_then(|v| v.as_str());

        if let Some(id) = symbol_id {
            use crate::symbol::context::ContextIncludes;

            // Direct lookup by symbol ID
            if let Some(symbol) = facade.get_symbol(crate::SymbolId(id)) {
                let mut all_calls = Vec::new();

                let context = facade.get_symbol_context(symbol.id, ContextIncludes::CALLS);
                if let Some(ctx) = context {
                    if let Some(calls) = ctx.relationships.calls {
                        for (called, metadata) in calls {
                            all_calls.push(CallRelation {
                                symbol: called,
                                call_line: metadata.as_ref().and_then(|m| m.line).map(|l| l + 1),
                                call_column: metadata.as_ref().and_then(|m| m.column),
                            });
                        }
                    }
                }

                Some(all_calls)
            } else {
                None // Symbol not found
            }
        } else if let Some(func_name) = function_name {
            use crate::symbol::context::ContextIncludes;
            use std::collections::HashSet;

            // Find ALL symbols with this name
            let symbols = facade.find_symbols_by_name(func_name, language);
            let function_symbols: Vec<_> = symbols
                .into_iter()
                .filter(|s| {
                    matches!(
                        s.kind,
                        crate::SymbolKind::Function | crate::SymbolKind::Method
                    )
                })
                .collect();

            if function_symbols.is_empty() {
                None // Function not found
            } else {
                // Aggregate calls from ALL symbols with this name (same as MCP handler)
                let mut all_calls = Vec::new();
                let mut seen_ids = HashSet::new();

                for symbol in function_symbols {
                    let context = facade.get_symbol_context(symbol.id, ContextIncludes::CALLS);
                    if let Some(ctx) = context {
                        if let Some(calls) = ctx.relationships.calls {
                            for (called, metadata) in calls {
                                // Deduplicate by symbol ID
                                if seen_ids.insert(called.id) {
                                    all_calls.push(CallRelation {
                                        symbol: called,
                                        call_line: metadata
                                            .as_ref()
                                            .and_then(|m| m.line)
                                            .map(|l| l + 1),
                                        call_column: metadata.as_ref().and_then(|m| m.column),
                                    });
                                }
                            }
                        }
                    }
                }

                Some(all_calls)
            }
        } else {
            None
        }
    } else {
        None
    };

    // Collect data for find_callers if JSON output is requested
    let find_callers_data = if json && tool == "find_callers" {
        let symbol_id = arguments
            .as_ref()
            .and_then(|m| m.get("symbol_id"))
            .and_then(|v| v.as_u64())
            .map(|id| id as u32);
        let function_name = arguments
            .as_ref()
            .and_then(|m| m.get("function_name"))
            .and_then(|v| v.as_str());
        let language = arguments
            .as_ref()
            .and_then(|m| m.get("lang"))
            .and_then(|v| v.as_str());

        if let Some(id) = symbol_id {
            // Direct lookup by symbol ID
            if let Some(symbol) = facade.get_symbol(crate::SymbolId(id)) {
                let callers = facade.get_calling_functions_with_metadata(symbol.id);
                let all_callers: Vec<_> = callers
                    .into_iter()
                    .map(|(caller, metadata)| CallRelation {
                        symbol: caller,
                        call_line: metadata.as_ref().and_then(|m| m.line).map(|l| l + 1),
                        call_column: metadata.as_ref().and_then(|m| m.column),
                    })
                    .collect();
                Some(all_callers)
            } else {
                None // Symbol not found
            }
        } else if let Some(func_name) = function_name {
            use std::collections::HashSet;

            // Find all functions with this name
            let symbols = facade.find_symbols_by_name(func_name, language);
            if !symbols.is_empty() {
                let mut all_callers = Vec::new();
                let mut seen_ids = HashSet::new();

                // Check all symbols with this name and deduplicate (same as MCP handler)
                for symbol in &symbols {
                    let callers = facade.get_calling_functions_with_metadata(symbol.id);
                    for (caller, metadata) in callers {
                        // Deduplicate by symbol ID
                        if seen_ids.insert(caller.id) {
                            all_callers.push(CallRelation {
                                symbol: caller,
                                call_line: metadata.as_ref().and_then(|m| m.line).map(|l| l + 1),
                                call_column: metadata.as_ref().and_then(|m| m.column),
                            });
                        }
                    }
                }

                Some(all_callers)
            } else {
                None // Function not found
            }
        } else {
            None
        }
    } else {
        None
    };

    // Collect data for analyze_impact if JSON output is requested
    let analyze_impact_data = if json && tool == "analyze_impact" {
        let symbol_id = arguments
            .as_ref()
            .and_then(|m| m.get("symbol_id"))
            .and_then(|v| v.as_u64())
            .map(|id| id as u32);
        let symbol_name = arguments
            .as_ref()
            .and_then(|m| m.get("symbol_name"))
            .and_then(|v| v.as_str());
        let language = arguments
            .as_ref()
            .and_then(|m| m.get("lang"))
            .and_then(|v| v.as_str());

        if let Some(id) = symbol_id {
            // Direct lookup by symbol ID
            if let Some(symbol) = facade.get_symbol(crate::SymbolId(id)) {
                let max_depth = arguments
                    .as_ref()
                    .and_then(|m| m.get("max_depth"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(3) as usize;

                let impacted_ids = facade.get_impact_radius(symbol.id, Some(max_depth));

                // Convert SymbolIds to full Symbols
                let mut impacted_symbols = Vec::new();
                for impact_id in impacted_ids {
                    if let Some(sym) = facade.get_symbol(impact_id) {
                        impacted_symbols.push(sym);
                    }
                }

                Some(impacted_symbols)
            } else {
                None // Symbol not found
            }
        } else if let Some(sym_name) = symbol_name {
            use std::collections::HashSet;

            // Find ALL symbols with this name (same as MCP handler)
            let symbols = facade.find_symbols_by_name(sym_name, language);

            if symbols.is_empty() {
                None // Symbol not found
            } else {
                let max_depth = arguments
                    .as_ref()
                    .and_then(|m| m.get("max_depth"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(3) as usize;

                // Aggregate impact from ALL symbols with this name (same as MCP handler)
                let mut all_impacted_ids = HashSet::new();
                for symbol in &symbols {
                    let impacted_ids = facade.get_impact_radius(symbol.id, Some(max_depth));
                    all_impacted_ids.extend(impacted_ids);
                }

                // Convert SymbolIds to full Symbols
                let mut impacted_symbols = Vec::new();
                for id in all_impacted_ids {
                    if let Some(sym) = facade.get_symbol(id) {
                        impacted_symbols.push(sym);
                    }
                }

                Some(impacted_symbols)
            }
        } else {
            None
        }
    } else {
        None
    };

    // Collect data for search_symbols if JSON output is requested
    let search_symbols_data = if json && tool == "search_symbols" {
        let query = arguments
            .as_ref()
            .and_then(|m| m.get("query"))
            .and_then(|v| v.as_str());

        if let Some(q) = query {
            let limit = arguments
                .as_ref()
                .and_then(|m| m.get("limit"))
                .and_then(|v| v.as_u64())
                .unwrap_or(10) as u32;
            let kind = arguments
                .as_ref()
                .and_then(|m| m.get("kind"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let module = arguments
                .as_ref()
                .and_then(|m| m.get("module"))
                .and_then(|v| v.as_str());
            let language = arguments
                .as_ref()
                .and_then(|m| m.get("lang"))
                .and_then(|v| v.as_str());

            // Parse the kind filter if provided
            let kind_filter = kind.as_ref().and_then(|k| match k.to_lowercase().as_str() {
                "function" => Some(crate::SymbolKind::Function),
                "struct" => Some(crate::SymbolKind::Struct),
                "trait" => Some(crate::SymbolKind::Trait),
                "method" => Some(crate::SymbolKind::Method),
                "field" => Some(crate::SymbolKind::Field),
                "module" => Some(crate::SymbolKind::Module),
                "constant" => Some(crate::SymbolKind::Constant),
                _ => None,
            });

            match facade.search(q, limit as usize, kind_filter, module, language) {
                Ok(results) => Some(results),
                Err(_) => Some(Vec::new()),
            }
        } else {
            None
        }
    } else {
        None
    };

    // Collect data for semantic_search_docs if JSON output is requested
    #[derive(serde::Serialize)]
    struct SemanticSearchResult {
        symbol: Symbol,
        score: f32,
    }

    /// Context without the symbol (avoids duplication since symbol is at top level)
    #[derive(serde::Serialize)]
    struct ContextWithoutSymbol {
        file_path: String,
        relationships: crate::symbol::context::SymbolRelationships,
    }

    #[derive(serde::Serialize)]
    struct SemanticSearchWithContextResult {
        symbol: Symbol,
        score: f32,
        context: ContextWithoutSymbol,
    }

    // Get guidance config before moving indexer
    let guidance_config = facade.settings().guidance.clone();

    let semantic_search_docs_data = if json && tool == "semantic_search_docs" {
        if !facade.has_semantic_search() {
            None // Semantic search not enabled
        } else {
            let query = arguments
                .as_ref()
                .and_then(|m| m.get("query"))
                .and_then(|v| v.as_str());

            if let Some(q) = query {
                let limit = arguments
                    .as_ref()
                    .and_then(|m| m.get("limit"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10) as usize;
                let threshold = arguments
                    .as_ref()
                    .and_then(|m| m.get("threshold"))
                    .and_then(|v| v.as_f64())
                    .map(|t| t as f32);
                let language = arguments
                    .as_ref()
                    .and_then(|m| m.get("lang"))
                    .and_then(|v| v.as_str());

                let results = match threshold {
                    Some(t) => facade
                        .semantic_search_docs_with_threshold_and_language(q, limit, t, language),
                    None => facade.semantic_search_docs_with_language(q, limit, language),
                };

                match results {
                    Ok(results) => {
                        let semantic_results: Vec<SemanticSearchResult> = results
                            .into_iter()
                            .map(|(symbol, score)| SemanticSearchResult { symbol, score })
                            .collect();
                        Some(semantic_results)
                    }
                    Err(_) => Some(Vec::new()),
                }
            } else {
                None
            }
        }
    } else {
        None
    };

    // Collect data for semantic_search_chunks if JSON output is requested
    let semantic_search_chunks_data = if json && tool == "semantic_search_chunks" {
        let query = arguments
            .as_ref()
            .and_then(|m| m.get("query"))
            .and_then(|v| v.as_str());

        if let Some(q) = query {
            let limit = arguments
                .as_ref()
                .and_then(|m| m.get("limit"))
                .and_then(|v| v.as_u64())
                .unwrap_or(10) as usize;
            let threshold = arguments
                .as_ref()
                .and_then(|m| m.get("threshold"))
                .and_then(|v| v.as_f64())
                .map(|t| t as f32);
            let language = arguments
                .as_ref()
                .and_then(|m| m.get("lang"))
                .and_then(|v| v.as_str());

            let results = match threshold {
                Some(t) => {
                    facade.hybrid_chunk_search_with_threshold_detailed(q, limit, t, language)
                }
                None => facade.hybrid_chunk_search_detailed(q, limit, language),
            };

            match results {
                Ok(outcome) => Some(outcome),
                Err(_) => Some(crate::indexing::facade::ChunkSearchOutcome::default()),
            }
        } else {
            None
        }
    } else {
        None
    };

    // Collect data for semantic_search_with_context if JSON output is requested
    let semantic_search_with_context_data = if json && tool == "semantic_search_with_context" {
        if !facade.has_semantic_search() {
            None // Semantic search not enabled
        } else {
            let query = arguments
                .as_ref()
                .and_then(|m| m.get("query"))
                .and_then(|v| v.as_str());

            if let Some(q) = query {
                let limit = arguments
                    .as_ref()
                    .and_then(|m| m.get("limit"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(5) as u32; // Default 5 for context version
                let threshold = arguments
                    .as_ref()
                    .and_then(|m| m.get("threshold"))
                    .and_then(|v| v.as_f64())
                    .map(|t| t as f32);
                let language = arguments
                    .as_ref()
                    .and_then(|m| m.get("lang"))
                    .and_then(|v| v.as_str());

                let search_results = match threshold {
                    Some(t) => facade.semantic_search_docs_with_threshold_and_language(
                        q,
                        limit as usize,
                        t,
                        language,
                    ),
                    None => facade.semantic_search_docs_with_language(q, limit as usize, language),
                };

                match search_results {
                    Ok(results) => {
                        use crate::symbol::context::ContextIncludes;
                        let context_results: Vec<SemanticSearchWithContextResult> = results
                            .into_iter()
                            .filter_map(|(symbol, score)| {
                                // Get full context for each symbol
                                let context = facade.get_symbol_context(
                                    symbol.id,
                                    ContextIncludes::CALLERS
                                        | ContextIncludes::CALLS
                                        | ContextIncludes::IMPLEMENTATIONS
                                        | ContextIncludes::DEFINITIONS,
                                );

                                context.map(|ctx| SemanticSearchWithContextResult {
                                    symbol,
                                    score,
                                    context: ContextWithoutSymbol {
                                        file_path: ctx.file_path,
                                        relationships: ctx.relationships,
                                    },
                                })
                            })
                            .collect();
                        Some(context_results)
                    }
                    Err(_) => Some(Vec::new()),
                }
            } else {
                None
            }
        }
    } else {
        None
    };

    // Check semantic search status before moving indexer
    let has_semantic_search = facade.has_semantic_search();

    // Only load document store for tools that need it (search_documents)
    // This is expensive (~1s to load ML model) so we skip it for other tools
    let needs_document_store = tool == "search_documents";
    let document_store = if needs_document_store {
        crate::documents::load_from_settings(config)
    } else {
        None
    };

    // If we need JSON output for get_index_info, collect data before moving indexer
    let index_info_data = if json && tool == "get_index_info" {
        let symbol_count = facade.symbol_count();
        let file_count = facade.file_count();
        let relationship_count = facade.relationship_count();

        // Count symbols by kind
        let mut kind_counts = std::collections::HashMap::new();
        for symbol in facade.get_all_symbols() {
            *kind_counts.entry(symbol.kind).or_insert(0) += 1;
        }

        let functions = *kind_counts.get(&crate::SymbolKind::Function).unwrap_or(&0);
        let methods = *kind_counts.get(&crate::SymbolKind::Method).unwrap_or(&0);
        let structs = *kind_counts.get(&crate::SymbolKind::Struct).unwrap_or(&0);
        let traits = *kind_counts.get(&crate::SymbolKind::Trait).unwrap_or(&0);

        // Get semantic search info
        let semantic_search = if let Some(metadata) = facade.get_semantic_metadata() {
            SemanticSearchInfo {
                enabled: true,
                model_name: Some(metadata.model_name),
                embeddings: Some(metadata.embedding_count),
                dimensions: Some(metadata.dimension),
                created: Some(crate::mcp::format_relative_time(metadata.created_at)),
                updated: Some(crate::mcp::format_relative_time(metadata.updated_at)),
            }
        } else {
            SemanticSearchInfo {
                enabled: false,
                model_name: None,
                embeddings: None,
                dimensions: None,
                created: None,
                updated: None,
            }
        };

        // Document collections info is skipped for performance
        // Loading DocumentStore requires ML model (~1s) which defeats fast index info
        // TODO: Add fast stats-only document store loader
        let documents: Option<DocumentsInfo> = None;

        Some(IndexInfo {
            symbol_count,
            file_count: file_count as usize,
            relationship_count,
            symbol_kinds: SymbolKindBreakdown {
                functions,
                methods,
                structs,
                traits,
            },
            semantic_search,
            documents,
        })
    } else {
        None
    };

    // Pre-collect search_documents data for JSON output
    let search_documents_data = if json && tool == "search_documents" {
        if let Some(ref store_arc) = document_store {
            let query = arguments
                .as_ref()
                .and_then(|m| m.get("query"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let collection = arguments
                .as_ref()
                .and_then(|m| m.get("collection"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let limit = arguments
                .as_ref()
                .and_then(|m| m.get("limit"))
                .and_then(|v| v.as_u64())
                .unwrap_or(5) as usize;

            let mut store = store_arc.write().await;
            let search_query = crate::documents::SearchQuery {
                text: query.clone(),
                collection,
                document: None,
                limit,
                preview_config: Some(config.documents.search.clone()),
            };

            match store.search(search_query) {
                Ok(results) => Some((query, results)),
                Err(_) => Some((query, Vec::new())),
            }
        } else {
            None
        }
    } else {
        None
    };

    // Embedded mode - use already loaded facade directly
    let server = {
        let server = crate::mcp::CodeIntelligenceServer::new(facade);

        // Add DocumentStore if documents are enabled and indexed
        if let Some(store_arc) = document_store {
            server.with_document_store_arc(store_arc)
        } else {
            server
        }
    };

    // Call the tool directly
    use crate::mcp::*;
    use rmcp::handler::server::wrapper::Parameters;

    let result = match tool.as_str() {
        "find_symbol" => {
            let name = arguments
                .as_ref()
                .and_then(|m| m.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    eprintln!("Error: find_symbol requires 'name' parameter");
                    std::process::exit(1);
                });
            let lang = arguments
                .as_ref()
                .and_then(|m| m.get("lang"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            server
                .find_symbol(Parameters(FindSymbolRequest {
                    name: name.to_string(),
                    lang,
                }))
                .await
        }
        "get_calls" => {
            let function_name = arguments
                .as_ref()
                .and_then(|m| m.get("function_name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let symbol_id = arguments
                .as_ref()
                .and_then(|m| m.get("symbol_id"))
                .and_then(|v| v.as_u64())
                .map(|id| id as u32);

            // Require either function_name or symbol_id
            if function_name.is_none() && symbol_id.is_none() {
                eprintln!(
                    "Error: get_calls requires either 'function_name' or 'symbol_id' parameter"
                );
                std::process::exit(1);
            }

            server
                .get_calls(Parameters(GetCallsRequest {
                    function_name,
                    symbol_id,
                }))
                .await
        }
        "find_callers" => {
            let function_name = arguments
                .as_ref()
                .and_then(|m| m.get("function_name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let symbol_id = arguments
                .as_ref()
                .and_then(|m| m.get("symbol_id"))
                .and_then(|v| v.as_u64())
                .map(|id| id as u32);

            // Require either function_name or symbol_id
            if function_name.is_none() && symbol_id.is_none() {
                eprintln!(
                    "Error: find_callers requires either 'function_name' or 'symbol_id' parameter"
                );
                std::process::exit(1);
            }

            server
                .find_callers(Parameters(FindCallersRequest {
                    function_name,
                    symbol_id,
                }))
                .await
        }
        "analyze_impact" => {
            let symbol_name = arguments
                .as_ref()
                .and_then(|m| m.get("symbol_name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let symbol_id = arguments
                .as_ref()
                .and_then(|m| m.get("symbol_id"))
                .and_then(|v| v.as_u64())
                .map(|id| id as u32);

            // Require either symbol_name or symbol_id
            if symbol_name.is_none() && symbol_id.is_none() {
                eprintln!(
                    "Error: analyze_impact requires either 'symbol_name' or 'symbol_id' parameter"
                );
                std::process::exit(1);
            }

            let max_depth = arguments
                .as_ref()
                .and_then(|m| m.get("max_depth"))
                .and_then(|v| v.as_u64())
                .unwrap_or(3) as u32;
            server
                .analyze_impact(Parameters(AnalyzeImpactRequest {
                    symbol_name,
                    symbol_id,
                    max_depth,
                }))
                .await
        }
        "get_index_info" => {
            use crate::mcp::GetIndexInfoRequest;
            use rmcp::handler::server::wrapper::Parameters;
            server
                .get_index_info(Parameters(GetIndexInfoRequest {}))
                .await
        }
        "search_symbols" => {
            let query = arguments
                .as_ref()
                .and_then(|m| m.get("query"))
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    eprintln!("Error: search_symbols requires 'query' parameter");
                    std::process::exit(1);
                });
            let limit = arguments
                .as_ref()
                .and_then(|m| m.get("limit"))
                .and_then(|v| v.as_u64())
                .unwrap_or(10) as u32;
            let kind = arguments
                .as_ref()
                .and_then(|m| m.get("kind"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let module = arguments
                .as_ref()
                .and_then(|m| m.get("module"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let lang = arguments
                .as_ref()
                .and_then(|m| m.get("lang"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            server
                .search_symbols(Parameters(SearchSymbolsRequest {
                    query: query.to_string(),
                    limit,
                    kind,
                    module,
                    lang,
                }))
                .await
        }
        "semantic_search_docs" => {
            let query = arguments
                .as_ref()
                .and_then(|m| m.get("query"))
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    eprintln!("Error: semantic_search_docs requires 'query' parameter");
                    std::process::exit(1);
                });
            let limit = arguments
                .as_ref()
                .and_then(|m| m.get("limit"))
                .and_then(|v| v.as_u64())
                .unwrap_or(10) as u32;
            let threshold = arguments
                .as_ref()
                .and_then(|m| m.get("threshold"))
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let lang = arguments
                .as_ref()
                .and_then(|m| m.get("lang"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            server
                .semantic_search_docs(Parameters(SemanticSearchRequest {
                    query: query.to_string(),
                    limit,
                    threshold,
                    lang,
                }))
                .await
        }
        "semantic_search_chunks" => {
            let query = arguments
                .as_ref()
                .and_then(|m| m.get("query"))
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    eprintln!("Error: semantic_search_chunks requires 'query' parameter");
                    std::process::exit(1);
                });
            let limit = arguments
                .as_ref()
                .and_then(|m| m.get("limit"))
                .and_then(|v| v.as_u64())
                // One source of truth: a hardcoded copy here would silently keep the CLI on
                // the old default while the MCP tool moved on.
                .unwrap_or(crate::mcp::default_chunk_limit() as u64) as u32;
            let threshold = arguments
                .as_ref()
                .and_then(|m| m.get("threshold"))
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let lang = arguments
                .as_ref()
                .and_then(|m| m.get("lang"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            server
                .semantic_search_chunks(Parameters(crate::mcp::ChunkSearchRequest {
                    query: query.to_string(),
                    limit,
                    threshold,
                    lang,
                }))
                .await
        }
        "semantic_search_with_context" => {
            let query = arguments
                .as_ref()
                .and_then(|m| m.get("query"))
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    eprintln!("Error: semantic_search_with_context requires 'query' parameter");
                    std::process::exit(1);
                });
            let limit = arguments
                .as_ref()
                .and_then(|m| m.get("limit"))
                .and_then(|v| v.as_u64())
                .unwrap_or(5) as u32;
            let threshold = arguments
                .as_ref()
                .and_then(|m| m.get("threshold"))
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let lang = arguments
                .as_ref()
                .and_then(|m| m.get("lang"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            server
                .semantic_search_with_context(Parameters(SemanticSearchWithContextRequest {
                    query: query.to_string(),
                    limit,
                    threshold,
                    lang,
                }))
                .await
        }
        "get_source" => {
            use crate::mcp::GetSourceRequest;
            let symbol_id = arguments
                .as_ref()
                .and_then(|m| m.get("symbol_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or_else(|| {
                    eprintln!("Error: get_source requires 'symbol_id' parameter");
                    std::process::exit(1);
                }) as u32;
            let context_lines = arguments
                .as_ref()
                .and_then(|m| m.get("context_lines"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            server
                .get_source(Parameters(GetSourceRequest {
                    symbol_id,
                    context_lines,
                }))
                .await
        }
        "get_module_exports" => {
            use crate::mcp::GetModuleExportsRequest;
            let file_path = arguments
                .as_ref()
                .and_then(|m| m.get("file_path"))
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    eprintln!("Error: get_module_exports requires 'file_path' parameter");
                    std::process::exit(1);
                })
                .to_string();
            server
                .get_module_exports(Parameters(GetModuleExportsRequest { file_path }))
                .await
        }
        "get_type_fields" => {
            use crate::mcp::GetTypeFieldsRequest;
            let symbol_id = arguments
                .as_ref()
                .and_then(|m| m.get("symbol_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or_else(|| {
                    eprintln!("Error: get_type_fields requires 'symbol_id' parameter");
                    std::process::exit(1);
                }) as u32;
            server
                .get_type_fields(Parameters(GetTypeFieldsRequest { symbol_id }))
                .await
        }
        "get_state_graph" => {
            use crate::mcp::GetStateGraphRequest;
            let symbol_id = arguments
                .as_ref()
                .and_then(|m| m.get("symbol_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or_else(|| {
                    eprintln!("Error: get_state_graph requires 'symbol_id' parameter");
                    std::process::exit(1);
                }) as u32;
            server
                .get_state_graph(Parameters(GetStateGraphRequest { symbol_id }))
                .await
        }
        "get_feature_context" => {
            use crate::mcp::GetFeatureContextRequest;

            let symbol_id = arguments
                .as_ref()
                .and_then(|m| m.get("symbol_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or_else(|| {
                    eprintln!("Error: get_feature_context requires 'symbol_id' parameter");
                    std::process::exit(1);
                }) as u32;

            let include_source = arguments
                .as_ref()
                .and_then(|m| m.get("include_source"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let include_impact = arguments
                .as_ref()
                .and_then(|m| m.get("include_impact"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let show_call_examples = arguments
                .as_ref()
                .and_then(|m| m.get("show_call_examples"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let max_callers = arguments
                .as_ref()
                .and_then(|m| m.get("max_callers"))
                .and_then(|v| v.as_u64())
                .unwrap_or(10) as u32;

            let max_calls = arguments
                .as_ref()
                .and_then(|m| m.get("max_calls"))
                .and_then(|v| v.as_u64())
                .unwrap_or(10) as u32;

            let max_impact = arguments
                .as_ref()
                .and_then(|m| m.get("max_impact"))
                .and_then(|v| v.as_u64())
                .unwrap_or(20) as u32;

            let impact_depth = arguments
                .as_ref()
                .and_then(|m| m.get("impact_depth"))
                .and_then(|v| v.as_u64())
                .unwrap_or(2) as u32;

            let max_examples = arguments
                .as_ref()
                .and_then(|m| m.get("max_examples"))
                .and_then(|v| v.as_u64())
                .unwrap_or(3) as u32;

            let context_lines = arguments
                .as_ref()
                .and_then(|m| m.get("context_lines"))
                .and_then(|v| v.as_u64())
                .unwrap_or(5) as u32;

            server
                .get_feature_context(rmcp::handler::server::wrapper::Parameters(
                    GetFeatureContextRequest {
                        symbol_id,
                        include_source,
                        include_impact,
                        show_call_examples,
                        max_callers,
                        max_calls,
                        max_impact,
                        impact_depth,
                        max_examples,
                        context_lines,
                    },
                ))
                .await
        }
        "get_call_tree" => {
            use crate::mcp::GetCallTreeRequest;

            let symbol_id = arguments
                .as_ref()
                .and_then(|m| m.get("symbol_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or_else(|| {
                    eprintln!("Error: get_call_tree requires 'symbol_id' parameter");
                    std::process::exit(1);
                }) as u32;

            let max_depth = arguments
                .as_ref()
                .and_then(|m| m.get("max_depth"))
                .and_then(|v| v.as_u64())
                .unwrap_or(4) as u32;

            let include_source = arguments
                .as_ref()
                .and_then(|m| m.get("include_source"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let include_metadata = arguments
                .as_ref()
                .and_then(|m| m.get("include_metadata"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let show_external_calls = arguments
                .as_ref()
                .and_then(|m| m.get("show_external_calls"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let max_nodes = arguments
                .as_ref()
                .and_then(|m| m.get("max_nodes"))
                .and_then(|v| v.as_u64())
                .unwrap_or(100) as u32;

            let collapse_duplicates = arguments
                .as_ref()
                .and_then(|m| m.get("collapse_duplicates"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let show_trivial = arguments
                .as_ref()
                .and_then(|m| m.get("show_trivial"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            server
                .get_call_tree(rmcp::handler::server::wrapper::Parameters(
                    GetCallTreeRequest {
                        symbol_id,
                        max_depth,
                        include_source,
                        include_metadata,
                        show_external_calls,
                        max_nodes,
                        collapse_duplicates,
                        show_trivial,
                    },
                ))
                .await
        }
        "search_documents" => {
            use crate::mcp::SearchDocumentsRequest;
            let query = arguments
                .as_ref()
                .and_then(|m| m.get("query"))
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    eprintln!("Error: search_documents requires 'query' parameter");
                    std::process::exit(1);
                })
                .to_string();
            let collection = arguments
                .as_ref()
                .and_then(|m| m.get("collection"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let limit = arguments
                .as_ref()
                .and_then(|m| m.get("limit"))
                .and_then(|v| v.as_u64())
                .unwrap_or(5) as u32;
            server
                .search_documents(Parameters(SearchDocumentsRequest {
                    query,
                    collection,
                    limit,
                }))
                .await
        }
        "get_project_overview" => {
            use crate::mcp::GetProjectOverviewRequest;

            let include_graph = arguments
                .as_ref()
                .and_then(|m| m.get("include_graph"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let include_dependencies = arguments
                .as_ref()
                .and_then(|m| m.get("include_dependencies"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let module_depth = arguments
                .as_ref()
                .and_then(|m| m.get("module_depth"))
                .and_then(|v| v.as_u64())
                .unwrap_or(2) as u32;

            server
                .get_project_overview(rmcp::handler::server::wrapper::Parameters(
                    GetProjectOverviewRequest {
                        include_graph,
                        include_dependencies,
                        module_depth,
                    },
                ))
                .await
        }
        _ => {
            if json {
                use crate::io::exit_code::ExitCode;
                use crate::io::format::JsonResponse;
                let response = JsonResponse::error(
                    ExitCode::GeneralError,
                    &format!("Unknown tool: {tool}"),
                    vec![
                        "Available tools: find_symbol, get_calls, find_callers, analyze_impact, get_index_info, search_symbols, semantic_search_docs, semantic_search_chunks, semantic_search_with_context, get_source, get_module_exports, get_type_fields, get_state_graph, search_documents, get_project_overview, get_feature_context, get_call_tree",
                    ],
                );
                println!("{}", serde_json::to_string_pretty(&response).unwrap());
            } else {
                eprintln!("Unknown tool: {tool}");
                eprintln!(
                    "Available tools: find_symbol, get_calls, find_callers, analyze_impact, get_index_info, search_symbols, semantic_search_docs, semantic_search_chunks, semantic_search_with_context, get_source, get_module_exports, get_type_fields, get_state_graph, search_documents, get_project_overview, get_feature_context, get_call_tree"
                );
            }
            std::process::exit(1);
        }
    };

    // Print result
    match result {
        Ok(call_result) => {
            if json && tool == "get_index_info" {
                use crate::io::envelope::Envelope;
                use crate::io::guidance_engine::generate_guidance_from_config;

                if let Some(index_info) = index_info_data {
                    let mut envelope =
                        Envelope::success(index_info).with_message("Index statistics");

                    if let Some(hint) =
                        generate_guidance_from_config(&guidance_config, "get_index_info", None, 1)
                    {
                        envelope = envelope.with_hint(hint);
                    }

                    let output = match &fields {
                        Some(f) => envelope.to_json_with_fields(f),
                        None => envelope.to_json(),
                    };
                    println!("{}", output.expect("envelope serialization"));
                }
            } else if json && tool == "find_symbol" {
                // Use pre-collected data for JSON output
                if let Some(symbol_contexts) = find_symbol_data {
                    use crate::io::envelope::{EntityType, Envelope};
                    use crate::io::guidance_engine::generate_guidance_from_config;

                    let name = arguments
                        .as_ref()
                        .and_then(|m| m.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let language = arguments
                        .as_ref()
                        .and_then(|m| m.get("lang"))
                        .and_then(|v| v.as_str());

                    if symbol_contexts.is_empty() {
                        let mut envelope: Envelope<()> =
                            Envelope::not_found(format!("Symbol '{name}' not found"))
                                .with_entity_type(EntityType::Symbol)
                                .with_query(name);

                        if let Some(lang) = language {
                            envelope = envelope.with_lang(lang);
                        }

                        if let Some(hint) = generate_guidance_from_config(
                            &guidance_config,
                            "find_symbol",
                            Some(name),
                            0,
                        ) {
                            envelope = envelope.with_hint(hint);
                        }

                        // Envelope serialization is infallible for simple types
                        println!("{}", envelope.to_json().expect("envelope serialization"));
                        std::process::exit(3);
                    } else {
                        let count = symbol_contexts.len();
                        let mut envelope = Envelope::success(symbol_contexts)
                            .with_entity_type(EntityType::Symbol)
                            .with_count(count)
                            .with_query(name)
                            .with_message(format!("Found {count} symbol(s)"));

                        if let Some(lang) = language {
                            envelope = envelope.with_lang(lang);
                        }

                        if let Some(hint) = generate_guidance_from_config(
                            &guidance_config,
                            "find_symbol",
                            Some(name),
                            count,
                        ) {
                            envelope = envelope.with_hint(hint);
                        }

                        let output = match &fields {
                            Some(f) => envelope.to_json_with_fields(f),
                            None => envelope.to_json(),
                        };
                        println!("{}", output.expect("envelope serialization"));
                    }
                }
            } else if json && tool == "get_calls" {
                use crate::io::envelope::{EntityType, Envelope};
                use crate::io::guidance_engine::generate_guidance_from_config;

                let identifier = if let Some(id) = arguments
                    .as_ref()
                    .and_then(|m| m.get("symbol_id"))
                    .and_then(|v| v.as_u64())
                {
                    format!("symbol_id:{id}")
                } else {
                    arguments
                        .as_ref()
                        .and_then(|m| m.get("function_name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string()
                };
                let language = arguments
                    .as_ref()
                    .and_then(|m| m.get("lang"))
                    .and_then(|v| v.as_str());

                if let Some(calls) = get_calls_data {
                    let count = calls.len();
                    let mut envelope = Envelope::success(calls)
                        .with_entity_type(EntityType::Calls)
                        .with_count(count)
                        .with_query(&identifier)
                        .with_message(format!("Calls {count} function(s)"));

                    if let Some(lang) = language {
                        envelope = envelope.with_lang(lang);
                    }

                    if let Some(hint) = generate_guidance_from_config(
                        &guidance_config,
                        "get_calls",
                        Some(&identifier),
                        count,
                    ) {
                        envelope = envelope.with_hint(hint);
                    }

                    let output = match &fields {
                        Some(f) => envelope.to_json_with_fields(f),
                        None => envelope.to_json(),
                    };
                    println!("{}", output.expect("envelope serialization"));
                } else {
                    let mut envelope: Envelope<()> =
                        Envelope::not_found(format!("Function '{identifier}' not found"))
                            .with_entity_type(EntityType::Calls)
                            .with_query(&identifier);

                    if let Some(lang) = language {
                        envelope = envelope.with_lang(lang);
                    }

                    if let Some(hint) = generate_guidance_from_config(
                        &guidance_config,
                        "get_calls",
                        Some(&identifier),
                        0,
                    ) {
                        envelope = envelope.with_hint(hint);
                    }

                    println!("{}", envelope.to_json().expect("envelope serialization"));
                    std::process::exit(3);
                }
            } else if json && tool == "find_callers" {
                use crate::io::envelope::{EntityType, Envelope};
                use crate::io::guidance_engine::generate_guidance_from_config;

                let identifier = if let Some(id) = arguments
                    .as_ref()
                    .and_then(|m| m.get("symbol_id"))
                    .and_then(|v| v.as_u64())
                {
                    format!("symbol_id:{id}")
                } else {
                    arguments
                        .as_ref()
                        .and_then(|m| m.get("function_name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string()
                };
                let language = arguments
                    .as_ref()
                    .and_then(|m| m.get("lang"))
                    .and_then(|v| v.as_str());

                if let Some(callers) = find_callers_data {
                    let count = callers.len();
                    let mut envelope = Envelope::success(callers)
                        .with_entity_type(EntityType::Callers)
                        .with_count(count)
                        .with_query(&identifier)
                        .with_message(format!("Called by {count} function(s)"));

                    if let Some(lang) = language {
                        envelope = envelope.with_lang(lang);
                    }

                    if let Some(hint) = generate_guidance_from_config(
                        &guidance_config,
                        "find_callers",
                        Some(&identifier),
                        count,
                    ) {
                        envelope = envelope.with_hint(hint);
                    }

                    let output = match &fields {
                        Some(f) => envelope.to_json_with_fields(f),
                        None => envelope.to_json(),
                    };
                    println!("{}", output.expect("envelope serialization"));
                } else {
                    let mut envelope: Envelope<()> =
                        Envelope::not_found(format!("Function '{identifier}' not found"))
                            .with_entity_type(EntityType::Callers)
                            .with_query(&identifier);

                    if let Some(lang) = language {
                        envelope = envelope.with_lang(lang);
                    }

                    if let Some(hint) = generate_guidance_from_config(
                        &guidance_config,
                        "find_callers",
                        Some(&identifier),
                        0,
                    ) {
                        envelope = envelope.with_hint(hint);
                    }

                    println!("{}", envelope.to_json().expect("envelope serialization"));
                    std::process::exit(3);
                }
            } else if json && tool == "analyze_impact" {
                use crate::io::envelope::{EntityType, Envelope};
                use crate::io::guidance_engine::generate_guidance_from_config;

                // Get identifier for messages
                let identifier = if let Some(id) = arguments
                    .as_ref()
                    .and_then(|m| m.get("symbol_id"))
                    .and_then(|v| v.as_u64())
                {
                    format!("symbol_id:{id}")
                } else {
                    arguments
                        .as_ref()
                        .and_then(|m| m.get("symbol_name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string()
                };

                let max_depth = arguments
                    .as_ref()
                    .and_then(|m| m.get("max_depth"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(3) as u32;

                if let Some(impacted) = analyze_impact_data {
                    let count = impacted.len();
                    let mut envelope = Envelope::success(impacted)
                        .with_entity_type(EntityType::ImpactGraph)
                        .with_count(count)
                        .with_query(&identifier)
                        .with_depth(max_depth)
                        .with_message(format!("{count} symbol(s) would be impacted"));

                    if let Some(hint) = generate_guidance_from_config(
                        &guidance_config,
                        "analyze_impact",
                        Some(&identifier),
                        count,
                    ) {
                        envelope = envelope.with_hint(hint);
                    }

                    let output = match &fields {
                        Some(f) => envelope.to_json_with_fields(f),
                        None => envelope.to_json(),
                    };
                    println!("{}", output.expect("envelope serialization"));
                } else {
                    // Symbol not found
                    let mut envelope: Envelope<()> =
                        Envelope::not_found(format!("Symbol '{identifier}' not found"))
                            .with_entity_type(EntityType::ImpactGraph)
                            .with_query(&identifier);

                    if let Some(hint) = generate_guidance_from_config(
                        &guidance_config,
                        "analyze_impact",
                        Some(&identifier),
                        0,
                    ) {
                        envelope = envelope.with_hint(hint);
                    }

                    println!("{}", envelope.to_json().expect("envelope serialization"));
                    std::process::exit(3);
                }
            } else if json && tool == "search_symbols" {
                use crate::io::envelope::{EntityType, Envelope, ResultCode};
                use crate::io::guidance_engine::generate_guidance_from_config;

                let query = arguments
                    .as_ref()
                    .and_then(|m| m.get("query"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let language = arguments
                    .as_ref()
                    .and_then(|m| m.get("lang"))
                    .and_then(|v| v.as_str());

                if let Some(results) = search_symbols_data {
                    // Transform to consistent nested symbol shape
                    let transformed: Vec<SearchSymbolResult> =
                        results.into_iter().map(Into::into).collect();
                    let count = transformed.len();

                    let mut envelope = if count == 0 {
                        Envelope::<Vec<SearchSymbolResult>>::not_found(format!(
                            "No symbols found for '{query}'"
                        ))
                        .with_entity_type(EntityType::SearchResult)
                        .with_query(query)
                    } else {
                        Envelope::success(transformed)
                            .with_entity_type(EntityType::SearchResult)
                            .with_count(count)
                            .with_query(query)
                            .with_message(format!("Found {count} symbol(s)"))
                    };

                    if let Some(lang) = language {
                        envelope = envelope.with_lang(lang);
                    }

                    if let Some(hint) = generate_guidance_from_config(
                        &guidance_config,
                        "search_symbols",
                        Some(query),
                        count,
                    ) {
                        envelope = envelope.with_hint(hint);
                    }

                    let output = match &fields {
                        Some(f) => envelope.to_json_with_fields(f),
                        None => envelope.to_json(),
                    };
                    println!("{}", output.expect("envelope serialization"));
                } else {
                    let envelope: Envelope<()> = Envelope::error(
                        ResultCode::InvalidQuery,
                        format!("Failed to search for '{query}'"),
                    )
                    .with_entity_type(EntityType::SearchResult)
                    .with_query(query)
                    .with_hint("Check query syntax");

                    println!("{}", envelope.to_json().expect("envelope serialization"));
                    std::process::exit(1);
                }
            } else if json && tool == "semantic_search_docs" {
                use crate::io::envelope::{EntityType, Envelope, ResultCode};
                use crate::io::guidance_engine::generate_guidance_from_config;

                let query = arguments
                    .as_ref()
                    .and_then(|m| m.get("query"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let language = arguments
                    .as_ref()
                    .and_then(|m| m.get("lang"))
                    .and_then(|v| v.as_str());

                if let Some(results) = semantic_search_docs_data {
                    let count = results.len();

                    let mut envelope = if count == 0 {
                        Envelope::<Vec<SemanticSearchResult>>::not_found(format!(
                            "No similar documentation found for '{query}'"
                        ))
                        .with_entity_type(EntityType::Symbol)
                        .with_query(query)
                    } else {
                        Envelope::success(results)
                            .with_entity_type(EntityType::Symbol)
                            .with_count(count)
                            .with_query(query)
                            .with_message(format!("Found {count} similar symbol(s)"))
                    };

                    if let Some(lang) = language {
                        envelope = envelope.with_lang(lang);
                    }

                    if let Some(hint) = generate_guidance_from_config(
                        &guidance_config,
                        "semantic_search_docs",
                        Some(query),
                        count,
                    ) {
                        envelope = envelope.with_hint(hint);
                    }

                    let output = match &fields {
                        Some(f) => envelope.to_json_with_fields(f),
                        None => envelope.to_json(),
                    };
                    println!("{}", output.expect("envelope serialization"));
                } else if !has_semantic_search {
                    let envelope: Envelope<()> =
                        Envelope::error(ResultCode::IndexError, "Semantic search is not enabled")
                            .with_entity_type(EntityType::Symbol)
                            .with_query(query)
                            .with_hint(
                                "Enable semantic search in settings.toml and rebuild the index",
                            );

                    println!("{}", envelope.to_json().expect("envelope serialization"));
                    std::process::exit(1);
                } else {
                    let envelope: Envelope<()> = Envelope::error(
                        ResultCode::InvalidQuery,
                        format!("Failed to search for '{query}'"),
                    )
                    .with_entity_type(EntityType::Symbol)
                    .with_query(query)
                    .with_hint("Check query syntax");

                    println!("{}", envelope.to_json().expect("envelope serialization"));
                    std::process::exit(1);
                }
            } else if json && tool == "semantic_search_with_context" {
                use crate::io::envelope::{EntityType, Envelope, ResultCode};
                use crate::io::guidance_engine::generate_guidance_from_config;

                let query = arguments
                    .as_ref()
                    .and_then(|m| m.get("query"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let language = arguments
                    .as_ref()
                    .and_then(|m| m.get("lang"))
                    .and_then(|v| v.as_str());

                if let Some(results) = semantic_search_with_context_data {
                    let count = results.len();

                    let mut envelope = if count == 0 {
                        Envelope::<Vec<SemanticSearchWithContextResult>>::not_found(format!(
                            "No similar symbols found for '{query}'"
                        ))
                        .with_entity_type(EntityType::Symbol)
                        .with_query(query)
                    } else {
                        Envelope::success(results)
                            .with_entity_type(EntityType::Symbol)
                            .with_count(count)
                            .with_query(query)
                            .with_message(format!("Found {count} symbol(s) with context"))
                    };

                    if let Some(lang) = language {
                        envelope = envelope.with_lang(lang);
                    }

                    if let Some(hint) = generate_guidance_from_config(
                        &guidance_config,
                        "semantic_search_with_context",
                        Some(query),
                        count,
                    ) {
                        envelope = envelope.with_hint(hint);
                    }

                    let output = match &fields {
                        Some(f) => envelope.to_json_with_fields(f),
                        None => envelope.to_json(),
                    };
                    println!("{}", output.expect("envelope serialization"));
                } else if !has_semantic_search {
                    let envelope: Envelope<()> =
                        Envelope::error(ResultCode::IndexError, "Semantic search is not enabled")
                            .with_entity_type(EntityType::Symbol)
                            .with_query(query)
                            .with_hint(
                                "Enable semantic search in settings.toml and rebuild the index",
                            );

                    println!("{}", envelope.to_json().expect("envelope serialization"));
                    std::process::exit(1);
                } else {
                    let envelope: Envelope<()> = Envelope::error(
                        ResultCode::InvalidQuery,
                        format!("Failed to search for '{query}'"),
                    )
                    .with_entity_type(EntityType::Symbol)
                    .with_query(query)
                    .with_hint("Check query syntax");

                    println!("{}", envelope.to_json().expect("envelope serialization"));
                    std::process::exit(1);
                }
            } else if json && tool == "semantic_search_chunks" {
                use crate::io::envelope::{EntityType, Envelope, ResultCode};
                use crate::io::guidance_engine::generate_guidance_from_config;

                let query = arguments
                    .as_ref()
                    .and_then(|m| m.get("query"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let language = arguments
                    .as_ref()
                    .and_then(|m| m.get("lang"))
                    .and_then(|v| v.as_str());

                if let Some(outcome) = semantic_search_chunks_data {
                    let count = outcome.results.len();

                    let mut envelope = if count == 0 {
                        Envelope::<Vec<crate::chunks::ChunkSearchResult>>::not_found(format!(
                            "No relevant chunks found for '{query}'"
                        ))
                        .with_entity_type(EntityType::Document)
                        .with_query(query)
                    } else {
                        Envelope::success(outcome.results)
                            .with_entity_type(EntityType::Document)
                            .with_count(count)
                            .with_query(query)
                            .with_message(format!(
                                "Found {count} strong chunk match(es) ({} weak hidden)",
                                outcome.weak_count
                            ))
                    };

                    if let Some(lang) = language {
                        envelope = envelope.with_lang(lang);
                    }
                    if outcome.bm25_only_fallback {
                        envelope = envelope.with_hint(
                            "Vector recall unavailable; results are BM25-only fallback. Rebuild embeddings with: codanna index . --force",
                        );
                    }

                    if let Some(hint) = generate_guidance_from_config(
                        &guidance_config,
                        "semantic_search_chunks",
                        Some(query),
                        count,
                    ) {
                        envelope = envelope.with_hint(hint);
                    }

                    let output = match &fields {
                        Some(f) => envelope.to_json_with_fields(f),
                        None => envelope.to_json(),
                    };
                    println!("{}", output.expect("envelope serialization"));
                } else {
                    let envelope: Envelope<()> = Envelope::error(
                        ResultCode::InvalidQuery,
                        format!("Failed to search chunks for '{query}'"),
                    )
                    .with_entity_type(EntityType::Document)
                    .with_query(query)
                    .with_hint("Check query syntax");

                    println!("{}", envelope.to_json().expect("envelope serialization"));
                    std::process::exit(1);
                }
            } else if json && tool == "search_documents" {
                use crate::io::envelope::{EntityType, Envelope};

                let query = arguments
                    .as_ref()
                    .and_then(|m| m.get("query"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                if let Some((query_text, results)) = search_documents_data {
                    let count = results.len();

                    // Convert to serializable format
                    let data: Vec<_> = results
                        .iter()
                        .map(|r| {
                            serde_json::json!({
                                "chunk_id": r.chunk_id,
                                "collection": r.collection,
                                "source_path": r.source_path,
                                "heading_context": r.heading_context,
                                "content_preview": r.content_preview,
                                "byte_range": r.byte_range,
                                "similarity": r.similarity
                            })
                        })
                        .collect();

                    let envelope = if count == 0 {
                        Envelope::<Vec<serde_json::Value>>::not_found(format!(
                            "No documents found for '{query_text}'"
                        ))
                        .with_entity_type(EntityType::Document)
                        .with_query(&query_text)
                    } else {
                        Envelope::success(data)
                            .with_entity_type(EntityType::Document)
                            .with_count(count)
                            .with_query(&query_text)
                            .with_message(format!("Found {count} matching documents"))
                            .with_hint(
                                "Use the file paths and byte ranges to read specific sections",
                            )
                    };

                    let output = match &fields {
                        Some(f) => envelope.to_json_with_fields(f),
                        None => envelope.to_json(),
                    };
                    println!("{}", output.expect("envelope serialization"));

                    if count == 0 {
                        std::process::exit(1);
                    }
                } else {
                    let envelope: Envelope<()> = Envelope::error(
                        crate::io::envelope::ResultCode::IndexError,
                        "Document search not available",
                    )
                    .with_entity_type(EntityType::Document)
                    .with_query(query)
                    .with_hint("Run 'codanna documents index' to create the index");

                    println!("{}", envelope.to_json().expect("envelope serialization"));
                    std::process::exit(1);
                }
            } else {
                // Default text output
                for content in &call_result.content {
                    match &**content {
                        rmcp::model::RawContent::Text(text_content) => {
                            println!("{}", text_content.text);
                        }
                        _ => {
                            eprintln!("Warning: Non-text content returned");
                        }
                    }
                }
            }
        }
        Err(e) => {
            if json {
                use crate::io::envelope::{Envelope, ResultCode};
                let envelope: Envelope<()> =
                    Envelope::error(ResultCode::InternalError, e.message.to_string())
                        .with_hint("Check the tool name and arguments");

                println!("{}", envelope.to_json().expect("envelope serialization"));
                std::process::exit(1);
            } else {
                eprintln!("Error calling tool: {}", e.message);
                std::process::exit(1);
            }
        }
    }
}
