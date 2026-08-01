//! MCP (Model Context Protocol) server implementation for code intelligence
//!
//! This module provides MCP tools that allow AI assistants to query
//! the code intelligence index.
//!
//! ## Architecture
//!
//! The MCP server can run in two modes:
//!
//! 1. **Standalone Server Mode**: Run with `cargo run -- serve`
//!    - Loads index once into memory
//!    - Listens for client connections via stdio
//!    - Efficient for production use with AI assistants
//!
//! 2. **Embedded Mode**: Used by the CLI directly
//!    - No separate process needed
//!    - Direct access to already-loaded index
//!    - Most memory efficient for CLI operations

pub mod client;
pub mod http_server;
pub mod https_server;
pub mod knowledge;
pub mod notifications;
pub mod react_hooks;

use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CustomNotification, CustomRequest, CustomResult, ErrorCode, ErrorData as McpError, *},
    schemars,
    service::{Peer, RequestContext, RoleServer, ServiceError},
    tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use crate::documents::{DocumentStore, SearchQuery as DocSearchQuery};
use crate::indexing::facade::IndexFacade;
use crate::{Settings, Symbol};

/// Generate guidance for MCP tool responses
fn generate_mcp_guidance(settings: &Settings, tool: &str, result_count: usize) -> Option<String> {
    use crate::io::guidance_engine::generate_guidance_from_config;
    generate_guidance_from_config(&settings.guidance, tool, None, result_count)
}

/// Format a Unix timestamp as relative time (e.g., "2 hours ago")
pub fn format_relative_time(timestamp: u64) -> String {
    use chrono::{DateTime, Utc};

    let now = Utc::now();
    let then = DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_else(Utc::now);

    let diff = (now.timestamp() - then.timestamp()) as u64;

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        let mins = diff / 60;
        format!("{} minute{} ago", mins, if mins == 1 { "" } else { "s" })
    } else if diff < 86400 {
        let hours = diff / 3600;
        format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
    } else if diff < 604800 {
        let days = diff / 86400;
        format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
    } else {
        // For older dates, show the actual formatted date
        then.format("%Y-%m-%d").to_string()
    }
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct FindSymbolRequest {
    /// Name of the symbol to find
    pub name: String,
    /// Filter by programming language (e.g., "rust", "python", "typescript", "php")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct GetCallsRequest {
    /// Name of the function to analyze (use symbol_id for unambiguous lookup)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_name: Option<String>,
    /// Symbol ID for direct lookup (recommended to avoid ambiguity)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct FindCallersRequest {
    /// Name of the function to find callers for (use symbol_id for unambiguous lookup)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_name: Option<String>,
    /// Symbol ID for direct lookup (recommended to avoid ambiguity)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct AnalyzeImpactRequest {
    /// Name of the symbol to analyze impact for (use symbol_id for unambiguous lookup)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_name: Option<String>,
    /// Symbol ID for direct lookup (recommended to avoid ambiguity)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<u32>,
    /// Maximum depth to search (default: 3)
    #[serde(default = "default_depth")]
    pub max_depth: u32,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct SearchSymbolsRequest {
    /// Search query (supports fuzzy matching)
    pub query: String,
    /// Maximum number of results (default: 10)
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Filter by symbol kind (e.g., "Function", "Struct", "Trait")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Filter by module path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// Filter by programming language (e.g., "rust", "python", "typescript", "php")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct SemanticSearchRequest {
    /// Natural language search query
    pub query: String,
    /// Maximum number of results (default: 10)
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Minimum similarity score (0-1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f32>,
    /// Filter by programming language (e.g., "rust", "python", "typescript", "php")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

/// Same shape as [`SemanticSearchRequest`], but with its own default limit: chunk search was
/// measured and ten is the wrong place to stop, while the doc search sharing that number was
/// not measured and should not move with it.
#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ChunkSearchRequest {
    /// Natural language search query
    pub query: String,
    /// Maximum number of results (default: 20)
    #[serde(default = "default_chunk_limit")]
    pub limit: u32,
    /// Minimum similarity score (0-1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f32>,
    /// Filter by programming language (e.g., "rust", "python", "typescript", "php")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct SemanticSearchWithContextRequest {
    /// Natural language search query
    pub query: String,
    /// Maximum number of results (default: 5, as each includes full context)
    #[serde(default = "default_context_limit")]
    pub limit: u32,
    /// Minimum similarity score (0-1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f32>,
    /// Filter by programming language (e.g., "rust", "python", "typescript", "php")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct GetIndexInfoRequest {}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct GetSourceRequest {
    /// Symbol ID to retrieve source code for
    pub symbol_id: u32,
    /// Number of context lines to include before and after the symbol (default: 0)
    #[serde(default)]
    pub context_lines: u32,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct SearchDocumentsRequest {
    /// Natural language search query
    pub query: String,
    /// Filter by collection name (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    /// Maximum number of results (default: 5)
    #[serde(default = "default_context_limit")]
    pub limit: u32,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct GetModuleExportsRequest {
    /// File path to get exports for (supports fuzzy matching with ends_with)
    pub file_path: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct GetTypeFieldsRequest {
    /// Symbol ID to retrieve fields for
    pub symbol_id: u32,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct GetStateGraphRequest {
    /// Symbol ID to analyze React hooks for
    pub symbol_id: u32,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct GetFeatureContextRequest {
    /// Symbol ID to retrieve comprehensive context for
    pub symbol_id: u32,

    /// Include source code in output (default: true)
    #[serde(default = "default_true")]
    pub include_source: bool,

    /// Include impact analysis in output (default: true)
    #[serde(default = "default_true")]
    pub include_impact: bool,

    /// Show call site code examples (default: true)
    #[serde(default = "default_true")]
    pub show_call_examples: bool,

    /// Maximum number of callers to show (default: 10)
    #[serde(default = "default_max_list")]
    pub max_callers: u32,

    /// Maximum number of calls to show (default: 10)
    #[serde(default = "default_max_list")]
    pub max_calls: u32,

    /// Maximum number of impact results to show (default: 20)
    #[serde(default = "default_max_impact")]
    pub max_impact: u32,

    /// Impact analysis depth (default: 2)
    #[serde(default = "default_impact_depth")]
    pub impact_depth: u32,

    /// Number of call site examples to show (default: 3)
    #[serde(default = "default_max_examples")]
    pub max_examples: u32,

    /// Context lines around source code (default: 5)
    #[serde(default = "default_context_limit")]
    pub context_lines: u32,
}

/// Request to get recursive call tree for a symbol
#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct GetCallTreeRequest {
    /// Symbol ID to start the call tree from
    pub symbol_id: u32,

    /// Maximum depth to traverse (default: 4)
    /// Higher values show deeper call chains but may be slower
    #[serde(default = "default_call_tree_depth")]
    pub max_depth: u32,

    /// Include source code snippets for each call (default: false)
    /// Useful for understanding context but increases output size
    #[serde(default)]
    pub include_source: bool,

    /// Include metadata (file path, line numbers) (default: true)
    #[serde(default = "default_true")]
    pub include_metadata: bool,

    /// Show external library calls (default: false)
    /// Set to true to see stdlib/node_modules calls
    #[serde(default)]
    pub show_external_calls: bool,

    /// Maximum total nodes to return (default: 100)
    /// Prevents huge trees from consuming too much memory
    #[serde(default = "default_max_call_tree_nodes")]
    pub max_nodes: u32,

    /// Collapse duplicate calls at same level into single entry with [×N] count (default: true)
    /// Example: pipeline::new [×14] instead of 14 separate lines
    /// Reduces noise from repeated utility calls
    #[serde(default = "default_true")]
    pub collapse_duplicates: bool,

    /// Show trivial utility calls (getters, iterators, constructors) (default: false)
    /// By default, only business logic calls are shown for cleaner output
    /// Set to true to see all calls including utility functions
    #[serde(default)]
    pub show_trivial: bool,
}

/// Request to get comprehensive project overview with architecture analysis
#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct GetProjectOverviewRequest {
    /// Include relationship graph showing module dependencies (default: true)
    #[serde(default = "default_true")]
    pub include_graph: bool,

    /// Include dependency information from package files (default: true)
    #[serde(default = "default_true")]
    pub include_dependencies: bool,

    /// Module grouping depth from indexed root (default: 2)
    /// Example: depth=2 groups src/api/routes/users.rs into src/api/
    #[serde(default = "default_module_depth")]
    pub module_depth: u32,
}

fn default_module_depth() -> u32 {
    2
}

fn default_depth() -> u32 {
    3
}

fn default_limit() -> u32 {
    10
}

// Twenty, because that is where the recall curve stops being steep. Across four repositories
// the wanted file is in the results 76% of the time at ten and 83% at twenty, and the next
// ten only add three points - so this doubling buys more than the two after it combined, for
// about 700 tokens.
pub(crate) fn default_chunk_limit() -> u32 {
    20
}

fn default_context_limit() -> u32 {
    5
}

fn default_true() -> bool {
    true
}

fn default_max_list() -> u32 {
    10
}

fn default_max_impact() -> u32 {
    20
}

fn default_impact_depth() -> u32 {
    2
}

fn default_max_examples() -> u32 {
    3
}

fn default_call_tree_depth() -> u32 {
    4
}

fn default_max_call_tree_nodes() -> u32 {
    100
}

#[derive(Clone)]
pub struct CodeIntelligenceServer {
    pub facade: Arc<RwLock<IndexFacade>>,
    pub document_store: Option<Arc<RwLock<DocumentStore>>>,
    tool_router: ToolRouter<Self>,
    peer: Arc<Mutex<Option<Peer<RoleServer>>>>,
}

#[tool_router]
impl CodeIntelligenceServer {
    pub fn new(facade: IndexFacade) -> Self {
        Self {
            facade: Arc::new(RwLock::new(facade)),
            document_store: None,
            tool_router: Self::tool_router(),
            peer: Arc::new(Mutex::new(None)),
        }
    }

    /// Create server from an already-loaded facade (most efficient)
    pub fn from_facade(facade: Arc<RwLock<IndexFacade>>) -> Self {
        Self {
            facade,
            document_store: None,
            tool_router: Self::tool_router(),
            peer: Arc::new(Mutex::new(None)),
        }
    }

    /// Create server with existing facade and settings (for HTTP server)
    pub fn new_with_facade(facade: Arc<RwLock<IndexFacade>>, _settings: Arc<Settings>) -> Self {
        Self {
            facade,
            document_store: None,
            tool_router: Self::tool_router(),
            peer: Arc::new(Mutex::new(None)),
        }
    }

    /// Add document store for document search capability
    pub fn with_document_store(mut self, store: DocumentStore) -> Self {
        self.document_store = Some(Arc::new(RwLock::new(store)));
        self
    }

    /// Add document store from existing Arc (for sharing with watcher)
    pub fn with_document_store_arc(mut self, store: Arc<RwLock<DocumentStore>>) -> Self {
        self.document_store = Some(store);
        self
    }

    /// Get a reference to the facade Arc for external management (e.g., hot-reload)
    pub fn get_facade_arc(&self) -> Arc<RwLock<IndexFacade>> {
        self.facade.clone()
    }

    /// Send a notification when a file is re-indexed
    pub async fn notify_file_reindexed(&self, file_path: &str) {
        let peer_guard = self.peer.lock().await;
        if let Some(peer) = peer_guard.as_ref() {
            // Send a resource updated notification
            let _ = peer
                .notify_resource_updated(ResourceUpdatedNotificationParam {
                    uri: format!("file://{file_path}"),
                })
                .await;

            // Also send a logging message for visibility
            let _ = peer
                .notify_logging_message(LoggingMessageNotificationParam {
                    level: LoggingLevel::Info,
                    logger: Some("codanna".to_string()),
                    data: serde_json::json!({
                        "action": "re-indexed",
                        "file": file_path
                    }),
                })
                .await;
        }
    }

    #[tool(description = "Find a symbol by name in the indexed codebase")]
    pub async fn find_symbol(
        &self,
        Parameters(FindSymbolRequest { name, lang }): Parameters<FindSymbolRequest>,
    ) -> Result<CallToolResult, McpError> {
        use crate::symbol::context::ContextIncludes;

        let indexer = self.facade.read().await;

        // Support symbol_id:XXX format for direct lookup (from semantic search results)
        let symbols = if let Some(id_str) = name.strip_prefix("symbol_id:") {
            if let Ok(id) = id_str.parse::<u32>() {
                indexer
                    .get_symbol(crate::SymbolId(id))
                    .map(|s| vec![s])
                    .unwrap_or_default()
            } else {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Invalid symbol_id format: {id_str}"
                ))]));
            }
        } else {
            indexer.find_symbols_by_name(&name, lang.as_deref())
        };

        if symbols.is_empty() {
            let mut output = format!("No symbols found with name: {name}");
            // Add guidance for no results
            if let Some(guidance) = generate_mcp_guidance(indexer.settings(), "find_symbol", 0) {
                output.push_str("\n\n---\n💡 ");
                output.push_str(&guidance);
                output.push('\n');
            }
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        let mut result = format!("Found {} symbol(s) named '{}':\n\n", symbols.len(), name);

        for (idx, symbol) in symbols.iter().enumerate() {
            if idx > 0 {
                result.push_str("\n---\n\n");
            }

            // Try to get full context with all relationship types
            if let Some(ctx) = indexer.get_symbol_context(
                symbol.id,
                ContextIncludes::IMPLEMENTATIONS
                    | ContextIncludes::DEFINITIONS
                    | ContextIncludes::CALLERS
                    | ContextIncludes::EXTENDS
                    | ContextIncludes::USES,
            ) {
                // Use formatted output from context
                result.push_str(&ctx.format_location_with_type());
                result.push('\n');

                // Add module path if available
                if let Some(module) = symbol.as_module_path() {
                    result.push_str(&format!("Module: {module}\n"));
                }

                // Add signature if available
                if let Some(sig) = symbol.as_signature() {
                    result.push_str(&format!("Signature: {sig}\n"));
                }

                // Add documentation preview
                if let Some(doc) = symbol.as_doc_comment() {
                    let doc_preview: Vec<&str> = doc.lines().take(3).collect();
                    let preview = if doc.lines().count() > 3 {
                        format!("{}...", doc_preview.join(" "))
                    } else {
                        doc_preview.join(" ")
                    };
                    result.push_str(&format!("Documentation: {preview}\n"));
                }

                // Add relationship summary
                let mut has_relationships = false;

                // What traits this type implements
                if let Some(impls) = &ctx.relationships.implements {
                    if !impls.is_empty() {
                        result.push_str(&format!("Implements: {} trait(s)\n", impls.len()));
                        for trait_sym in impls.iter().take(5) {
                            result.push_str(&format!(
                                "  -> {} at {}\n",
                                trait_sym.name,
                                crate::symbol::context::SymbolContext::symbol_location(trait_sym)
                            ));
                        }
                        if impls.len() > 5 {
                            result.push_str(&format!("  ... and {} more\n", impls.len() - 5));
                        }
                        has_relationships = true;
                    }
                }

                // What types implement this trait
                if let Some(impls) = &ctx.relationships.implemented_by {
                    if !impls.is_empty() {
                        result.push_str(&format!("Implemented by: {} type(s)\n", impls.len()));
                        for impl_sym in impls.iter().take(5) {
                            result.push_str(&format!(
                                "  <- {} at {}\n",
                                impl_sym.name,
                                crate::symbol::context::SymbolContext::symbol_location(impl_sym)
                            ));
                        }
                        if impls.len() > 5 {
                            result.push_str(&format!("  ... and {} more\n", impls.len() - 5));
                        }
                        has_relationships = true;
                    }
                }

                if let Some(defines) = &ctx.relationships.defines {
                    if !defines.is_empty() {
                        let methods = defines
                            .iter()
                            .filter(|s| s.kind == crate::SymbolKind::Method)
                            .count();
                        if methods > 0 {
                            result.push_str(&format!("Defines: {methods} method(s)\n"));
                            has_relationships = true;
                        }
                    }
                }

                if let Some(callers) = &ctx.relationships.called_by {
                    if !callers.is_empty() {
                        result.push_str(&format!("Called by: {} function(s)\n", callers.len()));
                        has_relationships = true;
                    }
                }

                // What base class(es) this extends
                if let Some(extends) = &ctx.relationships.extends {
                    if !extends.is_empty() {
                        result.push_str(&format!("Extends: {} class(es)\n", extends.len()));
                        for base in extends.iter().take(3) {
                            result.push_str(&format!(
                                "  -> {} at {}\n",
                                base.name,
                                crate::symbol::context::SymbolContext::symbol_location(base)
                            ));
                        }
                        if extends.len() > 3 {
                            result.push_str(&format!("  ... and {} more\n", extends.len() - 3));
                        }
                        has_relationships = true;
                    }
                }

                // What classes extend this
                if let Some(extended_by) = &ctx.relationships.extended_by {
                    if !extended_by.is_empty() {
                        result.push_str(&format!("Extended by: {} class(es)\n", extended_by.len()));
                        for derived in extended_by.iter().take(3) {
                            result.push_str(&format!(
                                "  <- {} at {}\n",
                                derived.name,
                                crate::symbol::context::SymbolContext::symbol_location(derived)
                            ));
                        }
                        if extended_by.len() > 3 {
                            result.push_str(&format!("  ... and {} more\n", extended_by.len() - 3));
                        }
                        has_relationships = true;
                    }
                }

                // What types this symbol uses
                if let Some(uses) = &ctx.relationships.uses {
                    if !uses.is_empty() {
                        result.push_str(&format!("Uses: {} type(s)\n", uses.len()));
                        for used in uses.iter().take(3) {
                            result.push_str(&format!(
                                "  -> {} at {}\n",
                                used.name,
                                crate::symbol::context::SymbolContext::symbol_location(used)
                            ));
                        }
                        if uses.len() > 3 {
                            result.push_str(&format!("  ... and {} more\n", uses.len() - 3));
                        }
                        has_relationships = true;
                    }
                }

                // What symbols use this type
                if let Some(used_by) = &ctx.relationships.used_by {
                    if !used_by.is_empty() {
                        result.push_str(&format!("Used by: {} symbol(s)\n", used_by.len()));
                        has_relationships = true;
                    }
                }

                if !has_relationships && symbol.kind == crate::SymbolKind::Function {
                    result.push_str("No direct callers found\n");
                }
            } else {
                // Fallback to basic info
                result.push_str(&format!(
                    "{:?} at {}:{}\n",
                    symbol.kind,
                    symbol.file_path,
                    symbol.range.start_line + 1
                ));

                if let Some(ref doc) = symbol.doc_comment {
                    let doc_preview: Vec<&str> = doc.lines().take(3).collect();
                    let preview = if doc.lines().count() > 3 {
                        format!("{}...", doc_preview.join(" "))
                    } else {
                        doc_preview.join(" ")
                    };
                    result.push_str(&format!("Documentation: {preview}\n"));
                }

                if let Some(ref sig) = symbol.signature {
                    result.push_str(&format!("Signature: {sig}\n"));
                }
            }
        }

        // Add system guidance
        if let Some(guidance) =
            generate_mcp_guidance(indexer.settings(), "find_symbol", symbols.len())
        {
            result.push_str("\n---\n💡 ");
            result.push_str(&guidance);
            result.push('\n');
        }

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(
        description = "Get functions that a given function CALLS (invokes with parentheses).\n\nShows: function_name() → what it calls\nDoes NOT show: Type usage, component rendering, or who calls this function.\n\nUse analyze_impact for: Type dependencies, component usage (JSX), or reverse lookups."
    )]
    pub async fn get_calls(
        &self,
        Parameters(GetCallsRequest {
            function_name,
            symbol_id,
        }): Parameters<GetCallsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let indexer = self.facade.read().await;

        // Get the symbol either by ID or by name
        let (symbol, identifier) = if let Some(id) = symbol_id {
            // Direct lookup by symbol ID
            match indexer.get_symbol(crate::SymbolId(id)) {
                Some(sym) => (sym, format!("symbol_id:{id}")),
                None => {
                    return Ok(CallToolResult::success(vec![Content::text(format!(
                        "Symbol not found: symbol_id:{id}"
                    ))]));
                }
            }
        } else if let Some(name) = function_name {
            // Lookup by name
            let symbols = indexer.find_symbols_by_name(&name, None);

            if symbols.is_empty() {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "Function not found: {name}"
                ))]));
            }

            if symbols.len() > 1 {
                // Multiple symbols found - return error with list
                let mut msg = format!(
                    "Ambiguous: found {} symbol(s) named '{}':\n",
                    symbols.len(),
                    name
                );
                for (i, sym) in symbols.iter().take(10).enumerate() {
                    msg.push_str(&format!(
                        "  {}. symbol_id:{} - {:?} at {}:{}\n",
                        i + 1,
                        sym.id.value(),
                        sym.kind,
                        sym.file_path,
                        sym.range.start_line + 1
                    ));
                }
                if symbols.len() > 10 {
                    msg.push_str(&format!("  ... and {} more\n", symbols.len() - 10));
                }
                msg.push_str("\nUse: get_calls symbol_id:<id> for specific symbol");
                return Ok(CallToolResult::success(vec![Content::text(msg)]));
            }

            // Single match - use it
            (symbols.into_iter().next().unwrap(), name)
        } else {
            return Ok(CallToolResult::success(vec![Content::text(
                "Error: Either function_name or symbol_id must be provided".to_string(),
            )]));
        };

        // Get calls for this specific symbol
        let all_called_with_metadata = indexer.get_called_functions_with_metadata(symbol.id);

        if all_called_with_metadata.is_empty() {
            let mut output = format!("{identifier} doesn't call any functions");
            // Add guidance for no results
            if let Some(guidance) = generate_mcp_guidance(indexer.settings(), "get_calls", 0) {
                output.push_str("\n\n---\n💡 ");
                output.push_str(&guidance);
                output.push('\n');
            }
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        let result_count = all_called_with_metadata.len();
        let mut result = format!("{identifier} calls {result_count} function(s):\n");
        for (callee, metadata) in all_called_with_metadata {
            // Parse metadata to extract receiver info and call site location
            let (call_display, call_line) = if let Some(ref meta) = metadata {
                let display = if let Some(context) = &meta.context {
                    if context.contains("receiver:") && context.contains("static:") {
                        // Parse "receiver:{receiver},static:{is_static}"
                        let parts: Vec<&str> = context.split(',').collect();
                        let mut receiver = "";
                        let mut is_static = false;

                        for part in parts {
                            if let Some(r) = part.strip_prefix("receiver:") {
                                receiver = r;
                            } else if let Some(s) = part.strip_prefix("static:") {
                                is_static = s == "true";
                            }
                        }

                        if !receiver.is_empty() {
                            if is_static {
                                format!("{}::{}", receiver, callee.name)
                            } else {
                                format!("{}.{}", receiver, callee.name)
                            }
                        } else {
                            callee.name.to_string()
                        }
                    } else {
                        callee.name.to_string()
                    }
                } else {
                    callee.name.to_string()
                };

                // Use call site line if available, otherwise definition line
                let line = meta
                    .line
                    .map(|l| l + 1)
                    .unwrap_or(callee.range.start_line + 1);
                (display, line)
            } else {
                (callee.name.to_string(), callee.range.start_line + 1)
            };

            result.push_str(&format!(
                "  -> {:?} {} at {}:{}\n",
                callee.kind, call_display, callee.file_path, call_line
            ));
            if let Some(ref sig) = callee.signature {
                result.push_str(&format!("     Signature: {sig}\n"));
            }
        }

        // Add system guidance
        if let Some(guidance) = generate_mcp_guidance(indexer.settings(), "get_calls", result_count)
        {
            result.push_str("\n---\n💡 ");
            result.push_str(&guidance);
            result.push('\n');
        }

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(
        description = "Find functions that CALL a given function (invoke it with parentheses).\n\nShows: what calls → function_name()\nDoes NOT show: Type references, component rendering, or what this function calls.\n\nUse analyze_impact for: Complete dependency graph including type usage and composition."
    )]
    pub async fn find_callers(
        &self,
        Parameters(FindCallersRequest {
            function_name,
            symbol_id,
        }): Parameters<FindCallersRequest>,
    ) -> Result<CallToolResult, McpError> {
        let indexer = self.facade.read().await;

        // Get the symbol either by ID or by name
        let (symbol, identifier) = if let Some(id) = symbol_id {
            // Direct lookup by symbol ID - UNAMBIGUOUS
            match indexer.get_symbol(crate::SymbolId(id)) {
                Some(sym) => (sym, format!("symbol_id:{id}")),
                None => {
                    return Ok(CallToolResult::success(vec![Content::text(format!(
                        "Symbol not found: symbol_id:{id}"
                    ))]));
                }
            }
        } else if let Some(name) = function_name {
            let symbols = indexer.find_symbols_by_name(&name, None);

            if symbols.is_empty() {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "Function not found: {name}"
                ))]));
            }

            if symbols.len() > 1 {
                // MULTIPLE MATCHES - Return error with list of symbol IDs
                let mut msg = format!(
                    "Ambiguous: found {} symbol(s) named '{}':\n",
                    symbols.len(),
                    name
                );
                for (i, sym) in symbols.iter().take(10).enumerate() {
                    msg.push_str(&format!(
                        "  {}. symbol_id:{} - {:?} at {}:{}\n",
                        i + 1,
                        sym.id.value(),
                        sym.kind,
                        sym.file_path,
                        sym.range.start_line + 1
                    ));
                }
                if symbols.len() > 10 {
                    msg.push_str(&format!("  ... and {} more\n", symbols.len() - 10));
                }
                msg.push_str("\nUse: find_callers symbol_id:<id> for specific symbol");
                return Ok(CallToolResult::success(vec![Content::text(msg)]));
            }

            // SINGLE MATCH - use it
            (symbols.into_iter().next().unwrap(), name)
        } else {
            return Ok(CallToolResult::success(vec![Content::text(
                "Error: Either function_name or symbol_id must be provided".to_string(),
            )]));
        };

        // Get callers for THIS SPECIFIC symbol only (no aggregation)
        let all_callers_with_metadata = indexer.get_calling_functions_with_metadata(symbol.id);

        if all_callers_with_metadata.is_empty() {
            let mut output = format!("No functions call {identifier}");
            // Add guidance for no results
            if let Some(guidance) = generate_mcp_guidance(indexer.settings(), "find_callers", 0) {
                output.push_str("\n\n---\n💡 ");
                output.push_str(&guidance);
                output.push('\n');
            }
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // Build structured text response with rich metadata
        let result_count = all_callers_with_metadata.len();
        let mut result = format!("{result_count} function(s) call {identifier}:\n");

        for (caller, metadata) in all_callers_with_metadata {
            // Parse metadata to extract receiver info and call site location
            let (call_info, call_line) = if let Some(ref meta) = metadata {
                let info = if let Some(context) = &meta.context {
                    if context.contains("receiver:") && context.contains("static:") {
                        // Parse "receiver:{receiver},static:{is_static}"
                        let parts: Vec<&str> = context.split(',').collect();
                        let mut receiver = "";
                        let mut is_static = false;

                        for part in parts {
                            if let Some(r) = part.strip_prefix("receiver:") {
                                receiver = r;
                            } else if let Some(s) = part.strip_prefix("static:") {
                                is_static = s == "true";
                            }
                        }

                        if !receiver.is_empty() {
                            let qualified_name = if is_static {
                                format!("{receiver}::{}", symbol.name)
                            } else {
                                format!("{receiver}.{}", symbol.name)
                            };
                            format!(" (calls {qualified_name})")
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                // Use call site line if available, otherwise definition line
                let line = meta
                    .line
                    .map(|l| l + 1)
                    .unwrap_or(caller.range.start_line + 1);
                (info, line)
            } else {
                (String::new(), caller.range.start_line + 1)
            };

            result.push_str(&format!(
                "  <- {:?} {} at {}:{}{}\n",
                caller.kind, caller.name, caller.file_path, call_line, call_info
            ));

            if let Some(ref sig) = caller.signature {
                result.push_str(&format!("     Signature: {sig}\n"));
            }
        }

        // Add system guidance
        if let Some(guidance) =
            generate_mcp_guidance(indexer.settings(), "find_callers", result_count)
        {
            result.push_str("\n---\n💡 ");
            result.push_str(&guidance);
            result.push('\n');
        }

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(
        description = "Analyze complete impact of changing a symbol. Shows ALL relationships: function calls, type usage, composition.\n\nShows:\n- What CALLS this function\n- What USES this as a type (fields, parameters, returns)\n- What RENDERS/COMPOSES this (JSX: <Component>, Rust: struct fields, etc.)\n- Full dependency graph across files\n\nUse this when: You need to see everything that depends on a symbol."
    )]
    pub async fn analyze_impact(
        &self,
        Parameters(AnalyzeImpactRequest {
            symbol_name,
            symbol_id,
            max_depth,
        }): Parameters<AnalyzeImpactRequest>,
    ) -> Result<CallToolResult, McpError> {
        use crate::symbol::context::ContextIncludes;

        let indexer = self.facade.read().await;

        // Get the symbol either by ID or by name
        let (symbol, identifier) = if let Some(id) = symbol_id {
            // Direct lookup by symbol ID - UNAMBIGUOUS
            match indexer.get_symbol(crate::SymbolId(id)) {
                Some(sym) => (sym, format!("symbol_id:{id}")),
                None => {
                    return Ok(CallToolResult::success(vec![Content::text(format!(
                        "Symbol not found: symbol_id:{id}"
                    ))]));
                }
            }
        } else if let Some(name) = symbol_name {
            let symbols = indexer.find_symbols_by_name(&name, None);

            if symbols.is_empty() {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "Symbol not found: {name}"
                ))]));
            }

            if symbols.len() > 1 {
                // MULTIPLE MATCHES - Return error with list of symbol IDs
                let mut msg = format!(
                    "Ambiguous: found {} symbol(s) named '{}':\n",
                    symbols.len(),
                    name
                );
                for (i, sym) in symbols.iter().take(10).enumerate() {
                    msg.push_str(&format!(
                        "  {}. symbol_id:{} - {:?} at {}:{}\n",
                        i + 1,
                        sym.id.value(),
                        sym.kind,
                        sym.file_path,
                        sym.range.start_line + 1
                    ));
                }
                if symbols.len() > 10 {
                    msg.push_str(&format!("  ... and {} more\n", symbols.len() - 10));
                }
                msg.push_str("\nUse: analyze_impact symbol_id:<id> for specific symbol");
                return Ok(CallToolResult::success(vec![Content::text(msg)]));
            }

            // SINGLE MATCH - use it
            (symbols.into_iter().next().unwrap(), name)
        } else {
            return Ok(CallToolResult::success(vec![Content::text(
                "Error: Either symbol_name or symbol_id must be provided".to_string(),
            )]));
        };

        // Analyze impact for THIS SPECIFIC symbol only (no aggregation)
        let impacted = indexer.get_impact_radius(symbol.id, Some(max_depth as usize));

        if impacted.is_empty() {
            let mut output = format!("No symbols would be impacted by changing {identifier}");
            // Add guidance for no results
            if let Some(guidance) = generate_mcp_guidance(indexer.settings(), "analyze_impact", 0) {
                output.push_str("\n\n---\n💡 ");
                output.push_str(&guidance);
                output.push('\n');
            }
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        let mut result = format!("Analyzing impact of changing: {identifier}\n");

        // Show the specific symbol being analyzed
        if let Some(ctx) = indexer.get_symbol_context(
            symbol.id,
            ContextIncludes::CALLERS | ContextIncludes::EXTENDS | ContextIncludes::USES,
        ) {
            let location = ctx.format_location();
            let direct_callers = ctx
                .relationships
                .called_by
                .as_ref()
                .map(|c| c.len())
                .unwrap_or(0);

            // For classes, also show inheritance info
            let inheritance_info = if matches!(
                symbol.kind,
                crate::SymbolKind::Class | crate::SymbolKind::Struct
            ) {
                let extends_count = ctx
                    .relationships
                    .extends
                    .as_ref()
                    .map(|e| e.len())
                    .unwrap_or(0);
                let extended_by_count = ctx
                    .relationships
                    .extended_by
                    .as_ref()
                    .map(|e| e.len())
                    .unwrap_or(0);

                if extends_count > 0 || extended_by_count > 0 {
                    format!(", extends: {extends_count}, extended by: {extended_by_count}")
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            // Show uses info for all symbols
            let uses_count = ctx
                .relationships
                .uses
                .as_ref()
                .map(|u| u.len())
                .unwrap_or(0);
            let used_by_count = ctx
                .relationships
                .used_by
                .as_ref()
                .map(|u| u.len())
                .unwrap_or(0);

            let uses_info = if uses_count > 0 || used_by_count > 0 {
                format!(", uses: {uses_count}, used by: {used_by_count}")
            } else {
                String::new()
            };

            result.push_str(&format!(
                "Symbol: {:?} at {} (direct callers: {}{}{})\n\n",
                symbol.kind, location, direct_callers, inheritance_info, uses_info
            ));
        }

        let impact_count = impacted.len();
        result.push_str(&format!(
            "Total impact: {impact_count} symbol(s) would be affected (max depth: {max_depth})\n"
        ));

        // Group by symbol kind
        let mut by_kind: std::collections::HashMap<crate::SymbolKind, Vec<Symbol>> =
            std::collections::HashMap::new();

        for id in impacted {
            if let Some(sym) = indexer.get_symbol(id) {
                by_kind.entry(sym.kind).or_default().push(sym);
            }
        }

        // Display grouped by kind with locations
        for (kind, symbols) in by_kind {
            result.push_str(&format!("\n{kind:?} ({}): \n", symbols.len()));
            for sym in symbols {
                result.push_str(&format!(
                    "  - {} at {}:{}\n",
                    sym.name,
                    sym.file_path,
                    sym.range.start_line + 1
                ));
            }
        }

        // Add system guidance
        if let Some(guidance) =
            generate_mcp_guidance(indexer.settings(), "analyze_impact", impact_count)
        {
            result.push_str("\n---\n💡 ");
            result.push_str(&guidance);
            result.push('\n');
        }

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get information about the indexed codebase")]
    pub async fn get_index_info(
        &self,
        Parameters(_params): Parameters<GetIndexInfoRequest>,
    ) -> Result<CallToolResult, McpError> {
        let indexer = self.facade.read().await;
        let symbol_count = indexer.symbol_count();
        let file_count = indexer.file_count();
        let relationship_count = indexer.relationship_count();

        // Efficiently count symbols by kind in one pass
        let mut kind_counts = std::collections::HashMap::new();
        for symbol in indexer.get_all_symbols() {
            *kind_counts.entry(symbol.kind).or_insert(0) += 1;
        }

        // Build symbol kinds display dynamically
        let mut kinds_display = String::new();

        // Sort by kind name for consistent output
        let mut sorted_kinds: Vec<_> = kind_counts.iter().collect();
        sorted_kinds.sort_by_key(|(kind, _)| format!("{kind:?}"));

        for (kind, count) in sorted_kinds {
            kinds_display.push_str(&format!("\n  - {kind:?}s: {count}"));
        }

        // Get semantic search info
        let semantic_info = if let Some(metadata) = indexer.get_semantic_metadata() {
            let binary_count = indexer.semantic_binary_index_count();
            let cached_files = indexer.semantic_embedded_file_count();
            let (float_bytes, binary_bytes) = indexer.semantic_memory_usage();
            format!(
                "\n\nSemantic Search:\n  - Status: Enabled\n  - Model: {}\n  - Embeddings: {} (float) + {} (binary)\n  - Dimensions: {}\n  - Cached files: {}\n  - Memory: {:.1} MB (float) + {:.1} KB (binary)\n  - Created: {}\n  - Updated: {}",
                metadata.model_name,
                metadata.embedding_count,
                binary_count,
                metadata.dimension,
                cached_files,
                float_bytes as f64 / (1024.0 * 1024.0),
                binary_bytes as f64 / 1024.0,
                format_relative_time(metadata.created_at),
                format_relative_time(metadata.updated_at)
            )
        } else {
            "\n\nSemantic Search:\n  - Status: Disabled".to_string()
        };

        let result = format!(
            "Index contains {symbol_count} symbols across {file_count} files.\n\nBreakdown:\n  - Symbols: {symbol_count}\n  - Relationships: {relationship_count}\n\nSymbol Kinds:{kinds_display}{semantic_info}"
        );

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(
        description = "Symbol-based search for code navigation. Returns function/class/method symbols using hybrid retrieval (BM25 + vector + reranking). Use when you need exact APIs and definitions."
    )]
    pub async fn semantic_search_docs(
        &self,
        Parameters(SemanticSearchRequest {
            query,
            limit,
            threshold,
            lang,
        }): Parameters<SemanticSearchRequest>,
    ) -> Result<CallToolResult, McpError> {
        let indexer = self.facade.read().await;

        tracing::debug!(
            target: "mcp",
            "semantic_search_docs called - symbols: {}, semantic: {}",
            indexer.symbol_count(),
            indexer.has_semantic_search()
        );

        // Use hybrid search (BM25 + vector with RRF merge)
        // Falls back gracefully: BM25-only if semantic not enabled, vector-only if BM25 fails
        let results = match threshold {
            Some(t) => {
                indexer.hybrid_search_with_threshold(&query, limit as usize, t, lang.as_deref())
            }
            None => indexer.hybrid_search(&query, limit as usize, lang.as_deref()),
        };

        match results {
            Ok(results) => {
                if results.is_empty() {
                    let mut output =
                        format!("No semantically similar documentation found for: {query}");
                    // Add guidance for no results
                    if let Some(guidance) =
                        generate_mcp_guidance(indexer.settings(), "semantic_search_docs", 0)
                    {
                        output.push_str("\n\n---\n💡 ");
                        output.push_str(&guidance);
                        output.push('\n');
                    }
                    return Ok(CallToolResult::success(vec![Content::text(output)]));
                }

                let mut result = format!(
                    "Found {} semantically similar result(s) for '{}':\n\n",
                    results.len(),
                    query
                );

                for (i, (symbol, score)) in results.iter().enumerate() {
                    result.push_str(&format!(
                        "{}. {} ({:?}) - Similarity: {:.3}\n",
                        i + 1,
                        symbol.name,
                        symbol.kind,
                        score
                    ));
                    result.push_str(&format!(
                        "   File: {}:{}\n",
                        symbol.file_path,
                        symbol.range.start_line + 1
                    ));

                    if let Some(ref doc) = symbol.doc_comment {
                        // Show first 3 lines of doc
                        let preview: Vec<&str> = doc.lines().take(3).collect();
                        let doc_preview = if doc.lines().count() > 3 {
                            format!("{}...", preview.join(" "))
                        } else {
                            preview.join(" ")
                        };
                        result.push_str(&format!("   Doc: {doc_preview}\n"));
                    }

                    if let Some(ref sig) = symbol.signature {
                        result.push_str(&format!("   Signature: {sig}\n"));
                    }

                    result.push('\n');
                }

                // Add system guidance
                if let Some(guidance) =
                    generate_mcp_guidance(indexer.settings(), "semantic_search_docs", results.len())
                {
                    result.push_str("\n---\n💡 ");
                    result.push_str(&guidance);
                    result.push('\n');
                }

                Ok(CallToolResult::success(vec![Content::text(result)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Semantic search failed: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Chunk-based search for understanding code flow and logic. Returns \
                       line-ranged snippets using hybrid retrieval: vector + BM25, fused on \
                       normalised scores, one chunk per file. Recall rises steeply with the \
                       limit you ask for - measured across four repositories, the file you \
                       want is in the results 76% of the time at limit 10, 83% at 20 (the \
                       default), 87% at 30 and 90% at 50, for 6-20 ms either way. Ask for \
                       more when the first answer matters more than the tokens it costs."
    )]
    pub async fn semantic_search_chunks(
        &self,
        Parameters(ChunkSearchRequest {
            query,
            limit,
            threshold,
            lang,
        }): Parameters<ChunkSearchRequest>,
    ) -> Result<CallToolResult, McpError> {
        let indexer = self.facade.read().await;

        let search = match threshold {
            Some(t) => indexer.hybrid_chunk_search_with_threshold_detailed(
                &query,
                limit as usize,
                t,
                lang.as_deref(),
            ),
            None => indexer.hybrid_chunk_search_detailed(&query, limit as usize, lang.as_deref()),
        };

        match search {
            Ok(outcome) => {
                if outcome.results.is_empty() {
                    let mut output = format!("No code chunks found for query: {query}");
                    if outcome.bm25_only_fallback {
                        output.push_str(
                            "\n\nWARNING: Vector recall unavailable; results are BM25-only fallback.",
                        );
                    }
                    if let Some(guidance) =
                        generate_mcp_guidance(indexer.settings(), "semantic_search_chunks", 0)
                    {
                        output.push_str("\n\n---\n💡 ");
                        output.push_str(&guidance);
                        output.push('\n');
                    }
                    return Ok(CallToolResult::success(vec![Content::text(output)]));
                }

                let mut result = format!(
                    "Found {} strong chunk match(es) for '{}' ({} weak hidden):\n\n",
                    outcome.results.len(),
                    query,
                    outcome.weak_count
                );
                for (i, item) in outcome.results.iter().enumerate() {
                    result.push_str(&format!(
                        "{}. chunk:{} - Score: {:.3}\n",
                        i + 1,
                        item.chunk_id,
                        item.score
                    ));
                    result.push_str(&format!(
                        "   File: {}:{}-{}\n",
                        item.filepath,
                        item.line_start + 1,
                        item.line_end + 1
                    ));
                    if let Some(scope) = item.parent_scope.as_deref() {
                        if !scope.is_empty() {
                            result.push_str(&format!("   Scope: {scope}\n"));
                        }
                    }
                    if let Some(lang) = item.language.as_deref() {
                        result.push_str(&format!("   Language: {lang}\n"));
                    }
                    let preview: String =
                        item.snippet.lines().take(6).collect::<Vec<_>>().join("\n");
                    result.push_str("   Snippet:\n");
                    result.push_str(&format!("{}\n\n", preview));
                }
                if !outcome.pruned_by.is_empty() {
                    result.push_str(&format!(
                        "Applied pruning: {}\n\n",
                        outcome.pruned_by.join(", ")
                    ));
                }
                if outcome.bm25_only_fallback {
                    result.push_str(
                        "WARNING: Vector recall unavailable; results are BM25-only fallback.\n\n",
                    );
                }

                if let Some(guidance) = generate_mcp_guidance(
                    indexer.settings(),
                    "semantic_search_chunks",
                    outcome.results.len(),
                ) {
                    result.push_str("\n---\n💡 ");
                    result.push_str(&guidance);
                    result.push('\n');
                }

                Ok(CallToolResult::success(vec![Content::text(result)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Chunk search failed: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Search by natural language and get full context: documentation, dependencies, callers, impact.\n\nReturns symbols with:\n- Their documentation\n- What calls them\n- What they call\n- Complete impact graph (includes ALL relationships: calls, type usage, composition)\n\nUse this when: You want to find and understand symbols with their complete usage context."
    )]
    pub async fn semantic_search_with_context(
        &self,
        Parameters(SemanticSearchWithContextRequest {
            query,
            limit,
            threshold,
            lang,
        }): Parameters<SemanticSearchWithContextRequest>,
    ) -> Result<CallToolResult, McpError> {
        let indexer = self.facade.read().await;

        // Use hybrid search (BM25 + vector with RRF merge)
        let search_results = match threshold {
            Some(t) => {
                indexer.hybrid_search_with_threshold(&query, limit as usize, t, lang.as_deref())
            }
            None => indexer.hybrid_search(&query, limit as usize, lang.as_deref()),
        };

        match search_results {
            Ok(results) => {
                if results.is_empty() {
                    let mut output = format!("No documentation found matching query: {query}");
                    // Add guidance for no results
                    if let Some(guidance) =
                        generate_mcp_guidance(indexer.settings(), "semantic_search_with_context", 0)
                    {
                        output.push_str("\n\n---\n💡 ");
                        output.push_str(&guidance);
                        output.push('\n');
                    }
                    return Ok(CallToolResult::success(vec![Content::text(output)]));
                }

                let mut output = String::new();
                output.push_str(&format!(
                    "Found {} results for query: '{}'\n\n",
                    results.len(),
                    query
                ));

                // For each result, gather comprehensive context
                for (idx, (symbol, score)) in results.iter().enumerate() {
                    // Basic symbol information - matching find_symbol format
                    output.push_str(&format!(
                        "{}. {} - {:?} at {} [symbol_id:{}]\n",
                        idx + 1,
                        symbol.name,
                        symbol.kind,
                        crate::symbol::context::SymbolContext::symbol_location(symbol),
                        symbol.id.value()
                    ));
                    output.push_str(&format!("   Similarity Score: {score:.3}\n"));

                    // Documentation
                    if let Some(ref doc) = symbol.doc_comment {
                        output.push_str("   Documentation:\n");
                        for line in doc.lines().take(5) {
                            output.push_str(&format!("     {line}\n"));
                        }
                        if doc.lines().count() > 5 {
                            output.push_str("     ...\n");
                        }
                    }

                    // Signature
                    if let Some(ref sig) = symbol.signature {
                        output.push_str(&format!("   Signature: {sig}\n"));
                    }

                    // Only gather additional context for functions/methods
                    if matches!(
                        symbol.kind,
                        crate::SymbolKind::Function | crate::SymbolKind::Method
                    ) {
                        // Dependencies (what this function calls) - using logic from get_calls
                        let called_with_metadata =
                            indexer.get_called_functions_with_metadata(symbol.id);
                        if !called_with_metadata.is_empty() {
                            output.push_str(&format!(
                                "\n   {} calls {} function(s):\n",
                                symbol.name,
                                called_with_metadata.len()
                            ));
                            for (i, (called, metadata)) in
                                called_with_metadata.iter().take(10).enumerate()
                            {
                                // Parse receiver information from metadata and get call site location
                                let (call_display, call_line) = if let Some(meta) = metadata {
                                    let display = if let Some(context) = &meta.context {
                                        if context.contains("receiver:")
                                            && context.contains("static:")
                                        {
                                            let parts: Vec<&str> = context.split(',').collect();
                                            let mut receiver = None;
                                            let mut is_static = false;

                                            for part in parts {
                                                if let Some(recv) = part.strip_prefix("receiver:") {
                                                    receiver = Some(recv.trim());
                                                } else if let Some(static_val) =
                                                    part.strip_prefix("static:")
                                                {
                                                    is_static = static_val.trim() == "true";
                                                }
                                            }

                                            match (receiver, is_static) {
                                                (Some("self"), false) => {
                                                    format!("(self.{})", called.name)
                                                }
                                                (Some(recv), true) if recv != "self" => {
                                                    format!("({}::{})", recv, called.name)
                                                }
                                                (Some(recv), false) if recv != "self" => {
                                                    format!("({}.{})", recv, called.name)
                                                }
                                                _ => called.name.to_string(),
                                            }
                                        } else {
                                            called.name.to_string()
                                        }
                                    } else {
                                        called.name.to_string()
                                    };

                                    // Use call site line if available
                                    let line = meta
                                        .line
                                        .map(|l| l + 1)
                                        .unwrap_or(called.range.start_line + 1);
                                    (display, line)
                                } else {
                                    (called.name.to_string(), called.range.start_line + 1)
                                };

                                output.push_str(&format!(
                                    "     -> {:?} {} at {}:{} [symbol_id:{}]\n",
                                    called.kind,
                                    call_display,
                                    called.file_path,
                                    call_line,
                                    called.id.value()
                                ));
                                if i == 9 && called_with_metadata.len() > 10 {
                                    output.push_str(&format!(
                                        "     ... and {} more\n",
                                        called_with_metadata.len() - 10
                                    ));
                                }
                            }
                        }

                        // Callers (who uses this function) - using logic from find_callers
                        let calling_functions_with_metadata =
                            indexer.get_calling_functions_with_metadata(symbol.id);
                        if !calling_functions_with_metadata.is_empty() {
                            output.push_str(&format!(
                                "\n   {} function(s) call {}:\n",
                                calling_functions_with_metadata.len(),
                                symbol.name
                            ));
                            for (i, (caller, metadata)) in
                                calling_functions_with_metadata.iter().take(10).enumerate()
                            {
                                // Parse metadata to extract receiver info and call site location
                                let (call_info, call_line) = if let Some(meta) = metadata {
                                    let info = if let Some(context) = &meta.context {
                                        if context.contains("receiver:")
                                            && context.contains("static:")
                                        {
                                            // Parse "receiver:{receiver},static:{is_static}"
                                            let parts: Vec<&str> = context.split(',').collect();
                                            let mut receiver = "";
                                            let mut is_static = false;

                                            for part in parts {
                                                if let Some(r) = part.strip_prefix("receiver:") {
                                                    receiver = r;
                                                } else if let Some(s) = part.strip_prefix("static:")
                                                {
                                                    is_static = s == "true";
                                                }
                                            }

                                            if !receiver.is_empty() {
                                                let qualified_name = if is_static {
                                                    format!("{}::{}", receiver, symbol.name)
                                                } else {
                                                    format!("{}.{}", receiver, symbol.name)
                                                };
                                                format!(" (calls {qualified_name})")
                                            } else {
                                                String::new()
                                            }
                                        } else {
                                            String::new()
                                        }
                                    } else {
                                        String::new()
                                    };

                                    // Use call site line if available
                                    let line = meta
                                        .line
                                        .map(|l| l + 1)
                                        .unwrap_or(caller.range.start_line + 1);
                                    (info, line)
                                } else {
                                    (String::new(), caller.range.start_line + 1)
                                };

                                output.push_str(&format!(
                                    "     <- {:?} {} at {}:{}{} [symbol_id:{}]\n",
                                    caller.kind,
                                    caller.name,
                                    caller.file_path,
                                    call_line,
                                    call_info,
                                    caller.id.value()
                                ));
                                if i == 9 && calling_functions_with_metadata.len() > 10 {
                                    output.push_str(&format!(
                                        "     ... and {} more\n",
                                        calling_functions_with_metadata.len() - 10
                                    ));
                                }
                            }
                        }

                        // Impact analysis - using logic from analyze_impact
                        let impacted = indexer.get_impact_radius(symbol.id, Some(2));
                        if !impacted.is_empty() {
                            output.push_str(&format!(
                                "\n   Changing {} would impact {} symbol(s) (max depth: 2):\n",
                                symbol.name,
                                impacted.len()
                            ));

                            // Get details and group by kind
                            let impacted_details: Vec<_> = impacted
                                .iter()
                                .filter_map(|id| indexer.get_symbol(*id))
                                .collect();

                            // Group by kind
                            let mut methods = Vec::new();
                            let mut functions = Vec::new();
                            let mut other = Vec::new();

                            for sym in impacted_details {
                                match sym.kind {
                                    crate::SymbolKind::Method => methods.push(sym),
                                    crate::SymbolKind::Function => functions.push(sym),
                                    _ => other.push(sym),
                                }
                            }

                            if !methods.is_empty() {
                                output.push_str(&format!("\n     methods ({}):\n", methods.len()));
                                for method in methods.iter().take(5) {
                                    output.push_str(&format!(
                                        "       - {} [symbol_id:{}]\n",
                                        method.name,
                                        method.id.value()
                                    ));
                                }
                                if methods.len() > 5 {
                                    output.push_str(&format!(
                                        "       ... and {} more\n",
                                        methods.len() - 5
                                    ));
                                }
                            }

                            if !functions.is_empty() {
                                output.push_str(&format!(
                                    "\n     functions ({}):\n",
                                    functions.len()
                                ));
                                for func in functions.iter().take(5) {
                                    output.push_str(&format!(
                                        "       - {} [symbol_id:{}]\n",
                                        func.name,
                                        func.id.value()
                                    ));
                                }
                                if functions.len() > 5 {
                                    output.push_str(&format!(
                                        "       ... and {} more\n",
                                        functions.len() - 5
                                    ));
                                }
                            }

                            if !other.is_empty() {
                                output.push_str(&format!("\n     other ({}):\n", other.len()));
                                for sym in other.iter().take(3) {
                                    output.push_str(&format!(
                                        "       - {} ({:?}) [symbol_id:{}]\n",
                                        sym.name,
                                        sym.kind,
                                        sym.id.value()
                                    ));
                                }
                            }
                        }
                    }

                    // Show inheritance relationships for classes/structs/enums
                    if matches!(
                        symbol.kind,
                        crate::SymbolKind::Class
                            | crate::SymbolKind::Struct
                            | crate::SymbolKind::Enum
                    ) {
                        // What does this class extend?
                        let extends = indexer.get_extends(symbol.id);
                        if !extends.is_empty() {
                            output.push_str(&format!(
                                "\n   {} extends {} class(es):\n",
                                symbol.name,
                                extends.len()
                            ));
                            for (i, base_class) in extends.iter().take(5).enumerate() {
                                output.push_str(&format!(
                                    "     -> {:?} {} at {} [symbol_id:{}]\n",
                                    base_class.kind,
                                    base_class.name,
                                    crate::symbol::context::SymbolContext::symbol_location(
                                        base_class
                                    ),
                                    base_class.id.value()
                                ));
                                if i == 4 && extends.len() > 5 {
                                    output.push_str(&format!(
                                        "     ... and {} more\n",
                                        extends.len() - 5
                                    ));
                                }
                            }
                        }

                        // What classes extend this class?
                        let extended_by = indexer.get_extended_by(symbol.id);
                        if !extended_by.is_empty() {
                            output.push_str(&format!(
                                "\n   {} class(es) extend {}:\n",
                                extended_by.len(),
                                symbol.name
                            ));
                            for (i, derived_class) in extended_by.iter().take(5).enumerate() {
                                output.push_str(&format!(
                                    "     <- {:?} {} at {} [symbol_id:{}]\n",
                                    derived_class.kind,
                                    derived_class.name,
                                    crate::symbol::context::SymbolContext::symbol_location(
                                        derived_class
                                    ),
                                    derived_class.id.value()
                                ));
                                if i == 4 && extended_by.len() > 5 {
                                    output.push_str(&format!(
                                        "     ... and {} more\n",
                                        extended_by.len() - 5
                                    ));
                                }
                            }
                        }

                        // What traits does this type implement?
                        let implements = indexer.get_implemented_traits(symbol.id);
                        if !implements.is_empty() {
                            output.push_str(&format!(
                                "\n   {} implements {} trait(s):\n",
                                symbol.name,
                                implements.len()
                            ));
                            for (i, trait_sym) in implements.iter().take(5).enumerate() {
                                output.push_str(&format!(
                                    "     -> {:?} {} at {} [symbol_id:{}]\n",
                                    trait_sym.kind,
                                    trait_sym.name,
                                    crate::symbol::context::SymbolContext::symbol_location(
                                        trait_sym
                                    ),
                                    trait_sym.id.value()
                                ));
                                if i == 4 && implements.len() > 5 {
                                    output.push_str(&format!(
                                        "     ... and {} more\n",
                                        implements.len() - 5
                                    ));
                                }
                            }
                        }
                    }

                    // Show what implements this trait/interface
                    if matches!(
                        symbol.kind,
                        crate::SymbolKind::Trait | crate::SymbolKind::Interface
                    ) {
                        let implementations = indexer.get_implementations(symbol.id);
                        if !implementations.is_empty() {
                            output.push_str(&format!(
                                "\n   {} type(s) implement {}:\n",
                                implementations.len(),
                                symbol.name
                            ));
                            for (i, impl_sym) in implementations.iter().take(5).enumerate() {
                                output.push_str(&format!(
                                    "     <- {:?} {} at {} [symbol_id:{}]\n",
                                    impl_sym.kind,
                                    impl_sym.name,
                                    crate::symbol::context::SymbolContext::symbol_location(
                                        impl_sym
                                    ),
                                    impl_sym.id.value()
                                ));
                                if i == 4 && implementations.len() > 5 {
                                    output.push_str(&format!(
                                        "     ... and {} more\n",
                                        implementations.len() - 5
                                    ));
                                }
                            }
                        }
                    }

                    // Show uses relationships (for all symbols)
                    let uses = indexer.get_uses(symbol.id);
                    if !uses.is_empty() {
                        output.push_str(&format!(
                            "\n   {} uses {} type(s):\n",
                            symbol.name,
                            uses.len()
                        ));
                        for (i, used_type) in uses.iter().take(5).enumerate() {
                            output.push_str(&format!(
                                "     -> {:?} {} at {} [symbol_id:{}]\n",
                                used_type.kind,
                                used_type.name,
                                crate::symbol::context::SymbolContext::symbol_location(used_type),
                                used_type.id.value()
                            ));
                            if i == 4 && uses.len() > 5 {
                                output.push_str(&format!("     ... and {} more\n", uses.len() - 5));
                            }
                        }
                    }

                    // What symbols use this type?
                    let used_by = indexer.get_used_by(symbol.id);
                    if !used_by.is_empty() {
                        output.push_str(&format!(
                            "\n   {} type(s) use {}:\n",
                            used_by.len(),
                            symbol.name
                        ));
                        for (i, using_symbol) in used_by.iter().take(5).enumerate() {
                            output.push_str(&format!(
                                "     <- {:?} {} at {} [symbol_id:{}]\n",
                                using_symbol.kind,
                                using_symbol.name,
                                crate::symbol::context::SymbolContext::symbol_location(
                                    using_symbol
                                ),
                                using_symbol.id.value()
                            ));
                            if i == 4 && used_by.len() > 5 {
                                output.push_str(&format!(
                                    "     ... and {} more\n",
                                    used_by.len() - 5
                                ));
                            }
                        }
                    }

                    output.push('\n');
                }

                // Add system guidance
                if let Some(guidance) = generate_mcp_guidance(
                    indexer.settings(),
                    "semantic_search_with_context",
                    results.len(),
                ) {
                    output.push_str("\n---\n💡 ");
                    output.push_str(&guidance);
                    output.push('\n');
                }

                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Semantic search failed: {e}"
            ))])),
        }
    }

    #[tool(description = "Search for symbols using full-text search with fuzzy matching")]
    pub async fn search_symbols(
        &self,
        Parameters(SearchSymbolsRequest {
            query,
            limit,
            kind,
            module,
            lang,
        }): Parameters<SearchSymbolsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let indexer = self.facade.read().await;

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

        match indexer.search(
            &query,
            limit as usize,
            kind_filter,
            module.as_deref(),
            lang.as_deref(),
        ) {
            Ok(results) => {
                if results.is_empty() {
                    let mut output = format!("No results found for query: {query}");
                    // Add guidance for no results
                    if let Some(guidance) =
                        generate_mcp_guidance(indexer.settings(), "search_symbols", 0)
                    {
                        output.push_str("\n\n---\n💡 ");
                        output.push_str(&guidance);
                        output.push('\n');
                    }
                    return Ok(CallToolResult::success(vec![Content::text(output)]));
                }

                let mut result = format!(
                    "Found {} result(s) for query '{}':\n\n",
                    results.len(),
                    query
                );

                for (i, search_result) in results.iter().enumerate() {
                    result.push_str(&format!(
                        "{}. {} ({:?})\n",
                        i + 1,
                        search_result.name,
                        search_result.kind
                    ));
                    result.push_str(&format!(
                        "   File: {}:{}\n",
                        search_result.file_path, search_result.line
                    ));

                    if !search_result.module_path.is_empty() {
                        result.push_str(&format!("   Module: {}\n", search_result.module_path));
                    }

                    if let Some(ref doc) = search_result.doc_comment {
                        // Show first line of doc comment
                        let first_line = doc.lines().next().unwrap_or("");
                        result.push_str(&format!("   Doc: {first_line}\n"));
                    }

                    if let Some(ref sig) = search_result.signature {
                        result.push_str(&format!("   Signature: {sig}\n"));
                    }

                    result.push_str(&format!("   Score: {:.2}\n", search_result.score));
                    result.push('\n');
                }

                // Add system guidance
                if let Some(guidance) =
                    generate_mcp_guidance(indexer.settings(), "search_symbols", results.len())
                {
                    result.push_str("\n---\n💡 ");
                    result.push_str(&guidance);
                    result.push('\n');
                }

                Ok(CallToolResult::success(vec![Content::text(result)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Search failed: {e}"
            ))])),
        }
    }

    #[tool(
        description = "Get comprehensive feature context: symbol info + source code + relationships + impact analysis + call examples in one call. Use this instead of multiple separate tool calls for complete context."
    )]
    pub async fn get_feature_context(
        &self,
        Parameters(GetFeatureContextRequest {
            symbol_id,
            include_source,
            include_impact,
            show_call_examples,
            max_callers: _max_callers,
            max_calls: _max_calls,
            max_impact,
            impact_depth,
            max_examples,
            context_lines,
        }): Parameters<GetFeatureContextRequest>,
    ) -> Result<CallToolResult, McpError> {
        use crate::symbol::context::ContextIncludes;

        let indexer = self.facade.read().await;

        // 1. Get symbol
        let symbol = match indexer.get_symbol(crate::SymbolId(symbol_id)) {
            Some(s) => s,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Symbol not found with id: {symbol_id}"
                ))]));
            }
        };

        // 2. Get comprehensive symbol context (relationships)
        let context =
            match indexer.get_symbol_context(crate::SymbolId(symbol_id), ContextIncludes::ALL) {
                Some(ctx) => ctx,
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Failed to retrieve context for symbol_id: {symbol_id}"
                    ))]));
                }
            };

        // 3. Build output starting with formatted context
        let mut output = context.format_full("");

        // 4. Add source code if requested
        if include_source {
            let file_path = std::path::Path::new(symbol.file_path.as_ref());
            if let Ok(content) = std::fs::read_to_string(file_path) {
                let lines: Vec<&str> = content.lines().collect();
                let start = symbol.range.start_line as usize;
                let end = (symbol.range.end_line as usize).min(lines.len().saturating_sub(1));

                let ctx = context_lines as usize;
                let display_start = start.saturating_sub(ctx);
                let display_end = (end + ctx).min(lines.len().saturating_sub(1));

                output.push_str(
                    "\n───────────────────────────────────────────────────────────────\n",
                );
                output.push_str("\n### Source Code\n\n");

                let lang_hint = symbol
                    .language_id
                    .map(|l| format!("{l:?}").to_lowercase())
                    .unwrap_or_default();
                output.push_str(&format!("```{lang_hint}\n"));

                for i in display_start..=display_end {
                    if i < lines.len() {
                        output.push_str(&format!("{:>5} | {}\n", i + 1, lines[i]));
                    }
                }

                output.push_str("```\n");
            }
        }

        // 5. Add impact analysis if requested
        if include_impact {
            let impact_ids =
                indexer.get_impact_radius(crate::SymbolId(symbol_id), Some(impact_depth as usize));

            if !impact_ids.is_empty() {
                output
                    .push_str("\n───────────────────────────────────────────────────────────────");
                output.push_str(&format_impact_graph(
                    &indexer,
                    &impact_ids,
                    max_impact,
                    impact_depth,
                ));
            }
        }

        // 6. Add call site examples if requested
        if show_call_examples {
            let callers_with_meta =
                indexer.get_calling_functions_with_metadata(crate::SymbolId(symbol_id));

            if !callers_with_meta.is_empty() {
                output
                    .push_str("\n───────────────────────────────────────────────────────────────");
                output.push_str(&format_call_examples(
                    &callers_with_meta,
                    max_examples,
                    3, // Fixed 3 lines context for call examples
                ));
            }
        }

        // 7. Add guidance
        output.push_str("\n───────────────────────────────────────────────────────────────\n");
        if let Some(guidance) = generate_mcp_guidance(indexer.settings(), "get_feature_context", 1)
        {
            output.push_str("💡 ");
            output.push_str(&guidance);
            output.push('\n');
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "Get recursive downstream call tree showing what this symbol calls with full depth. \
                       Returns hierarchical tree showing complete execution flow. Use this to understand \
                       what happens when a function runs. Opposite of find_callers (which shows upstream). \
                       Example: get_call_tree for SignupHandler shows validateInput→checkEmail, hashPassword→bcrypt, etc."
    )]
    pub async fn get_call_tree(
        &self,
        Parameters(GetCallTreeRequest {
            symbol_id,
            max_depth,
            include_source,
            include_metadata,
            show_external_calls,
            max_nodes,
            collapse_duplicates,
            show_trivial,
        }): Parameters<GetCallTreeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let indexer = self.facade.read().await;

        // Get root symbol
        let symbol = match indexer.get_symbol(crate::SymbolId(symbol_id)) {
            Some(s) => s,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Symbol not found with id: {symbol_id}"
                ))]));
            }
        };

        // Build call tree
        let mut tree = indexer.get_call_tree(
            crate::SymbolId(symbol_id),
            max_depth as usize,
            max_nodes as usize,
        );

        // Filter external calls if requested
        if !show_external_calls {
            fn filter_external_recursive(nodes: &mut Vec<crate::relationship::CallTreeNode>) {
                nodes.retain(|node| !node.is_external);
                for node in nodes.iter_mut() {
                    filter_external_recursive(&mut node.children);
                }
            }
            filter_external_recursive(&mut tree);
        }

        // Format output
        let output = format_call_tree(
            &symbol,
            &tree,
            include_source,
            include_metadata,
            collapse_duplicates,
            !show_trivial,
        );

        // Add guidance
        let mut final_output = output;
        if let Some(guidance) =
            generate_mcp_guidance(indexer.settings(), "get_call_tree", tree.len())
        {
            final_output.push_str("\n💡 ");
            final_output.push_str(&guidance);
            final_output.push('\n');
        }

        Ok(CallToolResult::success(vec![Content::text(final_output)]))
    }

    #[tool(
        description = "Get comprehensive project overview with module architecture, tech stack, dependencies, \
                       and high-level relationship graph. Returns a big-picture understanding of the codebase \
                       structure. Use this as a starting point before diving into specific modules with other tools. \
                       Shows: module structure with file/symbol counts, tech stack detection, entry points, \
                       module descriptions (based on imports/calls), and optional relationship graph with layer detection."
    )]
    pub async fn get_project_overview(
        &self,
        Parameters(GetProjectOverviewRequest {
            include_graph,
            include_dependencies,
            module_depth,
        }): Parameters<GetProjectOverviewRequest>,
    ) -> Result<CallToolResult, McpError> {
        let indexer = self.facade.read().await;

        // Get all symbols to extract file paths
        let all_symbols = indexer.get_all_symbols();
        if all_symbols.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text(
                "No symbols indexed. Run 'codanna index <directory>' first.".to_string(),
            )]));
        }

        // Extract unique file paths from symbols
        let mut file_paths: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
        for symbol in &all_symbols {
            file_paths.insert(PathBuf::from(symbol.file_path.as_ref()));
        }
        let indexed_files: Vec<PathBuf> = file_paths.into_iter().collect();

        // Detect primary language and load knowledge base
        let lang_counts = detect_primary_language(&indexed_files);
        let primary_language = lang_counts
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(lang, _)| lang.as_str())
            .unwrap_or("unknown");

        let knowledge_base = if let Some(ref workspace_root) = indexer.settings().workspace_root {
            load_knowledge_base(workspace_root)
        } else {
            None
        };

        let mut output = String::new();

        // Detect workspace packages for monorepo support
        let workspace_packages = if let Some(ref workspace_root) = indexer.settings().workspace_root
        {
            detect_workspace_packages(workspace_root)
        } else {
            Vec::new()
        };

        // Parse dependencies early (needed for header and tech stack)
        let mut dep_info: Option<DependencyInfo> = None;
        if let Some(ref workspace_root) = indexer.settings().workspace_root {
            let package_files = find_package_files(workspace_root);
            for pkg_file in package_files {
                if pkg_file.ends_with("package.json") {
                    dep_info = parse_package_json(&pkg_file);
                } else if pkg_file.ends_with("Cargo.toml") {
                    dep_info = parse_cargo_toml(&pkg_file);
                }
                if dep_info.is_some() {
                    break;
                }
            }

            // Aggregate dependencies from workspace sub-packages
            if !workspace_packages.is_empty() {
                if let Some(ref mut info) = dep_info {
                    let mut pkg_names = Vec::new();
                    for ws_pkg in &workspace_packages {
                        let abs_pkg = workspace_root.join(ws_pkg);
                        let sub_dep = if abs_pkg.join("package.json").exists() {
                            parse_package_json(&abs_pkg.join("package.json"))
                        } else if abs_pkg.join("Cargo.toml").exists() {
                            parse_cargo_toml(&abs_pkg.join("Cargo.toml"))
                        } else {
                            None
                        };
                        if let Some(sub) = sub_dep {
                            pkg_names.push(sub.package_name.clone());
                            // Merge dependencies (deduplicate by name)
                            for (name, ver) in sub.dependencies {
                                if !info.dependencies.iter().any(|(n, _)| n == &name) {
                                    info.dependencies.push((name, ver));
                                }
                            }
                            for (name, ver) in sub.dev_dependencies {
                                if !info.dev_dependencies.iter().any(|(n, _)| n == &name) {
                                    info.dev_dependencies.push((name, ver));
                                }
                            }
                        }
                    }
                    info.workspace_packages = pkg_names;
                }
            }
        }

        // Header with metadata (plain-text, no emoji)
        output
            .push_str("╔══════════════════════════════════════════════════════════════════════\n");

        let project_name = dep_info
            .as_ref()
            .map(|d| d.package_name.as_str())
            .unwrap_or("project");
        output.push_str(&format!("║ PROJECT: {}\n", project_name));

        // Build indexed_symbols map for filtering
        let mut indexed_symbols: HashMap<PathBuf, Vec<&crate::symbol::Symbol>> = HashMap::new();
        for symbol in &all_symbols {
            indexed_symbols
                .entry(PathBuf::from(symbol.file_path.as_ref()))
                .or_default()
                .push(symbol);
        }

        // Group files by module (workspace-aware, filters single-file low-value modules)
        let modules = group_files_by_module(
            &indexed_files,
            module_depth,
            &workspace_packages,
            &indexed_symbols,
        );

        output.push_str(&format!(
            "║ Total Files: {} | Modules: {} (depth: {}) | Symbols: {}\n",
            indexed_files.len(),
            modules.len(),
            module_depth,
            all_symbols.len()
        ));

        let lang_dist = format_language_distribution(&lang_counts);
        output.push_str(&format!("║ Primary Language: {}\n", lang_dist));

        if let Some(ref workspace_root) = indexer.settings().workspace_root {
            let arch = detect_architecture(workspace_root);
            if !workspace_packages.is_empty() {
                output.push_str(&format!(
                    "║ Architecture: {} ({} packages)\n",
                    arch,
                    workspace_packages.len()
                ));
            } else {
                output.push_str(&format!("║ Architecture: {}\n", arch));
            }
        }

        let proj_type = detect_project_type(&dep_info);
        output.push_str(&format!("║ Type: {}\n", proj_type));

        output.push_str(
            "╚══════════════════════════════════════════════════════════════════════\n\n",
        );

        // Tech Stack
        if include_dependencies {
            output.push_str(
                "# ── Tech Stack ────────────────────────────────────────────────────────\n\n",
            );
            let tech_stack = format_tech_stack(&dep_info, &knowledge_base, primary_language);
            if !tech_stack.is_empty() {
                output.push_str(&tech_stack);
            } else if dep_info.is_some() {
                // Show basic tech stack info even without KB categorization
                let info = dep_info.as_ref().unwrap();
                output.push_str(&format!("  Stack: {}\n", info.tech_stack));
                let dep_count = info.dependencies.len() + info.dev_dependencies.len();
                if dep_count > 0 {
                    let top_deps: Vec<_> = info
                        .dependencies
                        .iter()
                        .take(10)
                        .map(|(name, ver)| format!("{} {}", name, ver))
                        .collect();
                    output.push_str(&format!("  Dependencies: {}\n", top_deps.join(", ")));
                    if info.dependencies.len() > 10 {
                        output.push_str(&format!(
                            "  (+{} more deps, {} dev deps)\n",
                            info.dependencies.len() - 10,
                            info.dev_dependencies.len()
                        ));
                    }
                }
                output.push('\n');
            } else {
                output.push_str("  No dependencies detected.\n\n");
            }
        }

        // Entry points detection
        let mut entry_points = detect_entry_points(&all_symbols);

        // Add workspace bin entry points from package.json
        if let Some(ref workspace_root) = indexer.settings().workspace_root {
            detect_workspace_entry_points(
                workspace_root,
                &workspace_packages,
                &all_symbols,
                &mut entry_points,
            );
        }

        // Add Cargo.toml [[bin]] entry points
        detect_cargo_bin_entry_points(&indexer, &all_symbols, &mut entry_points);

        if !entry_points.is_empty() {
            output.push_str(
                "# ── Entry Points ──────────────────────────────────────────────────────\n\n",
            );
            for ep in entry_points.iter().take(10) {
                output.push_str(&format!(
                    "  [{}] {}  {}:{}\n",
                    ep.entry_type, ep.name, ep.file_path, ep.line
                ));
            }
            if entry_points.len() > 10 {
                output.push_str(&format!("  ... and {} more\n", entry_points.len() - 10));
            }
            output.push('\n');
        }

        // Module structure with stats (using tree-like format)
        output.push_str(
            "# ── Module Structure ──────────────────────────────────────────────────\n\n",
        );

        let mut sorted_modules: Vec<_> = modules.iter().collect();
        sorted_modules.sort_by_key(|(path, _)| path.as_path());

        // Calculate max widths for alignment
        let max_module_len = sorted_modules
            .iter()
            .map(|(path, _)| path.display().to_string().len())
            .max()
            .unwrap_or(0);

        for (idx, (module_path, files)) in sorted_modules.iter().enumerate() {
            let stats = count_symbols_by_module(&indexer, files);
            let is_last = idx == sorted_modules.len() - 1;
            let branch = if is_last { "└─" } else { "├─" };

            // Generate module description using knowledge base
            let description = generate_module_description_with_kb(
                module_path,
                files,
                &indexer,
                &knowledge_base,
                primary_language,
            );

            let module_str = format!("{}/", module_path.display());
            let padding = max_module_len.saturating_sub(module_str.len()) + 2;
            let stats_str = format_module_stats(&stats);

            if description.is_empty() {
                output.push_str(&format!(
                    "{} {}{:width$}{}\n",
                    branch,
                    module_str,
                    "",
                    stats_str,
                    width = padding
                ));
            } else {
                output.push_str(&format!(
                    "{} {}{:width$}{} — {}\n",
                    branch,
                    module_str,
                    "",
                    stats_str,
                    description,
                    width = padding
                ));
            }
        }
        output.push('\n');

        // Module Relationships (graph)
        let relations = build_module_graph(&modules, &indexer);

        if include_graph {
            output.push_str(
                "# ── Module Dependencies ───────────────────────────────────────────────\n\n",
            );

            if relations.is_empty() {
                output.push_str("  No inter-module relationships detected.\n\n");
            } else {
                let threshold = 20;
                let graph_output =
                    format_relationship_graph(&relations, threshold);
                output.push_str(&graph_output);
            }
        }

        // Blast Radius section
        if !relations.is_empty() {
            output.push_str(
                "# ── Blast Radius ──────────────────────────────────────────────────────\n\n",
            );
            let blast_output = format_blast_radius(&modules, &indexer);
            output.push_str(&blast_output);
        }

        // Critical Paths section
        if !entry_points.is_empty() {
            output.push_str(
                "# ── Critical Paths ────────────────────────────────────────────────────\n\n",
            );
            let paths_output = format_critical_paths(&entry_points, &indexer);
            output.push_str(&paths_output);
        }

        // Layer detection
        if !relations.is_empty() {
            output.push_str(
                "# ── Architecture Layers ───────────────────────────────────────────────\n\n",
            );
            let layers_output = format_layer_table(&modules, &relations);
            output.push_str(&layers_output);
        }

        // Add guidance (without emoji)
        if let Some(guidance) = generate_mcp_guidance(indexer.settings(), "get_project_overview", 1)
        {
            output.push_str("\n[INFO] ");
            output.push_str(&guidance);
            output.push('\n');
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

// Helper functions for get_feature_context

/// Format impact analysis with depth grouping and truncation
fn format_impact_graph(
    indexer: &IndexFacade,
    impact_ids: &[crate::SymbolId],
    max_impact: u32,
    depth: u32,
) -> String {
    let mut output = String::new();

    // Group by depth (this is simplified, real BFS tracks depth)
    let symbols: Vec<crate::Symbol> = impact_ids
        .iter()
        .take(max_impact as usize)
        .filter_map(|id| indexer.get_symbol(*id))
        .collect();

    if symbols.is_empty() {
        return output;
    }

    output.push_str(&format!("\n### Impact Analysis (depth: {})\n\n", depth));

    let total = impact_ids.len();
    let showing = symbols.len();

    if total > 100 {
        output.push_str(&format!(
            "⚠️  WARNING: High impact symbol! {} dependents detected.\n\n",
            total
        ));
    }

    output.push_str(&format!(
        "Changing this symbol will affect {} symbol(s){}:\n\n",
        total,
        if showing < total {
            format!(" (showing first {})", showing)
        } else {
            String::new()
        }
    ));

    for (i, sym) in symbols.iter().enumerate() {
        output.push_str(&format!(
            "  {}. {} ({:?}) at {}:{}\n",
            i + 1,
            sym.name,
            sym.kind,
            sym.file_path,
            sym.range.start_line + 1
        ));
    }

    if showing < total {
        output.push_str(&format!(
            "\n  ... and {} more (use max_impact:{} to see more)\n",
            total - showing,
            total.min(50)
        ));
    }

    output
}

/// Read source code snippet around a call site
fn read_call_site_snippet(file_path: &str, call_line: u32, context_lines: u32) -> Option<String> {
    let content = std::fs::read_to_string(file_path).ok()?;
    let lines: Vec<&str> = content.lines().collect();

    let line_idx = call_line.saturating_sub(1) as usize;
    if line_idx >= lines.len() {
        return None;
    }

    let ctx = context_lines as usize;
    let start = line_idx.saturating_sub(ctx);
    let end = (line_idx + ctx).min(lines.len().saturating_sub(1));

    let mut snippet = String::new();
    for i in start..=end {
        if i < lines.len() {
            let marker = if i == line_idx {
                " // ← CALL SITE"
            } else {
                ""
            };
            snippet.push_str(&format!("{:>5} | {}{}\n", i + 1, lines[i], marker));
        }
    }

    Some(snippet)
}

/// Format call site examples section
fn format_call_examples(
    callers_with_meta: &[(
        crate::Symbol,
        Option<crate::relationship::RelationshipMetadata>,
    )],
    max_examples: u32,
    context_lines: u32,
) -> String {
    let mut output = String::new();

    let examples: Vec<_> = callers_with_meta
        .iter()
        .filter_map(|(caller, meta)| {
            meta.as_ref()
                .and_then(|m| m.line)
                .map(|line| (caller, line))
        })
        .take(max_examples as usize)
        .collect();

    if examples.is_empty() {
        return output;
    }

    output.push_str("\n### Call Site Examples\n\n");

    for (i, (caller, call_line)) in examples.iter().enumerate() {
        output.push_str(&format!(
            "Example {}/{}: Called from {}\n",
            i + 1,
            examples.len(),
            caller.name
        ));
        output.push_str(&format!(
            "Location: {}:{}\n\n",
            caller.file_path,
            call_line + 1
        ));

        if let Some(snippet) = read_call_site_snippet(
            &caller.file_path,
            *call_line,
            context_lines.min(3), // Max 3 lines context for examples
        ) {
            // Detect language from file extension
            let lang = std::path::Path::new(caller.file_path.as_ref() as &str)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("text");

            output.push_str(&format!("```{}\n", lang));
            output.push_str(&snippet);
            output.push_str("```\n\n");
        }
    }

    output
}

/// Determine if a qualified call name is trivial (utility/boilerplate).
///
/// Trivial calls are:
/// - Getters/accessors (len, is_empty, value, metadata)
/// - Iterator utilities (iter, collect, map, filter)
/// - Empty constructors (new with no logic)
/// - Type conversions (as_str, to_string, into)
/// - Collection utilities (merge, push, insert without business logic)
///
/// Domain constructors (EmbeddingPool::new, Pipeline::new with logic) are NOT trivial.
fn is_trivial_call(qualified_name: &str, symbol_kind: &crate::types::SymbolKind) -> bool {
    use crate::types::SymbolKind;

    // Check for trivial patterns by name - getters/accessors
    let is_getter = qualified_name.ends_with("::len")
        || qualified_name.ends_with("::is_empty")
        || qualified_name.ends_with("::is_some")
        || qualified_name.ends_with("::is_none")
        || qualified_name.ends_with("::is_ok")
        || qualified_name.ends_with("::is_err")
        || qualified_name.ends_with("::value")
        || qualified_name.ends_with("::metadata")
        || qualified_name.ends_with("::as_str")
        || qualified_name.ends_with("::to_string")
        || qualified_name.ends_with("::into");

    // Iterator utilities
    let is_iterator_util = qualified_name.ends_with("::iter")
        || qualified_name.ends_with("::collect")
        || qualified_name.ends_with("::map")
        || qualified_name.ends_with("::filter")
        || qualified_name.ends_with("::for_each");

    // Collection utilities (common patterns)
    let is_collection_util =
        qualified_name.contains("::merge") || qualified_name.contains("::symbols_in_file"); // Codanna-specific getter

    // UI/Display utilities - progress bars, status lines, formatting
    let is_ui_display = qualified_name.starts_with("status_line::")
        || qualified_name.contains("::status_line::")
        || qualified_name.starts_with("progress::")
        || qualified_name.contains("::progress::")
        || qualified_name.ends_with("::CURSOR_PREV_LINE")
        || qualified_name.ends_with("::CURSOR_NEXT_LINE")
        || qualified_name.ends_with("::clear_line")
        || qualified_name.contains("::display::")
        || qualified_name.contains("::format_");

    // Observability - logging, metrics, timing
    let is_observability = qualified_name.starts_with("metrics::")
        || qualified_name.contains("::metrics::")
        || qualified_name.starts_with("log::")
        || qualified_name.contains("::log::")
        || qualified_name.ends_with("::elapsed")
        || qualified_name.ends_with("::duration")
        || qualified_name.ends_with("::timing")
        || qualified_name.ends_with("::measure")
        || qualified_name.contains("::tracing::")
        || qualified_name.contains("::debug::");

    // Empty constructors - harder to detect without body analysis
    // Use heuristic: stdlib/generic constructors are trivial
    let is_stdlib_constructor = matches!(
        qualified_name,
        "Vec::new"
            | "HashMap::new"
            | "HashSet::new"
            | "Box::new"
            | "Arc::new"
            | "Rc::new"
            | "String::new"
            | "Option::Some"
            | "Result::Ok"
    );

    // Method getters are usually trivial
    let is_trivial_method = matches!(symbol_kind, SymbolKind::Method)
        && (qualified_name.ends_with("::get")
            || qualified_name.ends_with("::set")
            || qualified_name.ends_with("::count"));

    is_getter
        || is_iterator_util
        || is_collection_util
        || is_stdlib_constructor
        || is_trivial_method
        || is_ui_display
        || is_observability
}

/// Group duplicate calls at the same level by symbol_id.
/// Returns Vec of (node, count) where count > 1 for duplicates.
fn group_duplicate_calls(
    nodes: &[crate::relationship::CallTreeNode],
) -> Vec<(&crate::relationship::CallTreeNode, usize)> {
    use std::collections::HashMap;

    // Count occurrences of each symbol_id
    let mut counts: HashMap<crate::types::SymbolId, usize> = HashMap::new();
    let mut first_occurrence: HashMap<crate::types::SymbolId, usize> = HashMap::new();

    for (idx, node) in nodes.iter().enumerate() {
        let count = counts.entry(node.symbol.id).or_insert(0);
        *count += 1;
        first_occurrence.entry(node.symbol.id).or_insert(idx);
    }

    // Build result: keep only first occurrence of each symbol_id
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for node in nodes.iter() {
        if seen.insert(node.symbol.id) {
            let count = *counts.get(&node.symbol.id).unwrap_or(&1);
            result.push((node, count));
        }
    }

    result
}

/// Count nodes that would actually be rendered after filtering.
/// Applies same filtering logic as format_tree_nodes_recursive.
fn count_rendered_nodes(
    nodes: &[crate::relationship::CallTreeNode],
    collapse_duplicates: bool,
    exclude_trivial: bool,
) -> usize {
    // Apply same filtering logic as format_tree_nodes_recursive
    let nodes_to_render = if collapse_duplicates {
        group_duplicate_calls(nodes)
    } else {
        nodes.iter().map(|n| (n, 1)).collect()
    };

    let mut count = 0;
    for (node, _) in nodes_to_render {
        // Skip if trivial
        if exclude_trivial {
            let qualified_name = get_qualified_name(&node.symbol);
            if is_trivial_call(&qualified_name, &node.symbol.kind) {
                continue;
            }
        }

        count += 1;
        count += count_rendered_nodes(&node.children, collapse_duplicates, exclude_trivial);
    }
    count
}

/// Generate qualified name for a symbol (Type::method or module::function format).
///
/// Priority:
/// 1. ClassMember scope → "ClassName::method"
/// 2. Module path → "module::function"
/// 3. Fallback → just "name"
fn get_qualified_name(symbol: &crate::Symbol) -> String {
    // Check scope context for class members
    if let Some(scope) = &symbol.scope_context {
        match scope {
            crate::symbol::ScopeContext::ClassMember {
                class_name: Some(class),
            } => {
                return format!("{}::{}", class, symbol.name);
            }
            crate::symbol::ScopeContext::Local {
                parent_name: Some(parent),
                ..
            } => {
                // Local functions usually don't appear in call trees,
                // but if they do, show parent context
                return format!("{}::{}", parent, symbol.name);
            }
            _ => {}
        }
    }

    // Fallback: Extract last module component
    if let Some(module) = &symbol.module_path {
        if let Some(last_component) = module.rsplit("::").next() {
            if !last_component.is_empty() && last_component != "crate" {
                return format!("{}::{}", last_component, symbol.name);
            }
        }
    }

    // Final fallback: just the name
    symbol.name.to_string()
}

/// Count consecutive cycle occurrences for deduplication.
fn count_cycle_repetitions(
    nodes: &[crate::relationship::CallTreeNode],
) -> std::collections::HashMap<crate::types::SymbolId, usize> {
    use std::collections::HashMap;

    let mut cycle_counts: HashMap<crate::types::SymbolId, usize> = HashMap::new();

    fn count_recursive(
        nodes: &[crate::relationship::CallTreeNode],
        counts: &mut std::collections::HashMap<crate::types::SymbolId, usize>,
    ) {
        for node in nodes {
            if node.is_recursive {
                *counts.entry(node.symbol.id).or_insert(0) += 1;
            }
            // Recurse to children
            count_recursive(&node.children, counts);
        }
    }

    count_recursive(nodes, &mut cycle_counts);
    cycle_counts
}

/// Format call tree as hierarchical text with tree drawing characters
fn format_call_tree(
    root: &crate::Symbol,
    nodes: &[crate::relationship::CallTreeNode],
    include_source: bool,
    include_metadata: bool,
    collapse_duplicates: bool,
    exclude_trivial: bool,
) -> String {
    let mut output = String::new();

    // Header
    output.push_str(&format!(
        "# Call Tree: {} (symbol_id:{})\n\n",
        root.name,
        root.id.value()
    ));

    if include_metadata {
        output.push_str(&format!(
            "**Location:** {}:{}\n",
            root.file_path,
            root.range.start_line + 1
        ));
        output.push_str(&format!("**Kind:** {:?}\n", root.kind));
        if let Some(ref sig) = root.signature {
            output.push_str(&format!("**Signature:** `{sig}`\n"));
        }
    }

    // Source snippet for root
    if include_source {
        if let Some(snippet) = read_source_snippet(root, 3) {
            output.push_str("\n**Source:**\n");
            output.push_str("```");
            if let Some(lang) = root.language_id {
                output.push_str(&format!("{lang:?}").to_lowercase());
            }
            output.push('\n');
            output.push_str(&snippet);
            output.push_str("```\n");
        }
    }

    output.push_str("\n## Call Tree\n\n");

    // Before recursive formatting, prepare cycle tracking
    let cycle_counts = count_cycle_repetitions(nodes);
    let mut shown_cycles = std::collections::HashSet::new();

    // Recursive tree formatting
    format_tree_nodes_recursive(
        &mut output,
        nodes,
        "",
        include_metadata,
        &cycle_counts,
        &mut shown_cycles,
        collapse_duplicates,
        exclude_trivial,
    );

    // Summary
    let total_nodes_unfiltered = count_tree_nodes(nodes);
    let total_nodes_filtered = if exclude_trivial || collapse_duplicates {
        count_rendered_nodes(nodes, collapse_duplicates, exclude_trivial)
    } else {
        total_nodes_unfiltered
    };
    let max_depth = calculate_tree_max_depth(nodes);

    // Show filtered count with unfiltered in parentheses if different
    if total_nodes_filtered < total_nodes_unfiltered {
        output.push_str(&format!(
            "\n---\n**Total:** {total_nodes_filtered} calls ({total_nodes_unfiltered} unfiltered), {max_depth} levels deep\n"
        ));
    } else {
        output.push_str(&format!(
            "\n---\n**Total:** {total_nodes_filtered} calls, {max_depth} levels deep\n"
        ));
    }

    // Warnings
    if has_cycles(nodes) {
        output.push_str("\n⚠️  **Note:** Recursive cycles detected and broken\n");
    }
    if has_truncation(nodes) {
        output.push_str("⚠️  **Note:** Tree truncated at depth/size limits\n");
    }

    output
}

/// Recursive tree node formatter with box-drawing characters
fn format_tree_nodes_recursive(
    output: &mut String,
    nodes: &[crate::relationship::CallTreeNode],
    prefix: &str,
    include_metadata: bool,
    cycle_counts: &std::collections::HashMap<crate::types::SymbolId, usize>,
    shown_cycles: &mut std::collections::HashSet<crate::types::SymbolId>,
    collapse_duplicates: bool,
    exclude_trivial: bool,
) {
    // Group duplicates if enabled
    let nodes_to_render: Vec<(&crate::relationship::CallTreeNode, usize)> = if collapse_duplicates {
        group_duplicate_calls(nodes)
    } else {
        // No collapsing - render all nodes individually
        nodes.iter().map(|n| (n, 1)).collect()
    };

    for (i, (node, duplicate_count)) in nodes_to_render.iter().enumerate() {
        // Check if this is a duplicate cycle node - skip it entirely
        if node.truncated {
            use crate::relationship::TruncationReason;
            if matches!(
                node.truncation_reason,
                Some(TruncationReason::CycleDetected)
            ) {
                if !shown_cycles.contains(&node.symbol.id) {
                    // This is a new cycle - we'll show it
                } else {
                    // Already shown this cycle - skip this entire node
                    continue;
                }
            }
        }

        // Skip trivial calls if filtering enabled
        if exclude_trivial {
            let qualified_name = get_qualified_name(&node.symbol);
            if is_trivial_call(&qualified_name, &node.symbol.kind) {
                continue; // Skip this node entirely
            }
        }

        let is_last = i == nodes_to_render.len() - 1;
        let connector = if is_last { "└─" } else { "├─" };
        let extension = if is_last { "   " } else { "│  " };

        // Node line with qualified name
        let qualified_name = get_qualified_name(&node.symbol);
        output.push_str(&format!(
            "{}{} {} (id:{})",
            prefix,
            connector,
            qualified_name,
            node.symbol.id.value()
        ));

        // Add duplicate count marker if > 1
        if *duplicate_count > 1 {
            output.push_str(&format!(" [×{duplicate_count}]"));
        }

        // Metadata (location)
        if include_metadata {
            if let Some(ref meta) = node.metadata {
                if let Some(line) = meta.line {
                    output.push_str(&format!(" @ {}:{}", node.symbol.file_path, line + 1));
                }
            }
        }

        // Truncation markers
        if node.truncated {
            use crate::relationship::TruncationReason;
            match node.truncation_reason {
                Some(TruncationReason::MaxDepthReached) => {
                    output.push_str(" [depth limit]");
                }
                Some(TruncationReason::CycleDetected) => {
                    // Mark this cycle as shown
                    shown_cycles.insert(node.symbol.id);
                    // Show count
                    let count = cycle_counts.get(&node.symbol.id).unwrap_or(&1);
                    if *count > 1 {
                        output.push_str(&format!(" [↻ cycle ×{count}]"));
                    } else {
                        output.push_str(" [↻ cycle]");
                    }
                }
                Some(TruncationReason::ExternalCall) => {
                    output.push_str(" (external)");
                }
                None => {}
            }
        }

        output.push('\n');

        // Recurse to children with extended prefix
        if !node.children.is_empty() {
            let new_prefix = format!("{prefix}{extension}");
            format_tree_nodes_recursive(
                output,
                &node.children,
                &new_prefix,
                include_metadata,
                cycle_counts,
                shown_cycles,
                collapse_duplicates,
                exclude_trivial,
            );
        }
    }
}

/// Count total nodes in tree (recursive)
fn count_tree_nodes(nodes: &[crate::relationship::CallTreeNode]) -> usize {
    let mut count = nodes.len();
    for node in nodes {
        count += count_tree_nodes(&node.children);
    }
    count
}

/// Calculate maximum depth of tree
fn calculate_tree_max_depth(nodes: &[crate::relationship::CallTreeNode]) -> usize {
    if nodes.is_empty() {
        return 0;
    }
    let mut max = 1;
    for node in nodes {
        let child_depth = calculate_tree_max_depth(&node.children);
        max = max.max(child_depth + 1);
    }
    max
}

/// Check if tree has any cycles
fn has_cycles(nodes: &[crate::relationship::CallTreeNode]) -> bool {
    nodes
        .iter()
        .any(|n| n.is_recursive || has_cycles(&n.children))
}

/// Check if tree has any truncation
fn has_truncation(nodes: &[crate::relationship::CallTreeNode]) -> bool {
    nodes
        .iter()
        .any(|n| n.truncated || has_truncation(&n.children))
}

/// Read source code snippet for a symbol
fn read_source_snippet(symbol: &crate::Symbol, context_lines: u32) -> Option<String> {
    let file_path = std::path::Path::new(symbol.file_path.as_ref() as &str);
    let content = std::fs::read_to_string(file_path).ok()?;
    let lines: Vec<&str> = content.lines().collect();

    let start = symbol.range.start_line as usize;
    let end = symbol.range.end_line as usize;

    let ctx = context_lines as usize;
    let display_start = start.saturating_sub(ctx);
    let display_end = (end + ctx).min(lines.len().saturating_sub(1));

    let mut snippet = String::new();
    for i in display_start..=display_end {
        if i < lines.len() {
            snippet.push_str(&format!("{:>5} | {}\n", i + 1, lines[i]));
        }
    }

    Some(snippet)
}

// Helper functions for get_project_overview

/// Module statistics
#[derive(Debug, Clone)]
struct ModuleStats {
    total_files: usize,
    total_symbols: usize,
    functions: usize,
    classes: usize,
    interfaces: usize,
    structs: usize,
    methods: usize,
}

impl Default for ModuleStats {
    fn default() -> Self {
        Self {
            total_files: 0,
            total_symbols: 0,
            functions: 0,
            classes: 0,
            interfaces: 0,
            structs: 0,
            methods: 0,
        }
    }
}

/// Group files by module based on parent directory structure
///
/// Example with depth=2:
/// - src/api/routes/users.rs → src/api/
/// - src/api/routes/posts.rs → src/api/
/// - src/auth/login.rs → src/auth/
///
/// When workspace_packages is non-empty, files belonging to a workspace package
/// use the package path as prefix, then apply depth to the remaining path.
///
/// Filters out single-file modules with fewer than 3 symbols to remove noise
/// from patches/, __mocks__/, and other low-value directories.
fn group_files_by_module(
    paths: &[PathBuf],
    depth: u32,
    workspace_packages: &[PathBuf],
    indexed_symbols: &HashMap<PathBuf, Vec<&crate::symbol::Symbol>>,
) -> HashMap<PathBuf, Vec<PathBuf>> {
    let mut modules: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();

    for path in paths {
        let module_path = if !workspace_packages.is_empty() {
            // Find which workspace package this file belongs to
            if let Some(pkg) = workspace_packages.iter().find(|pkg| path.starts_with(pkg)) {
                // Strip the package prefix, apply depth to the remainder
                if let Ok(remainder) = path.strip_prefix(pkg) {
                    let sub_module = get_module_path(remainder, depth as usize);
                    pkg.join(sub_module)
                } else {
                    get_module_path(path, depth as usize)
                }
            } else {
                // File doesn't belong to any workspace package — use standard depth
                get_module_path(path, depth as usize)
            }
        } else {
            get_module_path(path, depth as usize)
        };

        modules.entry(module_path).or_default().push(path.clone());
    }

    // Remove ONLY root-level config files (package.json, tsconfig.json, etc.)
    // Keep all directory-based modules even if they have only 1 indexed file
    let config_file_keys: Vec<PathBuf> = modules
        .keys()
        .filter(|k| {
            let key_str = k.to_string_lossy();
            let is_root_config = k.parent().is_none() || k.components().count() <= 1;
            let is_config_extension = key_str.ends_with(".js")
                || key_str.ends_with(".ts")
                || key_str.ends_with(".mjs")
                || key_str.ends_with(".cjs")
                || key_str.ends_with(".json");
            is_root_config && is_config_extension
        })
        .cloned()
        .collect();

    for key in config_file_keys {
        modules.remove(&key);
    }

    // Remove test/example/benchmark directories and root-level scripts
    let auxiliary_dir_keys: Vec<PathBuf> = modules
        .keys()
        .filter(|k| {
            let key_str = k.to_string_lossy().to_lowercase();
            let is_auxiliary_dir = key_str.starts_with("examples/")
                || key_str.starts_with("example/")
                || key_str.starts_with("tests/")
                || key_str.starts_with("test/")
                || key_str.starts_with("benches/")
                || key_str.starts_with("benchmarks/")
                || key_str == "examples"
                || key_str == "tests"
                || key_str == "benches"
                || key_str == "benchmarks";

            // Also filter root-level Python scripts (e.g., "quantize_jina.py/")
            let is_root_script = (k.components().count() == 1)
                && (key_str.ends_with(".py/") || key_str.ends_with(".sh/"));

            is_auxiliary_dir || is_root_script
        })
        .cloned()
        .collect();

    for key in auxiliary_dir_keys {
        modules.remove(&key);
    }

    // Remove low-value modules: 1 file + < 3 symbols
    // This filters noise from patches/, __mocks__/, empty test-setup files, etc.
    let low_value_keys: Vec<PathBuf> = modules
        .keys()
        .filter(|k| {
            if let Some(files) = modules.get(*k) {
                if files.len() == 1 {
                    // Count symbols in this single file
                    let symbol_count: usize = files
                        .iter()
                        .filter_map(|f| indexed_symbols.get(f))
                        .map(|syms| syms.len())
                        .sum();
                    return symbol_count < 3;
                }
            }
            false
        })
        .cloned()
        .collect();

    for key in low_value_keys {
        modules.remove(&key);
    }

    modules
}

/// Extract module path from file path based on depth
///
/// depth=1: src/api/routes/users.rs → src/
/// depth=2: src/api/routes/users.rs → src/api/
/// depth=3: src/api/routes/users.rs → src/api/routes/
fn get_module_path(path: &Path, depth: usize) -> PathBuf {
    let components: Vec<_> = path.components().collect();

    // Take first 'depth' components (excluding filename)
    let module_components = if components.len() > 1 {
        // -1 to exclude the file itself
        let take_count = depth.min(components.len() - 1);
        &components[..take_count]
    } else {
        &components[..]
    };

    module_components.iter().collect()
}

/// Count symbols by kind for a module
fn count_symbols_by_module(facade: &IndexFacade, module_files: &[PathBuf]) -> ModuleStats {
    let mut stats = ModuleStats::default();
    stats.total_files = module_files.len();

    for file_path in module_files {
        // Get file ID - need to convert PathBuf to &str
        if let Some(file_path_str) = file_path.to_str() {
            if let Some(file_id) = facade.get_file_id_for_path(file_path_str) {
                let symbols = facade.get_symbols_by_file(file_id);
                stats.total_symbols += symbols.len();

                for symbol in symbols {
                    match symbol.kind {
                        crate::SymbolKind::Function => stats.functions += 1,
                        crate::SymbolKind::Class => stats.classes += 1,
                        crate::SymbolKind::Interface => stats.interfaces += 1,
                        crate::SymbolKind::Struct => stats.structs += 1,
                        crate::SymbolKind::Method => stats.methods += 1,
                        _ => {}
                    }
                }
            }
        }
    }

    stats
}

/// Format module stats as a readable string
fn format_module_stats(stats: &ModuleStats) -> String {
    let mut parts = vec![format!("{} files", stats.total_files)];
    if stats.functions > 0 {
        parts.push(format!("{} functions", stats.functions));
    }
    if stats.classes > 0 {
        parts.push(format!("{} classes", stats.classes));
    }
    if stats.structs > 0 {
        parts.push(format!("{} structs", stats.structs));
    }
    if stats.interfaces > 0 {
        parts.push(format!("{} interfaces", stats.interfaces));
    }
    if stats.methods > 0 {
        parts.push(format!("{} methods", stats.methods));
    }
    format!("({})", parts.join(", "))
}

/// Entry point information
#[derive(Debug, Clone)]
struct EntryPoint {
    symbol_id: u32,
    name: String,
    file_path: String,
    line: u32,
    entry_type: String, // "main", "server", "test", etc.
    priority: u8,       // 0=manifest, 1=language convention, 2=framework, 3=auxiliary
}

/// Detect entry points in the codebase (priority-based, project-agnostic)
fn detect_entry_points(symbols: &[crate::Symbol]) -> Vec<EntryPoint> {
    let mut entry_points = Vec::new();

    // Generic auxiliary keywords — universal test/mock/script/CI patterns
    let auxiliary_keywords = [
        "test", "spec", "mock", "fixture", "example", "script", "bench",
        "eval", ".github", "ci", "integration-test", "__test",
    ];

    for symbol in symbols {
        // Only function-type symbols
        if !matches!(symbol.kind, crate::SymbolKind::Function) {
            continue;
        }

        let file_path_lower = symbol.file_path.to_lowercase();
        let symbol_name = symbol.name.as_ref();

        // Determine if auxiliary path (low priority, not skipped)
        let is_auxiliary = auxiliary_keywords
            .iter()
            .any(|kw| file_path_lower.contains(kw));

        // P1: main() function (Rust, Go, C, Java, Python, etc.)
        if symbol_name == "main" {
            entry_points.push(EntryPoint {
                symbol_id: symbol.id.value(),
                name: symbol_name.to_string(),
                file_path: symbol.file_path.to_string(),
                line: symbol.range.start_line + 1,
                entry_type: "main".to_string(),
                priority: if is_auxiliary { 3 } else { 1 },
            });
            continue;
        }

        // P1: bin/ directory executables (CLI tools)
        if file_path_lower.contains("/bin/") || file_path_lower.starts_with("bin/") {
            if matches!(symbol.visibility, crate::symbol::Visibility::Public) {
                entry_points.push(EntryPoint {
                    symbol_id: symbol.id.value(),
                    name: symbol_name.to_string(),
                    file_path: symbol.file_path.to_string(),
                    line: symbol.range.start_line + 1,
                    entry_type: "cli".to_string(),
                    priority: if is_auxiliary { 3 } else { 1 },
                });
                break; // Only first public function in bin files
            }
        }

        // Skip auxiliary paths for framework patterns (P2)
        if is_auxiliary {
            continue;
        }

        // P2: Server bootstrap (app.listen, server.start, etc.)
        if (symbol_name.contains("listen")
            || symbol_name.contains("bootstrap")
            || symbol_name == "start")
            && (file_path_lower.contains("server")
                || file_path_lower.contains("main")
                || file_path_lower.contains("index"))
        {
            entry_points.push(EntryPoint {
                symbol_id: symbol.id.value(),
                name: symbol_name.to_string(),
                file_path: symbol.file_path.to_string(),
                line: symbol.range.start_line + 1,
                entry_type: "server".to_string(),
                priority: 2,
            });
        }

        // P2: React entry points (createRoot, render, ReactDOM.render)
        if (symbol_name == "createRoot"
            || symbol_name == "render"
            || symbol_name == "hydrateRoot")
            && (file_path_lower.contains("index.tsx")
                || file_path_lower.contains("index.jsx")
                || file_path_lower.contains("index.ts")
                || file_path_lower.contains("index.js")
                || file_path_lower.contains("main.tsx")
                || file_path_lower.contains("main.jsx"))
        {
            entry_points.push(EntryPoint {
                symbol_id: symbol.id.value(),
                name: symbol_name.to_string(),
                file_path: symbol.file_path.to_string(),
                line: symbol.range.start_line + 1,
                entry_type: "react".to_string(),
                priority: 2,
            });
        }
    }

    // Sort by priority (P0/P1/P2 first, P3 auxiliary last)
    entry_points.sort_by(|a, b| a.priority.cmp(&b.priority));

    // Deduplicate by file to avoid multiple entries from same file
    let mut seen_files = std::collections::HashSet::new();
    entry_points.retain(|ep| seen_files.insert(ep.file_path.clone()));

    // Filter: only show P0-P2 entry points (hide auxiliary)
    entry_points.retain(|ep| ep.priority <= 2);

    entry_points
}

/// Detect entry points from workspace package.json bin fields and index files
fn detect_workspace_entry_points(
    workspace_root: &Path,
    workspace_packages: &[PathBuf],
    all_symbols: &[crate::Symbol],
    entry_points: &mut Vec<EntryPoint>,
) {
    let existing_files: std::collections::HashSet<String> =
        entry_points.iter().map(|ep| ep.file_path.clone()).collect();

    // Check root package.json bin field
    let mut bin_entries: Vec<(String, String)> = Vec::new();
    let root_pkg = workspace_root.join("package.json");
    if root_pkg.exists() {
        if let Ok(content) = std::fs::read_to_string(&root_pkg) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                collect_bin_entries(&json, &mut bin_entries);
            }
        }
    }

    // Check each workspace package's package.json bin field
    for ws_pkg in workspace_packages {
        let pkg_json = workspace_root.join(ws_pkg).join("package.json");
        if pkg_json.exists() {
            if let Ok(content) = std::fs::read_to_string(&pkg_json) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    collect_bin_entries(&json, &mut bin_entries);
                }
            }
        }
    }

    // For each bin entry, find the source file in the index
    for (bin_name, bin_path) in &bin_entries {
        // bin_path is like "bundle/gemini.js" or "dist/index.js"
        // Try to find matching source: replace dist/bundle with src, .js with .ts
        let source_candidates = infer_source_from_bin(bin_path);
        for candidate in &source_candidates {
            // Check if any indexed symbol lives in this file
            if let Some(symbol) = all_symbols.iter().find(|s| {
                let fp = s.file_path.as_ref();
                fp.ends_with(candidate.as_str()) && !existing_files.contains(fp)
            }) {
                entry_points.push(EntryPoint {
                    symbol_id: symbol.id.value(),
                    name: bin_name.clone(),
                    file_path: symbol.file_path.to_string(),
                    line: 1,
                    entry_type: "bin".to_string(),
                    priority: 0, // P0: manifest-driven
                });
                break;
            }
        }
    }

    // Add workspace package src/index.ts|js files as bootstrap entry points
    for ws_pkg in workspace_packages {
        let pkg_str = ws_pkg.to_string_lossy();
        for ext in &["index.ts", "index.tsx", "index.js"] {
            let index_path = format!("{}/src/{}", pkg_str, ext);
            if existing_files.contains(&index_path) {
                continue;
            }
            if let Some(symbol) = all_symbols
                .iter()
                .find(|s| s.file_path.as_ref() == index_path.as_str())
            {
                let pkg_name = ws_pkg
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("package");
                entry_points.push(EntryPoint {
                    symbol_id: symbol.id.value(),
                    name: format!("{} bootstrap", pkg_name),
                    file_path: symbol.file_path.to_string(),
                    line: 1,
                    entry_type: "bootstrap".to_string(),
                    priority: 2, // P2: framework pattern
                });
                break;
            }
        }
    }

    // Deduplicate by file
    let mut seen_files = std::collections::HashSet::new();
    entry_points.retain(|ep| seen_files.insert(ep.file_path.clone()));
}

/// Detect entry points from Cargo.toml [[bin]] targets
fn detect_cargo_bin_entry_points(
    indexer: &IndexFacade,
    all_symbols: &[crate::Symbol],
    entry_points: &mut Vec<EntryPoint>,
) {
    let existing_files: std::collections::HashSet<String> =
        entry_points.iter().map(|ep| ep.file_path.clone()).collect();

    // Find Cargo.toml in workspace or indexed root
    let cargo_path = if let Some(ref ws_root) = indexer.settings().workspace_root {
        ws_root.join("Cargo.toml")
    } else {
        PathBuf::from("Cargo.toml")
    };

    if !cargo_path.exists() {
        return;
    }

    let content = match std::fs::read_to_string(&cargo_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let toml_val: toml::Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };

    // Check [[bin]] targets
    if let Some(bins) = toml_val.get("bin").and_then(|b| b.as_array()) {
        for bin in bins {
            let name = bin.get("name").and_then(|n| n.as_str()).unwrap_or("main");
            let path = bin.get("path").and_then(|p| p.as_str());

            if let Some(bin_path) = path {
                // Find matching main() symbol in the bin source file
                if let Some(symbol) = all_symbols.iter().find(|s| {
                    let fp = s.file_path.as_ref();
                    fp.ends_with(bin_path) && s.name.as_ref() == "main" && !existing_files.contains(fp)
                }) {
                    entry_points.push(EntryPoint {
                        symbol_id: symbol.id.value(),
                        name: name.to_string(),
                        file_path: symbol.file_path.to_string(),
                        line: symbol.range.start_line + 1,
                        entry_type: "bin".to_string(),
                        priority: 0, // P0: manifest-driven
                    });
                }
            }
        }
    }
}

/// Collect bin entries from package.json
fn collect_bin_entries(json: &serde_json::Value, entries: &mut Vec<(String, String)>) {
    if let Some(bin) = json.get("bin") {
        match bin {
            serde_json::Value::String(path) => {
                // "bin": "path/to/file" — use package name
                let name = json
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("main");
                entries.push((name.to_string(), path.clone()));
            }
            serde_json::Value::Object(map) => {
                // "bin": { "gemini": "bundle/gemini.js" }
                for (name, path) in map {
                    if let Some(p) = path.as_str() {
                        entries.push((name.clone(), p.to_string()));
                    }
                }
            }
            _ => {}
        }
    }
}

/// Infer source file path from a bin/dist output path
fn infer_source_from_bin(bin_path: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    // bundle/gemini.js → src/index.ts, src/gemini.ts
    // dist/index.js → src/index.ts
    let filename = std::path::Path::new(bin_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("index");

    candidates.push(format!("src/{}.ts", filename));
    candidates.push(format!("src/{}.tsx", filename));
    candidates.push("src/index.ts".to_string());
    candidates.push("src/index.tsx".to_string());
    candidates
}

/// Package/dependency information
#[derive(Debug, Clone)]
struct DependencyInfo {
    package_name: String,
    tech_stack: String,
    dependencies: Vec<(String, String)>, // (name, version)
    dev_dependencies: Vec<(String, String)>,
    workspace_packages: Vec<String>, // workspace sub-package names
}

/// Find package configuration files
fn find_package_files(workspace_root: &Path) -> Vec<PathBuf> {
    let mut package_files = Vec::new();

    // Check for common package files
    let candidates = vec![
        "package.json",
        "Cargo.toml",
        "go.mod",
        "pyproject.toml",
        "requirements.txt",
        "pom.xml",
        "build.gradle",
        "composer.json",
    ];

    for candidate in candidates {
        let path = workspace_root.join(candidate);
        if path.exists() {
            package_files.push(path);
        }
    }

    package_files
}

/// Parse package.json file
fn parse_package_json(path: &Path) -> Option<DependencyInfo> {
    let content = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    let package_name = json.get("name")?.as_str()?.to_string();

    let mut dependencies = Vec::new();
    if let Some(deps) = json.get("dependencies").and_then(|v| v.as_object()) {
        for (name, version) in deps {
            if let Some(ver_str) = version.as_str() {
                dependencies.push((name.clone(), ver_str.to_string()));
            }
        }
    }

    let mut dev_dependencies = Vec::new();
    if let Some(dev_deps) = json.get("devDependencies").and_then(|v| v.as_object()) {
        for (name, version) in dev_deps {
            if let Some(ver_str) = version.as_str() {
                dev_dependencies.push((name.clone(), ver_str.to_string()));
            }
        }
    }

    // Detect tech stack from dependencies
    let tech_stack = if dev_dependencies.iter().any(|(n, _)| n.contains("electron")) {
        "Electron/Node.js"
    } else if dependencies
        .iter()
        .any(|(n, _)| n == "react" || n == "react-dom")
    {
        "React/JavaScript"
    } else if dependencies.iter().any(|(n, _)| n == "vue") {
        "Vue.js"
    } else if dependencies.iter().any(|(n, _)| n == "express") {
        "Express/Node.js"
    } else {
        "JavaScript/Node.js"
    };

    Some(DependencyInfo {
        package_name,
        tech_stack: tech_stack.to_string(),
        dependencies,
        dev_dependencies,
        workspace_packages: Vec::new(),
    })
}

/// Parse Cargo.toml file
fn parse_cargo_toml(path: &Path) -> Option<DependencyInfo> {
    let content = std::fs::read_to_string(path).ok()?;
    let toml: toml::Value = toml::from_str(&content).ok()?;

    let package_name = toml
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())?
        .to_string();

    let mut dependencies = Vec::new();
    if let Some(deps) = toml.get("dependencies").and_then(|v| v.as_table()) {
        for (name, version) in deps {
            let ver_str = match version {
                toml::Value::String(s) => s.clone(),
                toml::Value::Table(t) => t
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                _ => String::new(),
            };
            if !ver_str.is_empty() {
                dependencies.push((name.clone(), ver_str));
            }
        }
    }

    let mut dev_dependencies = Vec::new();
    if let Some(dev_deps) = toml.get("dev-dependencies").and_then(|v| v.as_table()) {
        for (name, version) in dev_deps {
            let ver_str = match version {
                toml::Value::String(s) => s.clone(),
                toml::Value::Table(t) => t
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                _ => String::new(),
            };
            if !ver_str.is_empty() {
                dev_dependencies.push((name.clone(), ver_str));
            }
        }
    }

    Some(DependencyInfo {
        package_name,
        tech_stack: "Rust".to_string(),
        dependencies,
        dev_dependencies,
        workspace_packages: Vec::new(),
    })
}

/// Module relationship information
#[derive(Debug, Clone)]
struct ModuleRelation {
    from_module: PathBuf,
    to_module: PathBuf,
    call_count: usize,
}

/// Layer classification for architectural analysis
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Layer {
    Interface,
    Processing,
    Data,
    Infrastructure,
}

impl Layer {
    fn as_str(&self) -> &str {
        match self {
            Layer::Interface => "Interface",
            Layer::Processing => "Processing",
            Layer::Data => "Data",
            Layer::Infrastructure => "Infrastructure",
        }
    }
}

// Layer detection thresholds for two-dimensional classification
const HIGH_ACTIVITY_THRESHOLD: usize = 50;   // Total activity indicating core module
const HIGH_FAN_IN_THRESHOLD: usize = 25;     // Significant incoming calls
const HIGH_FAN_OUT_THRESHOLD: usize = 30;    // Significant outgoing calls
const DATA_RATIO_THRESHOLD: f64 = 0.75;      // Strong bias toward being called

/// Detect architectural layer using two-dimensional fan-in/fan-out classification
///
/// This function uses quadrant-based logic to distinguish:
/// - Interface: Low fan-in, high fan-out (entry points)
/// - Processing: High fan-in AND high fan-out (core orchestrators)
/// - Data: High fan-in, low fan-out (shared utilities)
/// - Infrastructure: Test/mock/script patterns or orphan modules
fn detect_layer_enhanced(module_path: &Path, fan_in: usize, fan_out: usize) -> Layer {
    let path_str = module_path.to_string_lossy().to_lowercase();

    // Infrastructure tie-breaker: universal test/mock/script/CI patterns only
    let infra_keywords = [
        "test", "mock", "fixture", "script", ".github", "ci", "spec", "bench",
        "example", "third_party", "third-party", "vendor",
    ];
    if infra_keywords.iter().any(|kw| path_str.contains(kw)) {
        return Layer::Infrastructure;
    }

    // Core modules are always Processing (business logic)
    if path_str.contains("/core/") || path_str.starts_with("core/") {
        return Layer::Processing;
    }

    // Two-dimensional classification with absolute thresholds
    let total = fan_in + fan_out;
    if total == 0 {
        return Layer::Infrastructure; // orphan module
    }

    // Quadrant 1: High activity in BOTH dimensions = Processing
    // This fixes src/parsing (fan_in=38, fan_out=71+) and src/indexing
    if fan_in >= HIGH_FAN_IN_THRESHOLD && fan_out >= HIGH_FAN_OUT_THRESHOLD {
        return Layer::Processing;
    }

    // Quadrant 2: High fan-in, low fan-out = Data (shared utilities)
    let ratio = fan_in as f64 / total as f64;
    if ratio >= DATA_RATIO_THRESHOLD {
        return Layer::Data;
    }

    // Quadrant 3: Low fan-in, high fan-out = Interface (entry points)
    if fan_in < HIGH_FAN_IN_THRESHOLD && fan_out >= HIGH_FAN_OUT_THRESHOLD {
        return Layer::Interface;
    }

    // Quadrant 4: Low activity = fallback to ratio-based
    if ratio > 0.65 {
        Layer::Data
    } else if ratio < 0.35 {
        Layer::Interface
    } else {
        Layer::Processing
    }
}

/// Calculate fan-in and fan-out for a module from relations
fn calculate_fan_metrics(
    module_path: &Path,
    relations: &[ModuleRelation],
) -> (usize, usize) {
    let fan_out: usize = relations
        .iter()
        .filter(|r| r.from_module == module_path)
        .map(|r| r.call_count)
        .sum();

    let fan_in: usize = relations
        .iter()
        .filter(|r| r.to_module == module_path)
        .map(|r| r.call_count)
        .sum();

    (fan_in, fan_out)
}

/// Format layer table with box drawing
fn format_layer_table(
    modules: &HashMap<PathBuf, Vec<PathBuf>>,
    relations: &[ModuleRelation],
) -> String {
    let mut output = String::new();

    // Classify all modules
    let mut layers: std::collections::BTreeMap<Layer, Vec<String>> =
        std::collections::BTreeMap::new();

    for module_path in modules.keys() {
        let (fan_in, fan_out) = calculate_fan_metrics(module_path, relations);
        let layer = detect_layer_enhanced(module_path, fan_in, fan_out);
        layers
            .entry(layer)
            .or_default()
            .push(module_path.display().to_string());
    }

    // Sort module names within each layer
    for modules_in_layer in layers.values_mut() {
        modules_in_layer.sort();
    }

    let layer_order = [
        Layer::Interface,
        Layer::Processing,
        Layer::Data,
        Layer::Infrastructure,
    ];

    for layer in &layer_order {
        if let Some(mods) = layers.get(layer) {
            output.push_str(&format!("  [{}] ({} modules)\n", layer.as_str(), mods.len()));
            for m in mods {
                output.push_str(&format!("    {}\n", m));
            }
            output.push('\n');
        }
    }

    output
}

/// Format blast radius section showing high-impact modules
fn format_blast_radius(
    modules: &HashMap<PathBuf, Vec<PathBuf>>,
    facade: &IndexFacade,
) -> String {
    let mut output = String::new();
    let mut results: Vec<(PathBuf, usize, usize)> = Vec::new();

    for (module_path, files) in modules {
        let mut total_callers = 0usize;
        let mut calling_modules: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for file_path in files {
            if let Some(file_path_str) = file_path.to_str() {
                if let Some(file_id) = facade.get_file_id_for_path(file_path_str) {
                    let symbols = facade.get_symbols_by_file(file_id);
                    for symbol in &symbols {
                        let callers = facade.get_calling_functions(symbol.id);
                        total_callers += callers.len();

                        for caller in &callers {
                            // Extract module from caller file path
                            let caller_path = PathBuf::from(caller.file_path.as_ref());
                            if let Some(parent) = caller_path.parent() {
                                calling_modules.insert(parent.display().to_string());
                            }
                        }
                    }
                }
            }
        }

        if total_callers > 0 {
            results.push((module_path.clone(), total_callers, calling_modules.len()));
        }
    }

    // Sort by total callers descending
    results.sort_by(|a, b| b.1.cmp(&a.1));

    if results.is_empty() {
        output.push_str("  No reverse dependencies detected.\n\n");
        return output;
    }

    for (module, rev_deps, calling_mods) in results.iter().take(5) {
        let risk = if *rev_deps > 1000 || *calling_mods > 20 {
            "CRITICAL"
        } else if *rev_deps > 500 || *calling_mods > 10 {
            "HIGH"
        } else if *rev_deps > 100 || *calling_mods > 5 {
            "MEDIUM"
        } else {
            "LOW"
        };

        let warning = match risk {
            "CRITICAL" => "Changes here cascade across the entire codebase",
            "HIGH" => "Changes may affect many dependent modules",
            "MEDIUM" => "Changes affect several modules",
            _ => "Localized impact",
        };

        output.push_str(&format!("  {}\n", module.display()));
        output.push_str(&format!(
            "  ├─ Called by: {} modules ({} total incoming calls)\n",
            calling_mods, rev_deps
        ));
        output.push_str(&format!("  ├─ Risk: {}\n", risk));
        output.push_str(&format!("  └─ {}\n\n", warning));
    }

    output
}

/// Format critical paths section showing execution flows from entry points
/// Check if symbol should be excluded from critical path traversal
fn is_trivial_for_critical_path(symbol: &crate::Symbol) -> bool {
    let name_lower = symbol.name.to_lowercase();
    let file_lower = symbol.file_path.to_lowercase();

    // Trivial function names (logging, debugging, serialization, formatting)
    let trivial_names = [
        "log", "debug", "info", "warn", "error", "trace",
        "println", "print", "eprintln",
        "fmt", "format", "display",
        "serialize", "deserialize",
        "clone", "drop", "into", "from", "to_string",
        "unwrap", "expect",
        "console.log", "console.warn", "console.error",
    ];

    if trivial_names.iter().any(|t| name_lower.contains(t)) {
        return true;
    }

    // Logging/telemetry/tracing modules
    let trivial_modules = [
        "telemetry", "logging", "tracing", "log", "metrics",
        "observability", "monitoring", "analytics",
    ];

    if trivial_modules.iter().any(|m| file_lower.contains(m)) {
        return true;
    }

    false
}

/// Check if symbol belongs to utility module (widely-called helpers)
fn is_utility_module(symbol: &crate::Symbol) -> bool {
    let file_lower = symbol.file_path.to_lowercase();
    let utility_patterns = [
        "/utils/", "/helpers/", "/events/",
        "/common/", "/shared/", "/lib/",
    ];
    utility_patterns.iter().any(|p| file_lower.contains(p))
}

fn format_critical_paths(entry_points: &[EntryPoint], facade: &IndexFacade) -> String {
    let mut output = String::new();

    if entry_points.is_empty() {
        output.push_str("  No entry points detected.\n\n");
        return output;
    }

    for (idx, ep) in entry_points.iter().take(3).enumerate() {
        output.push_str(&format!(
            "  [Flow {}] {} ({})\n",
            idx + 1,
            ep.name,
            ep.entry_type
        ));
        output.push_str(&format!("  {}:{}\n", ep.file_path, ep.line));

        // Trace hottest path: at each step, follow the callee with highest fan-in
        let mut current_id = crate::SymbolId(ep.symbol_id);
        let mut visited = std::collections::HashSet::new();
        visited.insert(current_id);
        let mut path_nodes: Vec<(String, String, u32)> = Vec::new(); // (name, file, line)

        for _ in 0..6 {
            let callees = facade.get_called_functions(current_id);
            if callees.is_empty() {
                break;
            }

            // Find callee with highest fan-in (most important downstream symbol)
            // Exclude trivial/logging functions and utility modules from critical path
            let best = callees
                .iter()
                .filter(|c| !visited.contains(&c.id))
                .filter(|c| !is_trivial_for_critical_path(c))
                .filter(|c| !is_utility_module(c))
                .max_by_key(|c| facade.get_calling_functions(c.id).len());

            if let Some(next) = best {
                visited.insert(next.id);
                path_nodes.push((
                    next.name.to_string(),
                    next.file_path.to_string(),
                    next.range.start_line + 1,
                ));
                current_id = next.id;
            } else {
                break;
            }
        }

        // Format path
        for (i, (name, file, line)) in path_nodes.iter().enumerate() {
            let branch = if i == path_nodes.len() - 1 { "└─>" } else { "├─>" };
            output.push_str(&format!("    {} {}  {}:{}\n", branch, name, file, line));
        }

        if path_nodes.is_empty() {
            output.push_str("    └─ (no downstream calls detected)\n");
        }
        output.push('\n');
    }

    output
}



/// Build module relationship graph
fn build_module_graph(
    modules: &HashMap<PathBuf, Vec<PathBuf>>,
    facade: &IndexFacade,
) -> Vec<ModuleRelation> {
    let mut relations: HashMap<(PathBuf, PathBuf), usize> = HashMap::new();

    // For each module, get all symbols and their calls
    for (from_module, files) in modules {
        for file_path in files {
            if let Some(file_path_str) = file_path.to_str() {
                if let Some(file_id) = facade.get_file_id_for_path(file_path_str) {
                    let symbols = facade.get_symbols_by_file(file_id);

                    for symbol in symbols {
                        // Get functions this symbol calls
                        let called = facade.get_called_functions(symbol.id);

                        for called_symbol in called {
                            // Determine which module the called symbol belongs to
                            let called_file_path = PathBuf::from(called_symbol.file_path.as_ref());

                            // Find which module this file belongs to
                            for (to_module, to_files) in modules {
                                if to_files.contains(&called_file_path) {
                                    // Don't count calls within the same module
                                    if from_module != to_module {
                                        *relations
                                            .entry((from_module.clone(), to_module.clone()))
                                            .or_insert(0) += 1;
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Convert to Vec
    relations
        .into_iter()
        .map(|((from, to), count)| ModuleRelation {
            from_module: from,
            to_module: to,
            call_count: count,
        })
        .collect()
}

/// Format relationship graph with compact arrow format
fn format_relationship_graph(
    relations: &[ModuleRelation],
    threshold: usize,
) -> String {
    let mut output = String::new();

    // Filter by threshold and collect edges
    let mut filtered_relations: Vec<_> = relations
        .iter()
        .filter(|r| r.call_count >= threshold)
        .collect();

    if filtered_relations.is_empty() {
        return output;
    }

    // Sort by call count descending and take top 15
    filtered_relations.sort_by(|a, b| b.call_count.cmp(&a.call_count));
    filtered_relations.truncate(15);

    output.push_str(&format!(
        "  Internal Call Graph (threshold: {}+ calls)\n\n",
        threshold
    ));

    // Calculate max source module length for alignment
    let max_source_len = filtered_relations
        .iter()
        .map(|r| r.from_module.display().to_string().len())
        .max()
        .unwrap_or(0);

    // Format as: source_module → target_module (count×)
    for rel in &filtered_relations {
        let source = rel.from_module.display().to_string();
        let target = rel.to_module.display().to_string();
        let padding = max_source_len.saturating_sub(source.len()) + 2;

        output.push_str(&format!(
            "  {}{:width$}→ {:30} ({}×)\n",
            source,
            "",
            target,
            rel.call_count,
            width = padding
        ));
    }

    output.push('\n');
    output
}

/// Extract keywords from public symbol names
fn extract_keywords_from_symbols(symbols: &[crate::Symbol]) -> Vec<String> {
    let mut keywords = Vec::new();

    // Stop words to filter out (verbs, UI generics, common terms)
    let stop_words: std::collections::HashSet<&str> = [
        // Generic verbs
        "get",
        "set",
        "new",
        "create",
        "handle",
        "process",
        "validate",
        "generate",
        "build",
        "make",
        "init",
        "initialize",
        "setup",
        "update",
        "delete",
        "add",
        "remove",
        "is",
        "has",
        "can",
        "should",
        "will",
        "do",
        "does",
        "to",
        "from",
        "with",
        "without",
        "show",
        "hide",
        "toggle",
        "open",
        "close",
        "render",
        "mount",
        // UI/React boilerplate
        "props",
        "state",
        "ref",
        "component",
        "element",
        "node",
        "children",
        "child",
        "parent",
        "root",
        "container",
        "wrapper",
        "icon",
        "button",
        "input",
        "form",
        "field",
        "label",
        "text",
        "item",
        "list",
        "menu",
        "modal",
        "dialog",
        "popup",
        "tooltip",
        // Generic programming terms
        "data",
        "value",
        "result",
        "response",
        "request",
        "error",
        "callback",
        "handler",
        "listener",
        "event",
        "action",
        "type",
        "interface",
        "class",
        "function",
        "method",
        "property",
        "field",
    ]
    .iter()
    .copied()
    .collect();

    for symbol in symbols {
        // Only analyze public symbols
        if !matches!(symbol.visibility, crate::symbol::Visibility::Public) {
            continue;
        }

        // Split symbol name into words
        let words = crate::utils::split_identifier(symbol.name.as_ref());

        for word in words {
            let word_lower = word.to_lowercase();

            // Filter out stop words and short words
            if !stop_words.contains(word_lower.as_str()) && word_lower.len() > 2 {
                keywords.push(word_lower);
            }
        }
    }

    keywords
}


/// Analyze keyword frequency and return top keywords
fn analyze_keyword_frequency(keywords: &[String], top_n: usize) -> Vec<(String, usize)> {
    let mut frequency: HashMap<String, usize> = HashMap::new();

    for keyword in keywords {
        *frequency.entry(keyword.clone()).or_insert(0) += 1;
    }

    let mut sorted: Vec<_> = frequency.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1)); // Sort by frequency descending

    sorted.into_iter().take(top_n).collect()
}

/// Map keywords to domain descriptions (comprehensive mapping from ChatGPT spec)
fn keywords_to_domains(keywords: &[(String, usize)]) -> Vec<String> {
    let mut domains = Vec::new();

    // Domain keyword mapping (based on codanna-domains.toml spec)
    for (keyword, _) in keywords {
        let domain = match keyword.as_str() {
            // Authentication & Security
            "jwt" | "token" | "bearer" | "refresh" | "auth" | "login" | "signin" | "session" => {
                Some("Authentication")
            }
            "oauth" | "oauth2" | "openid" | "oidc" | "authorize" | "consent" => Some("OAuth2"),
            "encrypt" | "decrypt" | "hash" | "crypto" | "secret" | "key" | "cipher" => {
                Some("Security")
            }
            "password" | "credential" | "passphrase" => Some("Password Management"),
            "permission" | "role" | "access" | "rbac" | "acl" => Some("Authorization"),

            // Data & Storage
            "sql" | "query" | "migration" | "schema" | "table" | "column" | "index" | "seed"
            | "database" | "db" => Some("Database"),
            "cache" | "ttl" | "invalidate" | "redis" | "memcached" => Some("Caching"),
            "upload" | "download" | "blob" | "storage" | "bucket" | "file" | "s3" => {
                Some("File Storage")
            }

            // Communication
            "smtp" | "mail" | "email" | "mailer" | "inbox" | "newsletter" => Some("Email"),
            "socket" | "websocket" | "realtime" | "broadcast" | "emit" => Some("WebSocket"),
            "route" | "endpoint" | "middleware" | "controller" | "handler" | "request"
            | "response" | "api" => Some("HTTP/API"),

            // Business Logic
            "payment" | "invoice" | "billing" | "checkout" | "subscription" | "charge"
            | "stripe" => Some("Payments"),
            "worker" | "job" | "queue" | "background" | "cron" | "scheduler" => Some("Job Queue"),

            // Development & Testing
            "test" | "spec" | "mock" | "stub" | "assert" | "fixture" | "expect" => Some("Testing"),
            "log" | "logger" | "trace" | "metric" | "monitor" | "alert" | "span" => {
                Some("Logging/Monitoring")
            }
            "validate" | "constraint" | "rule" | "sanitize" => Some("Validation"),

            // AI/ML
            "llm" | "embedding" | "prompt" | "completion" | "model" | "inference" | "vector"
            | "openai" | "anthropic" => Some("AI/LLM"),

            // User Management
            "user" | "profile" | "account" => Some("User Management"),
            "notification" | "message" | "notify" => Some("Notifications"),

            // Infrastructure
            "deploy" | "deployment" | "infra" | "infrastructure" | "terraform" | "docker"
            | "kubernetes" => Some("Infrastructure"),
            "config" | "configuration" | "settings" | "env" | "environment" => {
                Some("Configuration")
            }

            _ => None,
        };

        if let Some(d) = domain {
            if !domains.contains(&d.to_string()) {
                domains.push(d.to_string());
            }
        }
    }

    domains
}

/// Detect external library calls (calls to symbols outside indexed files)
#[allow(dead_code)]
fn detect_external_calls(module_files: &[PathBuf], facade: &IndexFacade) -> Vec<String> {
    let mut external_calls = Vec::new();

    for file_path in module_files {
        if let Some(file_path_str) = file_path.to_str() {
            if let Some(file_id) = facade.get_file_id_for_path(file_path_str) {
                let symbols = facade.get_symbols_by_file(file_id);

                for symbol in symbols {
                    let called = facade.get_called_functions(symbol.id);

                    for called_symbol in called {
                        let called_file_path = called_symbol.file_path.as_ref();

                        // Check if called symbol is external (not in our module files)
                        let is_external = !module_files
                            .iter()
                            .any(|f| f.to_str().map_or(false, |s| s == called_file_path));

                        if is_external {
                            // Extract library/package name from symbol name or file path
                            if let Some(lib_name) = extract_library_name(&called_symbol.name) {
                                if !external_calls.contains(&lib_name) {
                                    external_calls.push(lib_name);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    external_calls
}

/// Extract library/package name from symbol name
#[allow(dead_code)]
fn extract_library_name(symbol_name: &str) -> Option<String> {
    // Common patterns for library calls
    // JavaScript: axios.get, fs.readFile, express.Router
    // Rust: std::fs::read, tokio::spawn

    if let Some(dot_pos) = symbol_name.find('.') {
        // JavaScript-style: library.method
        return Some(symbol_name[..dot_pos].to_string());
    }

    if symbol_name.contains("::") {
        // Rust-style: library::module::function
        let parts: Vec<_> = symbol_name.split("::").collect();
        if !parts.is_empty() {
            return Some(parts[0].to_string());
        }
    }

    None
}

/// Map common library calls to behavior descriptions (comprehensive package mapping)
#[allow(dead_code)]
fn map_external_call_to_behavior(lib_name: &str) -> Option<&'static str> {
    match lib_name {
        // Authentication
        "jsonwebtoken" | "jose" | "passport" | "next-auth" | "lucia" | "clerk" => {
            Some("Authentication")
        }
        "passport-google-oauth20" | "passport-github" | "openid-client" => Some("OAuth2"),
        "bcrypt" | "argon2" | "scrypt" => Some("Password hashing"),

        // Email
        "nodemailer" | "sendgrid" | "resend" | "postmark" => Some("Email delivery"),
        "@aws-sdk/client-ses" => Some("AWS SES email"),

        // Databases
        "pg" | "mysql2" | "sqlite3" | "oracledb" => Some("SQL database"),
        "prisma" | "drizzle-orm" | "knex" | "sequelize" | "typeorm" => Some("ORM"),
        "mongodb" | "mongoose" => Some("MongoDB"),
        "sqlx" | "diesel" => Some("Rust SQL"),

        // Caching
        "ioredis" | "redis" | "node-cache" | "lru-cache" => Some("Caching"),

        // Storage
        "@aws-sdk/client-s3" | "minio" => Some("S3 storage"),
        "multer" | "@google-cloud/storage" => Some("File uploads"),

        // HTTP/API Frameworks
        "express" | "hono" | "fastify" | "koa" | "@nestjs/core" => Some("HTTP server"),
        "axios" | "node-fetch" | "undici" | "got" => Some("HTTP client"),
        "axum" | "actix-web" | "rocket" | "warp" => Some("Rust HTTP server"),
        "reqwest" | "hyper" => Some("Rust HTTP client"),
        "gin" | "fiber" | "echo" => Some("Go HTTP server"),

        // WebSocket
        "socket.io" | "ws" | "@nestjs/websockets" | "sockjs" => Some("WebSocket"),

        // Job Queue
        "bull" | "bullmq" | "bee-queue" | "agenda" => Some("Job queue"),

        // Payments
        "stripe" | "@stripe/stripe-js" | "paypal-rest-sdk" | "square" => Some("Payment processing"),

        // Testing
        "jest" | "vitest" | "mocha" | "chai" | "pytest" | "rspec" => Some("Testing"),
        "cypress" | "playwright" | "@playwright/test" => Some("E2E testing"),

        // Logging/Monitoring
        "winston" | "pino" | "bunyan" => Some("Logging"),
        "sentry" | "@sentry/node" => Some("Error tracking"),
        "datadog" | "newrelic" => Some("Monitoring"),
        "tracing" | "log" => Some("Rust logging"),

        // Validation
        "zod" | "joi" | "yup" | "class-validator" | "ajv" => Some("Validation"),

        // AI/LLM
        "openai" | "@anthropic-ai/sdk" | "langchain" | "llamaindex" => Some("AI/LLM"),
        "transformers" | "@huggingface/inference" => Some("ML models"),

        // Async/Runtime
        "tokio" | "async-std" => Some("Async runtime"),

        // Serialization
        "serde" | "serde_json" | "serde_yaml" => Some("Serialization"),

        _ => None,
    }
}

/// Generate module description using waterfall strategy (legacy, replaced by KB-based version)
/// Priority: doc comments → external calls → keyword analysis
#[allow(dead_code)]
fn generate_module_description(
    module_path: &Path,
    module_files: &[PathBuf],
    facade: &IndexFacade,
) -> String {
    // Strategy 1: Check for README or doc comments in module
    // (Simplified - just check first file for now)

    // Strategy 2: External call analysis
    let external_calls = detect_external_calls(module_files, facade);
    if !external_calls.is_empty() {
        let mut behaviors = Vec::new();
        for lib in &external_calls {
            if let Some(behavior) = map_external_call_to_behavior(lib) {
                behaviors.push(behavior);
            }
        }

        if !behaviors.is_empty() {
            // Deduplicate and join
            behaviors.sort();
            behaviors.dedup();
            return format!("Uses: {}", behaviors.join(", "));
        }
    }

    // Strategy 3: Keyword extraction → domain mapping
    let mut all_symbols = Vec::new();
    for file_path in module_files {
        if let Some(file_path_str) = file_path.to_str() {
            if let Some(file_id) = facade.get_file_id_for_path(file_path_str) {
                all_symbols.extend(facade.get_symbols_by_file(file_id));
            }
        }
    }

    if !all_symbols.is_empty() {
        let keywords = extract_keywords_from_symbols(&all_symbols);
        let top_keywords = analyze_keyword_frequency(&keywords, 10);

        // Map keywords to domains
        let domains = keywords_to_domains(&top_keywords);
        if !domains.is_empty() {
            return domains.join(", ");
        }

        // Fallback: show top 3 meaningful keywords if no domain mapping
        if !top_keywords.is_empty() {
            let keyword_str: Vec<_> = top_keywords
                .iter()
                .take(3)
                .map(|(k, _)| k.as_str())
                .collect();
            return keyword_str.join(", ");
        }
    }

    // Fallback: Use module path name
    module_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("module")
        .to_string()
}

/// Infer module description from dependency info (simplified import analysis)
#[allow(dead_code)]
fn infer_description_from_dependencies(
    dep_info: &Option<DependencyInfo>,
    module_path: &Path,
) -> Option<String> {
    let info = dep_info.as_ref()?;

    let path_str = module_path.to_string_lossy().to_lowercase();

    // Map dependencies to domain descriptions
    let all_deps: Vec<_> = info
        .dependencies
        .iter()
        .chain(info.dev_dependencies.iter())
        .map(|(n, _)| n.as_str())
        .collect();

    // Check for specific patterns
    let mut domains = Vec::new();

    // Authentication
    if all_deps
        .iter()
        .any(|n| n.contains("jwt") || n.contains("passport") || n.contains("bcrypt"))
    {
        domains.push("Authentication");
    }

    // Database
    if all_deps.iter().any(|n| {
        n.contains("pg") || n.contains("mysql") || n.contains("mongodb") || n.contains("sqlite")
    }) {
        domains.push("Database");
    }

    // HTTP/API
    if all_deps
        .iter()
        .any(|n| n.contains("express") || n.contains("axios") || n.contains("fetch"))
    {
        if path_str.contains("api") || path_str.contains("route") {
            domains.push("HTTP API");
        }
    }

    // Email
    if all_deps
        .iter()
        .any(|n| n.contains("nodemailer") || n.contains("sendgrid"))
    {
        domains.push("Email");
    }

    // Testing
    if all_deps
        .iter()
        .any(|n| n.contains("jest") || n.contains("mocha") || n.contains("test"))
    {
        if path_str.contains("test") {
            domains.push("Testing");
        }
    }

    if !domains.is_empty() {
        Some(format!("Handles: {}", domains.join(", ")))
    } else {
        None
    }
}

// Second impl block - contains additional tools
// Note: #[tool_router] removed to avoid duplicate, these tools won't auto-register
// TODO: Move tools to first impl block or implement manual registration

impl CodeIntelligenceServer {
    #[tool(
        description = "Get the source code of a symbol by its ID. Returns the actual code from the file. Use after find_symbol or search_symbols to read implementation details."
    )]
    pub async fn get_source(
        &self,
        Parameters(GetSourceRequest {
            symbol_id,
            context_lines,
        }): Parameters<GetSourceRequest>,
    ) -> Result<CallToolResult, McpError> {
        let indexer = self.facade.read().await;

        let symbol = match indexer.get_symbol(crate::SymbolId(symbol_id)) {
            Some(s) => s,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Symbol not found with id: {symbol_id}"
                ))]));
            }
        };

        let file_path = std::path::Path::new(symbol.file_path.as_ref());
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read file '{}': {e}",
                    symbol.file_path
                ))]));
            }
        };

        let lines: Vec<&str> = content.lines().collect();
        let start = symbol.range.start_line as usize;
        let end = (symbol.range.end_line as usize).min(lines.len().saturating_sub(1));

        // Apply context lines
        let ctx = context_lines as usize;
        let display_start = start.saturating_sub(ctx);
        let display_end = (end + ctx).min(lines.len().saturating_sub(1));

        let mut output = format!(
            "{:?} {} at {}:{}-{} [symbol_id:{}]\n\n",
            symbol.kind,
            symbol.name,
            symbol.file_path,
            start + 1,
            end + 1,
            symbol_id
        );

        // Add language hint for code block
        let lang_hint = symbol
            .language_id
            .map(|l| format!("{l:?}").to_lowercase())
            .unwrap_or_default();
        output.push_str(&format!("```{lang_hint}\n"));

        for i in display_start..=display_end {
            if i < lines.len() {
                output.push_str(&format!("{:>4} | {}\n", i + 1, lines[i]));
            }
        }

        output.push_str("```\n");

        // Add guidance
        if let Some(guidance) = generate_mcp_guidance(indexer.settings(), "get_source", 1) {
            output.push_str("\n---\n💡 ");
            output.push_str(&guidance);
            output.push('\n');
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "List all exported (public) symbols from a file, grouped by kind (functions, types, constants, etc.)."
    )]
    pub async fn get_module_exports(
        &self,
        Parameters(GetModuleExportsRequest { file_path }): Parameters<GetModuleExportsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let indexer = self.facade.read().await;

        // Find file_id using fuzzy ends_with matching
        let all_paths = indexer.get_all_indexed_paths();
        let matched_path = all_paths.iter().find(|p| {
            let p_str = p.to_string_lossy();
            p_str.ends_with(&file_path) || p_str == file_path
        });

        let matched_path = match matched_path {
            Some(p) => p.to_string_lossy().to_string(),
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "File not found in index: {file_path}\nHint: Use a relative path like 'src/hooks/useExport.ts'"
                ))]));
            }
        };

        let file_id = match indexer.get_file_id_for_path(&matched_path) {
            Some(id) => id,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Could not resolve file ID for: {matched_path}"
                ))]));
            }
        };

        let symbols = indexer.get_symbols_by_file(file_id);
        let exports: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.visibility == crate::Visibility::Public)
            .collect();

        if exports.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No public exports found in: {matched_path}"
            ))]));
        }

        // Group by SymbolKind
        use std::collections::BTreeMap;
        let mut groups: BTreeMap<&str, Vec<&Symbol>> = BTreeMap::new();
        for sym in &exports {
            let group = match sym.kind {
                crate::SymbolKind::Function => "Functions",
                crate::SymbolKind::Method => "Methods",
                crate::SymbolKind::Struct => "Structs",
                crate::SymbolKind::Interface => "Interfaces",
                crate::SymbolKind::Class => "Classes",
                crate::SymbolKind::Enum => "Enums",
                crate::SymbolKind::TypeAlias => "Types",
                crate::SymbolKind::Constant => "Constants",
                crate::SymbolKind::Variable => "Variables",
                crate::SymbolKind::Trait => "Traits",
                crate::SymbolKind::Module => "Modules",
                crate::SymbolKind::Macro => "Macros",
                _ => "Other",
            };
            groups.entry(group).or_default().push(sym);
        }

        let mut output = format!(
            "Exports from {} ({} public symbol(s)):\n\n",
            matched_path,
            exports.len()
        );

        for (group_name, syms) in &groups {
            output.push_str(&format!("### {} ({})\n", group_name, syms.len()));
            for sym in syms {
                let sig = sym
                    .signature
                    .as_ref()
                    .map(|s| format!(" — {s}"))
                    .unwrap_or_default();
                output.push_str(&format!(
                    "  - {} [symbol_id:{}] L{}{}\n",
                    sym.name,
                    sym.id.value(),
                    sym.range.start_line + 1,
                    sig
                ));
            }
            output.push('\n');
        }

        // Add guidance
        if let Some(guidance) =
            generate_mcp_guidance(indexer.settings(), "get_module_exports", exports.len())
        {
            output.push_str("---\n💡 ");
            output.push_str(&guidance);
            output.push('\n');
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "Get fields and methods of a struct, interface, class, trait, or type alias. Returns all members grouped by kind (fields vs methods)."
    )]
    pub async fn get_type_fields(
        &self,
        Parameters(GetTypeFieldsRequest { symbol_id }): Parameters<GetTypeFieldsRequest>,
    ) -> Result<CallToolResult, McpError> {
        use crate::symbol::context::ContextIncludes;

        let indexer = self.facade.read().await;

        let symbol = match indexer.get_symbol(crate::SymbolId(symbol_id)) {
            Some(s) => s,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Symbol not found with id: {symbol_id}"
                ))]));
            }
        };

        // Validate kind
        if !matches!(
            symbol.kind,
            crate::SymbolKind::Struct
                | crate::SymbolKind::Interface
                | crate::SymbolKind::Class
                | crate::SymbolKind::Trait
                | crate::SymbolKind::TypeAlias
                | crate::SymbolKind::Enum
        ) {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Symbol '{}' is a {:?}, not a type with fields. Expected Struct, Interface, Class, Trait, TypeAlias, or Enum.",
                symbol.name, symbol.kind
            ))]));
        }

        let context = indexer.get_symbol_context(symbol.id, ContextIncludes::DEFINITIONS);

        let defines = context
            .as_ref()
            .and_then(|ctx| ctx.relationships.defines.as_ref());

        let mut fields: Vec<&Symbol> = Vec::new();
        let mut methods: Vec<&Symbol> = Vec::new();

        if let Some(defs) = defines {
            for def in defs {
                match def.kind {
                    crate::SymbolKind::Field
                    | crate::SymbolKind::Variable
                    | crate::SymbolKind::Constant => {
                        fields.push(def);
                    }
                    crate::SymbolKind::Method | crate::SymbolKind::Function => {
                        methods.push(def);
                    }
                    _ => {
                        // Other kinds (nested types, etc.) go to fields for visibility
                        fields.push(def);
                    }
                }
            }
        }

        let total = fields.len() + methods.len();
        if total == 0 {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "{:?} {} [symbol_id:{}] has no indexed fields or methods.\nHint: Re-index after parser updates with 'codanna index <path>'",
                symbol.kind, symbol.name, symbol_id
            ))]));
        }

        let file_path = &symbol.file_path;
        let mut output = format!(
            "{:?} {} at {}:{} [symbol_id:{}]\n\n",
            symbol.kind,
            symbol.name,
            file_path,
            symbol.range.start_line + 1,
            symbol_id
        );

        if !fields.is_empty() {
            output.push_str(&format!("### Fields ({})\n", fields.len()));
            for f in &fields {
                let sig = f
                    .signature
                    .as_ref()
                    .map(|s| format!(" — {s}"))
                    .unwrap_or_default();
                output.push_str(&format!(
                    "  - {} ({:?}) [symbol_id:{}] L{}{}\n",
                    f.name,
                    f.kind,
                    f.id.value(),
                    f.range.start_line + 1,
                    sig
                ));
            }
            output.push('\n');
        }

        if !methods.is_empty() {
            output.push_str(&format!("### Methods ({})\n", methods.len()));
            for m in &methods {
                let sig = m
                    .signature
                    .as_ref()
                    .map(|s| format!(" — {s}"))
                    .unwrap_or_default();
                output.push_str(&format!(
                    "  - {} [symbol_id:{}] L{}{}\n",
                    m.name,
                    m.id.value(),
                    m.range.start_line + 1,
                    sig
                ));
            }
            output.push('\n');
        }

        // Add guidance
        if let Some(guidance) = generate_mcp_guidance(indexer.settings(), "get_type_fields", total)
        {
            output.push_str("---\n💡 ");
            output.push_str(&guidance);
            output.push('\n');
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "Analyze React hooks in a component/hook function. Extracts useState, useRef, useEffect, useCallback, and useMemo with their dependencies using AST parsing."
    )]
    pub async fn get_state_graph(
        &self,
        Parameters(GetStateGraphRequest { symbol_id }): Parameters<GetStateGraphRequest>,
    ) -> Result<CallToolResult, McpError> {
        let indexer = self.facade.read().await;

        let symbol = match indexer.get_symbol(crate::SymbolId(symbol_id)) {
            Some(s) => s,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Symbol not found with id: {symbol_id}"
                ))]));
            }
        };

        // Read source file
        let file_path = std::path::Path::new(symbol.file_path.as_ref());
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read file '{}': {e}",
                    symbol.file_path
                ))]));
            }
        };

        // Extract function body lines
        let lines: Vec<&str> = content.lines().collect();
        let start = symbol.range.start_line as usize;
        let end = (symbol.range.end_line as usize).min(lines.len().saturating_sub(1));
        let source_slice: String = lines[start..=end].join("\n");

        // Parse with tree-sitter
        let graph = react_hooks::extract_react_hooks(&source_slice);

        if graph.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "{:?} {} [symbol_id:{}] — No React hooks found.",
                symbol.kind, symbol.name, symbol_id
            ))]));
        }

        let mut output = format!(
            "React Hook State Graph for {:?} {} [symbol_id:{}]\n\n",
            symbol.kind, symbol.name, symbol_id
        );

        output.push_str(&graph.format());

        // Add guidance
        if let Some(guidance) = generate_mcp_guidance(indexer.settings(), "get_state_graph", 1) {
            output.push_str("\n---\n💡 ");
            output.push_str(&guidance);
            output.push('\n');
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // get_feature_context moved to first impl block (with #[tool_router])

    #[tool(
        description = "Search indexed documents (markdown, text files) using natural language queries. Returns relevant chunks with context and highlighted keywords."
    )]
    pub async fn search_documents(
        &self,
        Parameters(SearchDocumentsRequest {
            query,
            collection,
            limit,
        }): Parameters<SearchDocumentsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let store = match &self.document_store {
            Some(s) => s,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "Document search not available. No document collections are indexed.\n\n\
                    To enable:\n\
                    1. Add a collection: codanna documents add-collection docs docs/\n\
                    2. Index it: codanna documents index\n\
                    3. Restart the MCP server",
                )]));
            }
        };

        let mut store = store.write().await;
        let indexer = self.facade.read().await;

        // Auto-sync: check for file changes in all collections before searching
        let settings = indexer.settings();
        for (name, config) in &settings.documents.collections {
            if let Err(e) = store.index_collection(name, config, &settings.documents.defaults) {
                tracing::warn!(target: "rag", "auto-sync failed for collection '{}': {}", name, e);
            }
        }

        let search_query = DocSearchQuery {
            text: query.clone(),
            collection,
            document: None,
            limit: limit as usize,
            preview_config: Some(indexer.settings().documents.search.clone()),
        };

        match store.search(search_query) {
            Ok(results) => {
                if results.is_empty() {
                    return Ok(CallToolResult::success(vec![Content::text(format!(
                        "No documents found for: {query}"
                    ))]));
                }

                let mut output = format!(
                    "Found {} document(s) matching '{}':\n\n",
                    results.len(),
                    query
                );

                for (i, result) in results.iter().enumerate() {
                    output.push_str(&format!(
                        "{}. {} (score: {:.3})\n",
                        i + 1,
                        result.source_path.display(),
                        result.similarity
                    ));

                    if !result.heading_context.is_empty() {
                        output.push_str(&format!(
                            "   Context: {}\n",
                            result.heading_context.join(" > ")
                        ));
                    }

                    // Preview is already KWIC-processed with highlighting
                    output.push_str(&format!("   Preview: {}\n\n", result.content_preview));
                }

                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Document search failed: {e}"
            ))])),
        }
    }
}

#[tool_handler]
impl ServerHandler for CodeIntelligenceServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "codanna".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("Codanna Code Intelligence".to_string()),
                website_url: Some("https://github.com/bartolli/codanna".to_string()),
                icons: None,
            },
            instructions: Some(
                "This server provides code intelligence tools for analyzing this codebase. \
                WORKFLOW: Start with 'semantic_search_with_context', 'semantic_search_docs', or 'semantic_search_chunks' to anchor on the right files and APIs. \
                Then use 'find_symbol' and 'search_symbols' to lock onto exact files and kinds. \
                Treat 'get_calls', 'find_callers', and 'analyze_impact' as hints; confirm with code reading or tighter queries (unique names, kind filters). \
                Use 'search_documents' to find relevant project documentation (markdown files). \
                Use 'get_index_info' to understand what's indexed."
                .to_string()
            ),
        }
    }

    async fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        // Register client capabilities (required for MCP handshake)
        if context.peer.peer_info().is_none() {
            context.peer.set_peer_info(request);
        }

        // Store the peer reference for sending notifications
        let mut peer_guard = self.peer.lock().await;
        *peer_guard = Some(context.peer.clone());

        // Return the server info
        Ok(self.get_info())
    }

    async fn on_custom_request(
        &self,
        request: CustomRequest,
        _context: RequestContext<RoleServer>,
    ) -> Result<CustomResult, McpError> {
        match request.method.as_str() {
            "requests/codanna/force-reindex" => self.handle_force_reindex(request).await,
            "requests/codanna/index-stats" => self.handle_index_stats().await,
            _ => Err(McpError::new(
                ErrorCode::METHOD_NOT_FOUND,
                format!("Unknown method: {}", request.method),
                None,
            )),
        }
    }
}

// Custom request handlers
impl CodeIntelligenceServer {
    /// Handle force-reindex request
    async fn handle_force_reindex(&self, request: CustomRequest) -> Result<CustomResult, McpError> {
        use std::time::Instant;

        let start = Instant::now();

        // Parse optional paths parameter
        let paths: Option<Vec<String>> = request
            .params
            .as_ref()
            .and_then(|p| p.get("paths"))
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        let mut indexer = self.facade.write().await;

        let (reindexed, symbols) = if let Some(paths) = paths {
            // Reindex specific paths
            let mut total_reindexed = 0;
            for path in &paths {
                let path = std::path::Path::new(path);
                if path.is_file() {
                    match indexer.index_file(path) {
                        Ok(crate::IndexingResult::Indexed(_)) => total_reindexed += 1,
                        Ok(crate::IndexingResult::Cached(_)) => {}
                        Err(e) => {
                            tracing::warn!("Failed to reindex {}: {e}", path.display());
                        }
                    }
                } else if path.is_dir() {
                    match indexer.index_directory(path, false) {
                        Ok(stats) => total_reindexed += stats.files_indexed,
                        Err(e) => {
                            tracing::warn!("Failed to reindex {}: {e}", path.display());
                        }
                    }
                }
            }
            (total_reindexed, indexer.symbol_count())
        } else {
            // Full reindex using indexed_paths from settings
            let indexed_paths = indexer.settings().indexing.indexed_paths.clone();
            let mut total_reindexed = 0;

            for path in &indexed_paths {
                if path.is_dir() {
                    match indexer.index_directory(path, false) {
                        Ok(stats) => total_reindexed += stats.files_indexed,
                        Err(e) => {
                            tracing::warn!("Failed to reindex {}: {e}", path.display());
                        }
                    }
                }
            }
            (total_reindexed, indexer.symbol_count())
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(CustomResult(serde_json::json!({
            "reindexed": reindexed,
            "symbols": symbols,
            "duration_ms": duration_ms
        })))
    }

    /// Handle index-stats request
    async fn handle_index_stats(&self) -> Result<CustomResult, McpError> {
        let indexer = self.facade.read().await;

        let semantic = if let Some(metadata) = indexer.get_semantic_metadata() {
            serde_json::json!({
                "enabled": true,
                "model": metadata.model_name,
                "embeddings": metadata.embedding_count,
                "dimensions": metadata.dimension
            })
        } else {
            serde_json::json!({
                "enabled": false
            })
        };

        Ok(CustomResult(serde_json::json!({
            "symbols": indexer.symbol_count(),
            "files": indexer.file_count(),
            "relationships": indexer.relationship_count(),
            "semantic": semantic
        })))
    }

    /// Send a custom notification to the connected client
    pub async fn notify_custom(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), ServiceError> {
        let peer_guard = self.peer.lock().await;
        if let Some(peer) = peer_guard.as_ref() {
            peer.send_notification(ServerNotification::CustomNotification(
                CustomNotification::new(method, Some(params)),
            ))
            .await?;
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════
// Enhanced Project Overview Helpers
// ═══════════════════════════════════════════════════════════════

/// Detect primary language from file extensions
fn detect_primary_language(file_paths: &[PathBuf]) -> HashMap<String, usize> {
    let mut lang_counts: HashMap<String, usize> = HashMap::new();

    for path in file_paths {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let language = match ext {
                "rs" => "Rust",
                "ts" | "tsx" => "TypeScript",
                "js" | "jsx" => "JavaScript",
                "py" => "Python",
                "go" => "Go",
                "java" => "Java",
                "cpp" | "cc" | "cxx" => "C++",
                "c" | "h" => "C",
                "php" => "PHP",
                "rb" => "Ruby",
                "swift" => "Swift",
                "kt" | "kts" => "Kotlin",
                "cs" => "C#",
                _ => continue,
            };
            *lang_counts.entry(language.to_string()).or_insert(0) += 1;
        }
    }

    lang_counts
}

/// Format language distribution as percentage string
fn format_language_distribution(lang_counts: &HashMap<String, usize>) -> String {
    let total: usize = lang_counts.values().sum();
    if total == 0 {
        return "Unknown".to_string();
    }

    let mut langs: Vec<_> = lang_counts.iter().collect();
    langs.sort_by(|a, b| b.1.cmp(a.1)); // Sort by count descending

    langs
        .iter()
        .take(3)
        .map(|(lang, count)| {
            let pct = (**count as f64 / total as f64 * 100.0).round() as u32;
            format!("{} ({}%)", lang, pct)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Detect project architecture type
fn detect_architecture(workspace_root: &Path) -> &'static str {
    // Check for monorepo indicators
    if workspace_root.join("package.json").exists() {
        if let Ok(content) = std::fs::read_to_string(workspace_root.join("package.json")) {
            if content.contains("\"workspaces\"") {
                return "Monorepo (npm workspaces)";
            }
        }
    }

    if workspace_root.join("lerna.json").exists() {
        return "Monorepo (Lerna)";
    }

    if workspace_root.join("Cargo.toml").exists() {
        if let Ok(content) = std::fs::read_to_string(workspace_root.join("Cargo.toml")) {
            if content.contains("[workspace]") {
                return "Workspace (Cargo)";
            }
        }
    }

    "Single project"
}

/// Detect workspace packages in monorepo setups
///
/// Supports npm workspaces (package.json), Cargo workspaces (Cargo.toml), and Lerna (lerna.json).
/// Returns list of workspace package directories relative to workspace_root.
fn detect_workspace_packages(workspace_root: &Path) -> Vec<PathBuf> {
    let mut packages = Vec::new();

    // npm workspaces: package.json → "workspaces" array
    let pkg_json_path = workspace_root.join("package.json");
    if pkg_json_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&pkg_json_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(workspaces) = json.get("workspaces").and_then(|v| v.as_array()) {
                    for ws in workspaces {
                        if let Some(pattern) = ws.as_str() {
                            expand_workspace_glob(workspace_root, pattern, &mut packages);
                        }
                    }
                }
            }
        }
    }

    // Cargo workspace: Cargo.toml → [workspace] members
    if packages.is_empty() {
        let cargo_path = workspace_root.join("Cargo.toml");
        if cargo_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_path) {
                if let Ok(toml_val) = toml::from_str::<toml::Value>(&content) {
                    if let Some(members) = toml_val
                        .get("workspace")
                        .and_then(|w| w.get("members"))
                        .and_then(|m| m.as_array())
                    {
                        for member in members {
                            if let Some(pattern) = member.as_str() {
                                expand_workspace_glob(workspace_root, pattern, &mut packages);
                            }
                        }
                    }
                }
            }
        }
    }

    // Lerna: lerna.json → "packages" array
    if packages.is_empty() {
        let lerna_path = workspace_root.join("lerna.json");
        if lerna_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&lerna_path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(pkgs) = json.get("packages").and_then(|v| v.as_array()) {
                        for pkg in pkgs {
                            if let Some(pattern) = pkg.as_str() {
                                expand_workspace_glob(workspace_root, pattern, &mut packages);
                            }
                        }
                    }
                }
            }
        }
    }

    packages
}

/// Expand a workspace glob pattern like "packages/*" into actual directories
fn expand_workspace_glob(workspace_root: &Path, pattern: &str, results: &mut Vec<PathBuf>) {
    if let Some(prefix) = pattern.strip_suffix("/*") {
        // "packages/*" → list directories under workspace_root/packages/
        let dir = workspace_root.join(prefix);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    // Store as relative path from workspace_root (no ./ prefix)
                    if let Ok(rel) = entry.path().strip_prefix(workspace_root) {
                        results.push(rel.to_path_buf());
                    }
                }
            }
        }
    } else {
        // Direct path like "packages/core" — just check if it exists
        let dir = workspace_root.join(pattern);
        if dir.is_dir() {
            results.push(PathBuf::from(pattern));
        }
    }
}

/// Detect project type from dependencies and structure
fn detect_project_type(dep_info: &Option<DependencyInfo>) -> &'static str {
    if let Some(info) = dep_info {
        // Check for specific tech patterns
        if info
            .dev_dependencies
            .iter()
            .any(|(n, _)| n.contains("electron"))
        {
            return "Desktop App (Electron)";
        }

        if info
            .dependencies
            .iter()
            .any(|(n, _)| n == "react" || n == "react-dom")
        {
            if info.dependencies.iter().any(|(n, _)| n == "next") {
                return "Web App (Next.js)";
            }
            return "Web App (React)";
        }

        if info.dependencies.iter().any(|(n, _)| n == "vue") {
            return "Web App (Vue)";
        }

        if info
            .dependencies
            .iter()
            .any(|(n, _)| n == "express" || n == "fastify" || n == "koa" || n.contains("@nestjs"))
        {
            return "Backend Service";
        }

        if info.tech_stack.contains("Rust") {
            return "System Tool/Library";
        }
    }

    "Application"
}

/// Load knowledge base from .codanna/knowledge directory
fn load_knowledge_base(workspace_root: &Path) -> Option<knowledge::KnowledgeBase> {
    let knowledge_path = workspace_root.join(".codanna").join("knowledge");
    knowledge::KnowledgeBase::load_from_dir(&knowledge_path).ok()
}

/// Generate enhanced module description using knowledge base
fn generate_module_description_with_kb(
    module_path: &Path,
    module_files: &[PathBuf],
    facade: &IndexFacade,
    kb: &Option<knowledge::KnowledgeBase>,
    primary_language: &str,
) -> String {
    // Strategy 1: Keyword extraction → domain mapping via knowledge base
    let mut all_symbols = Vec::new();
    for file_path in module_files {
        if let Some(file_path_str) = file_path.to_str() {
            if let Some(file_id) = facade.get_file_id_for_path(file_path_str) {
                all_symbols.extend(facade.get_symbols_by_file(file_id));
            }
        }
    }

    if !all_symbols.is_empty() {
        let keywords = extract_keywords_from_symbols(&all_symbols);

        // Filter stop words using knowledge base if available
        let filtered_keywords = if let Some(knowledge_base) = kb {
            let stop_words = knowledge_base.get_stop_words(primary_language);
            let stop_set: std::collections::HashSet<_> =
                stop_words.iter().map(|s| s.to_lowercase()).collect();
            keywords
                .into_iter()
                .filter(|k| !stop_set.contains(&k.to_lowercase()))
                .collect()
        } else {
            keywords
        };

        let top_keywords = analyze_keyword_frequency(&filtered_keywords, 10);

        // Map keywords to domains using knowledge base (returns behavior text)
        if let Some(knowledge_base) = kb {
            let mut keyword_strs: Vec<String> =
                top_keywords.iter().map(|(k, _)| k.clone()).collect();

            // Add module name as keyword hint (skip generic root names)
            let module_name = module_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_lowercase())
                .filter(|s| !matches!(s.as_str(), "src" | "lib" | "app" | "bin"));
            if let Some(ref name) = module_name {
                if !keyword_strs.contains(name) {
                    keyword_strs.insert(0, name.clone());
                }
            }

            let mut domain_matches =
                knowledge_base.match_keywords_to_domains(primary_language, &keyword_strs);

            // Boost domains whose keywords match the module name
            if let Some(ref name) = module_name {
                for entry in &mut domain_matches {
                    // entry = (domain_name, behavior, priority, match_count)
                    // Check if any domain keyword matches the module name
                    if let Some(eco) = knowledge_base.get_for_language(primary_language) {
                        for domain in &eco.domain {
                            if domain.name == entry.0
                                && domain.keywords.iter().any(|dk| {
                                    name.contains(&dk.to_lowercase())
                                        || dk.to_lowercase().contains(name.as_str())
                                })
                            {
                                entry.3 += 5; // Strong boost for module name match
                            }
                        }
                    }
                }
                // Re-sort after boosting
                domain_matches.sort_by(|a, b| b.3.cmp(&a.3).then(b.2.cmp(&a.2)));
            }

            if !domain_matches.is_empty() {
                // Use behavior text from the best match
                let best_behavior = &domain_matches[0].1;
                let mut desc = best_behavior.clone();
                // Truncate to 60 chars
                if desc.len() > 60 {
                    desc.truncate(57);
                    desc.push_str("...");
                }
                return desc;
            }
        } else {
            // Fallback to hardcoded mapping if KB not available
            let domains = keywords_to_domains(&top_keywords);
            if !domains.is_empty() {
                return domains.join(", ");
            }
        }

        // No domain match: fall through to symbol name fallback
    }

    // Fallback: show top 3 most-referenced exported symbols (fan-in sorted)
    let kind_priority = |s: &crate::Symbol| -> u8 {
        match s.kind {
            crate::SymbolKind::Class => 0,
            crate::SymbolKind::Interface => 1,
            crate::SymbolKind::Struct => 1,
            crate::SymbolKind::Enum => 2,
            crate::SymbolKind::Function => 3,
            _ => 4,
        }
    };

    // Take top 20 candidates by kind priority first (avoid fan-in lookup for all)
    let mut candidates: Vec<&crate::Symbol> = all_symbols
        .iter()
        .filter(|s| matches!(s.visibility, crate::symbol::Visibility::Public))
        .filter(|s| matches!(s.kind,
            crate::SymbolKind::Class | crate::SymbolKind::Interface |
            crate::SymbolKind::Struct | crate::SymbolKind::Enum |
            crate::SymbolKind::Function
        ))
        .collect();
    candidates.sort_by(|a, b| kind_priority(a).cmp(&kind_priority(b)).then(a.name.cmp(&b.name)));
    candidates.dedup_by(|a, b| a.name == b.name);
    candidates.truncate(20);

    // Compute fan-in only for the 20 candidates
    let mut with_fan_in: Vec<(&str, u8, usize)> = candidates
        .iter()
        .map(|s| {
            let fan_in = facade.get_calling_functions(s.id).len();
            (s.name.as_ref(), kind_priority(s), fan_in)
        })
        .collect();

    // Sort: kind priority, then fan-in desc, then alphabetical
    with_fan_in.sort_by(|a, b| a.1.cmp(&b.1).then(b.2.cmp(&a.2)).then(a.0.cmp(b.0)));
    let top_names: Vec<&str> = with_fan_in.iter().take(3).map(|(n, _, _)| *n).collect();
    if !top_names.is_empty() {
        return top_names.join(", ");
    }

    String::new()
}

/// Format tech stack section using knowledge base categorization
fn format_tech_stack(
    dep_info: &Option<DependencyInfo>,
    kb: &Option<knowledge::KnowledgeBase>,
    primary_language: &str,
) -> String {
    let mut output = String::new();

    if let Some(info) = dep_info {
        // Extract all package names
        let all_packages: Vec<String> = info
            .dependencies
            .iter()
            .chain(info.dev_dependencies.iter())
            .map(|(name, _)| name.clone())
            .collect();

        // Categorize using knowledge base
        // Try primary language first, then detect from package files
        let categorized = if let Some(knowledge_base) = kb {
            let mut cat_map: HashMap<String, (Vec<String>, bool)> = HashMap::new();

            // Determine which language KBs to try
            let lang_to_try = if knowledge_base.get_for_language(primary_language).is_some() {
                primary_language.to_string()
            } else {
                // Detect from dep_info tech_stack or package type
                if info.tech_stack.contains("React")
                    || info.tech_stack.contains("JavaScript")
                    || info.tech_stack.contains("Vue")
                    || info.tech_stack.contains("Node")
                    || info.tech_stack.contains("Express")
                    || info.tech_stack.contains("Electron")
                {
                    "typescript".to_string()
                } else if info.tech_stack.contains("Rust") {
                    "rust".to_string()
                } else {
                    primary_language.to_string()
                }
            };

            if let Some(eco) = knowledge_base.get_for_language(&lang_to_try) {
                for stack in &eco.stack {
                    let matched: Vec<String> = all_packages
                        .iter()
                        .filter(|pkg| {
                            stack
                                .packages
                                .iter()
                                .any(|sp| pkg.to_lowercase().contains(&sp.to_lowercase()))
                        })
                        .cloned()
                        .collect();

                    if !matched.is_empty() {
                        cat_map.insert(
                            stack.category.clone(),
                            (matched, stack.is_utility.unwrap_or(false)),
                        );
                    }
                }
            }
            cat_map
        } else {
            HashMap::new()
        };

        if !categorized.is_empty() {
            let category_order = [
                "runtime", "frontend", "backend", "ai_ml", "protocol",
                "cli_tools", "parsing", "search", "database",
                "services", "testing", "build", "infrastructure",
            ];

            for category in category_order {
                if let Some((packages, is_utility)) = categorized.get(category) {
                    // Skip utility categories
                    if *is_utility || packages.is_empty() {
                        continue;
                    }

                    let category_name = capitalize_category(category);

                    // Inline format: Category: pkg1, pkg2, pkg3
                    let pkg_list: Vec<String> = packages
                        .iter()
                        .take(8)
                        .map(|pkg| {
                            let version = info
                                .dependencies
                                .iter()
                                .chain(info.dev_dependencies.iter())
                                .find(|(name, _)| name == pkg)
                                .map(|(_, ver)| ver.as_str())
                                .unwrap_or("");
                            if version.is_empty() {
                                pkg.clone()
                            } else {
                                format!("{} {}", pkg, version)
                            }
                        })
                        .collect();

                    let suffix = if packages.len() > 8 {
                        format!(" (+{} more)", packages.len() - 8)
                    } else {
                        String::new()
                    };

                    output.push_str(&format!(
                        "  {:<14} {}{}\n",
                        format!("{}:", category_name),
                        pkg_list.join(", "),
                        suffix
                    ));
                }
            }

            if !output.is_empty() {
                output.push('\n');
            }
        }
    }

    output
}

/// Capitalize a category name (e.g. "runtime" → "Runtime")
fn capitalize_category(category: &str) -> String {
    category
        .split('_')
        .map(|word| {
            let mut c = word.chars();
            match c.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
