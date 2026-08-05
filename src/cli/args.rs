//! CLI argument parsing using clap.
//!
//! Contains the Cli struct, Commands enum, and all subcommand enums.

use clap::{
    Parser, Subcommand,
    builder::styling::{AnsiColor, Effects, Styles},
};
use std::path::PathBuf;

fn clap_cargo_style() -> Styles {
    Styles::styled()
        .header(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .usage(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .literal(AnsiColor::Green.on_default())
        .placeholder(AnsiColor::Green.on_default())
}

/// Create custom help text with consistent styling
fn create_custom_help() -> String {
    use crate::display::theme::Theme;
    use console::style;

    let mut help = String::new();

    // Quick Start section
    if Theme::should_disable_colors() {
        help.push_str("Quick Start:\n");
    } else {
        help.push_str(&format!("{}\n", style("Quick Start:").cyan().bold()));
    }
    help.push_str("  $ quarry init                      # Initialize in current directory\n");
    help.push_str("  $ quarry index src lib            # Index multiple directories\n");
    help.push_str("  $ quarry add-dir tests            # Add tests directory to indexed paths\n");
    help.push_str("  $ quarry list-dirs                # List all indexed directories\n");
    help.push_str("  $ quarry serve                    # Persistent MCP server (low-latency)\n");
    help.push_str("  $ quarry serve --http --watch     # HTTP server with OAuth\n");
    help.push_str("  $ quarry serve --https --watch    # HTTPS server with TLS\n");
    help.push_str("  $ quarry documents add-collection docs ./docs  # Add doc collection\n");
    help.push_str("  $ quarry documents index          # Index all document collections\n\n");

    // About section
    help.push_str("Index code and query relationships, symbols, and dependencies.\n\n");

    // Usage
    if Theme::should_disable_colors() {
        help.push_str("Usage:");
    } else {
        help.push_str(&format!("{}", style("Usage:").cyan().bold()));
    }
    help.push_str(" quarry [OPTIONS] <COMMAND>\n\n");

    // Commands
    if Theme::should_disable_colors() {
        help.push_str("Commands:\n");
    } else {
        help.push_str(&format!("{}\n", style("Commands:").cyan().bold()));
    }
    help.push_str("  init          Set up .quarry directory\n");
    help.push_str("  index         Build searchable index from codebase\n");
    help.push_str("  add-dir       Add a directory to be indexed\n");
    help.push_str("  remove-dir    Remove a directory from indexed paths\n");
    help.push_str("  list-dirs     List all directories that are being indexed\n");
    help.push_str("  retrieve      Query symbols, relationships, and dependencies\n");
    help.push_str("  serve         Start persistent MCP server (low-latency)\n");
    help.push_str("  config        Display active settings\n");
    help.push_str("  mcp-test      Test MCP connection\n");
    help.push_str("  mcp           Execute MCP tools directly (one-shot)\n");
    help.push_str("  benchmark     Benchmark parser performance\n");
    help.push_str("  benchmark-rerank  Persistent reranker benchmark (quality + latency)\n");
    help.push_str("  benchmark-rerank-quick  Quick reranker latency benchmark (no qrels)\n");
    help.push_str("  parse         Output AST nodes in JSONL format\n");
    help.push_str("  plugin        Manage Claude Code plugins\n");
    help.push_str("  documents     Index and search document collections\n");
    help.push_str("  help          Print this message or the help of the given subcommand(s)\n\n");

    help.push_str("See 'quarry help <command>' for more information on a specific command.\n\n");

    // Options
    if Theme::should_disable_colors() {
        help.push_str("Options:\n");
    } else {
        help.push_str(&format!("{}\n", style("Options:").cyan().bold()));
    }
    help.push_str("  -c, --config <CONFIG>  Path to custom settings.toml file\n");
    help.push_str("      --info             Show detailed loading information\n");
    help.push_str("  -h, --help             Print help\n");
    help.push_str("  -V, --version          Print version\n\n");

    // Learn More
    if Theme::should_disable_colors() {
        help.push_str("Learn More:\n");
    } else {
        help.push_str(&format!("{}\n", style("Learn More:").cyan().bold()));
    }
    help.push_str("  GitHub: https://github.com/bartolli/codanna");

    help
}

/// Code intelligence system
#[derive(Parser)]
#[command(
    name = "quarry",
    version = env!("CARGO_PKG_VERSION"),
    about = "Code intelligence system",
    long_about = "Index code and query relationships, symbols, and dependencies.",
    next_line_help = true,
    styles = clap_cargo_style(),
    override_help = create_custom_help()
)]
pub struct Cli {
    /// Path to custom settings.toml file
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    /// Show detailed loading information
    #[arg(long, global = true)]
    pub info: bool,

    #[command(subcommand)]
    pub command: Commands,
}

/// Available CLI commands
#[derive(Subcommand)]
pub enum Commands {
    /// Initialize project
    #[command(about = "Set up .quarry directory with default configuration")]
    Init {
        /// Force overwrite existing configuration
        #[arg(short, long)]
        force: bool,
    },

    /// Index source files or directories
    #[command(about = "Build searchable index from codebase")]
    Index {
        /// Paths to files or directories to index (multiple paths allowed)
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,

        /// Number of threads to use (overrides config)
        #[arg(short, long)]
        threads: Option<usize>,

        /// Force re-indexing even if index exists
        #[arg(short, long)]
        force: bool,

        /// Disable progress bars (overrides settings.toml show_progress)
        #[arg(long)]
        no_progress: bool,

        /// Dry run - show what would be indexed without indexing
        #[arg(long)]
        dry_run: bool,

        /// Maximum number of files to index
        #[arg(long)]
        max_files: Option<usize>,
    },

    /// Add a directory to the indexed paths list
    #[command(about = "Add a directory to be indexed")]
    AddDir {
        /// Path to directory to add
        path: PathBuf,
    },

    /// Remove a directory from the indexed paths list
    #[command(about = "Remove a directory from indexed paths")]
    RemoveDir {
        /// Path to directory to remove
        path: PathBuf,
    },

    /// List all indexed directories
    #[command(about = "List all directories that are being indexed")]
    ListDirs,

    /// Query code relationships and dependencies
    #[command(
        about = "Search symbols, find callers/callees, analyze impact",
        long_about = "Query indexed symbols, relationships, and dependencies.",
        after_help = "Examples:\n  quarry retrieve symbol main\n  quarry retrieve callers process_file\n  quarry retrieve callers symbol_id:1771\n  quarry retrieve calls init\n  quarry retrieve calls symbol_id:1771\n  quarry retrieve implementations Parser\n  quarry retrieve describe OutputManager\n  quarry retrieve search \"parse\" --limit 10\n\nJSON paths:\n  retrieve symbol     .data.items[0].symbol.name\n  retrieve search     .data.items[].symbol.name\n  retrieve callers    .data.items[].symbol.name\n  retrieve describe   .data.items[0].symbol.name"
    )]
    Retrieve {
        #[command(subcommand)]
        query: RetrieveQuery,
    },

    /// Search the codebase in plain English
    #[command(
        about = "Search the codebase in plain English",
        long_about = "Ask for what the code does rather than what it is called. Prints paths, \
                      line ranges and a short preview, like grep does.",
        after_help = "Examples:\n  quarry search \"where do we validate the auth token\"\n  \
                      quarry search \"retry with backoff\" --limit 5\n  \
                      quarry search \"parse the config file\" --lang rust --json"
    )]
    Search {
        /// What you are looking for, in your own words
        #[arg(value_name = "QUERY")]
        query: String,

        /// Maximum number of results
        #[arg(short, long, default_value_t = 20)]
        limit: usize,

        /// Restrict to one language (rust, python, typescript, go, ...)
        #[arg(long)]
        lang: Option<String>,

        /// Emit JSON instead of the human-readable listing
        #[arg(long)]
        json: bool,
    },

    /// Show current configuration settings
    #[command(about = "Display active settings from .quarry/settings.toml")]
    Config,

    /// Start MCP server (persistent, recommended for low-latency queries)
    #[command(
        about = "Start persistent MCP server (recommended for low-latency)",
        long_about = "Start a long-lived MCP server. Recommended for repeated low-latency queries.",
        after_help = "Examples:\n  quarry serve\n  quarry serve --http --watch\n  quarry serve --https --watch\n  quarry serve --http --bind 0.0.0.0:3000\n\nModes:\n  Default: stdio\n  --http: HTTP with OAuth\n  --https: HTTPS with TLS"
    )]
    Serve {
        /// Watch index file for changes and auto-reload
        #[arg(long, help = "Enable hot-reload when index changes")]
        watch: bool,

        /// Check interval in seconds (default: 5)
        #[arg(
            long,
            default_value = "5",
            help = "How often to check for index changes"
        )]
        watch_interval: u64,

        /// Enable HTTP server mode instead of stdio
        #[arg(long, help = "Run as HTTP server instead of stdio transport")]
        http: bool,

        /// Enable HTTPS server mode with TLS
        #[arg(
            long,
            conflicts_with = "http",
            help = "Run as HTTPS server with TLS support"
        )]
        https: bool,

        /// Bind address for HTTP/HTTPS server
        #[arg(
            long,
            default_value = "127.0.0.1:8080",
            help = "Address to bind HTTP/HTTPS server to"
        )]
        bind: String,
    },

    /// Test MCP connection
    #[command(name = "mcp-test", about = "Test MCP connection and list tools")]
    McpTest {
        /// Path to server binary (defaults to current binary)
        #[arg(long)]
        server_binary: Option<PathBuf>,

        /// Tool to call (if not specified, just lists tools)
        #[arg(long)]
        tool: Option<String>,

        /// Tool arguments as JSON
        #[arg(long)]
        args: Option<String>,

        /// Delay (seconds) before calling the tool, to exercise watcher reloads
        #[arg(
            long,
            help = "Wait N seconds before calling the tool",
            value_name = "SECONDS"
        )]
        delay: Option<u64>,

        /// Repeat tool call N times in one persistent MCP session
        #[arg(long, default_value_t = 1, value_name = "N")]
        repeat: usize,
    },

    /// Call MCP tools directly (one-shot, advanced)
    #[command(
        about = "Execute MCP tools directly (one-shot)",
        long_about = "Execute MCP tools directly in one-shot mode. For repeated low-latency queries, prefer `quarry serve`.\n\nSupports positional arguments, key=value pairs, and JSON arguments.",
        after_help = "Tools:\n  find_symbol       <name>              Exact name lookup\n  search_symbols    query:<text>        Fuzzy text search (kind:<type> limit:<n>)\n  get_calls         <name|symbol_id:N>  What this symbol calls\n  find_callers      <name|symbol_id:N>  What calls this symbol\n  analyze_impact    <name|symbol_id:N>  Full dependency graph\n  semantic_search_docs query:<text>     Symbol-based, for code navigation\n  semantic_search_chunks query:<text>   Chunk-based, for code flow/logic understanding\n  semantic_search_with_context query:<text>  Search with relationships\n  search_documents  query:<text>        Search markdown/text docs\n  get_index_info                        Index stats\n\nExamples:\n  quarry mcp find_symbol <name>\n  quarry mcp search_symbols query:<text> kind:function\n  quarry mcp get_calls <name>\n  quarry mcp get_calls symbol_id:<N>\n  quarry mcp semantic_search_docs query:\"<text>\" limit:5\n  quarry mcp semantic_search_chunks query:\"<text>\" limit:5\n  quarry mcp search_symbols query:<text> --json | jq '.data[].symbol_id'"
    )]
    Mcp {
        /// Tool to call
        tool: String,

        /// Positional arguments (can be simple values or key:value pairs)
        #[arg(num_args = 0..)]
        positional: Vec<String>,

        /// Tool arguments as JSON (for backward compatibility and complex cases)
        #[arg(long)]
        args: Option<String>,

        /// Output in JSON format
        #[arg(long)]
        json: bool,

        /// Filter output to specific fields (comma-separated)
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,

        /// Check for file changes and reindex before running tool
        #[arg(long)]
        watch: bool,
    },

    /// Benchmark parser performance
    #[command(about = "Benchmark parser performance")]
    Benchmark {
        /// Language to benchmark (rust, python, php, typescript, go, csharp, all)
        #[arg(default_value = "all")]
        language: String,

        /// Custom file to benchmark
        #[arg(short, long)]
        file: Option<PathBuf>,
    },

    /// Benchmark reranker quality/latency with persistent in-process execution
    #[command(
        name = "benchmark-rerank",
        about = "Persistent reranker benchmark (quality + latency)",
        long_about = "Run chunk-search reranker benchmarks in persistent in-process mode (no one-shot model reload per query).\n\nProduces JSON and Markdown reports with quality metrics (Hit@1, MRR@10, nDCG@10, Recall@5/10), cold/warm latency, timeout miss split, and Pareto frontier.",
        after_help = "Examples:\n  quarry benchmark-rerank --queries ./bench/queries.jsonl --qrels ./bench/qrels.jsonl --profiles ./bench/profiles.toml --out ./bench/out\n  quarry benchmark-rerank --queries ./bench/queries.jsonl --qrels ./bench/qrels.jsonl --profiles ./bench/profiles.toml --cold-runs 1 --warm-runs 3 --limit 10"
    )]
    BenchmarkRerank {
        /// Query set JSONL path ({\"id\":\"...\",\"query\":\"...\"})
        #[arg(long, value_name = "PATH")]
        queries: PathBuf,

        /// Qrels JSONL path ({\"query_id\":\"...\",\"chunk_id\":123,\"grade\":2})
        #[arg(long, value_name = "PATH")]
        qrels: PathBuf,

        /// Profile matrix TOML path
        #[arg(long, value_name = "PATH")]
        profiles: PathBuf,

        /// Output directory for reports
        #[arg(long, value_name = "DIR")]
        out: PathBuf,

        /// Number of cold runs per query
        #[arg(long, default_value_t = 1)]
        cold_runs: usize,

        /// Number of warm runs per query
        #[arg(long, default_value_t = 3)]
        warm_runs: usize,

        /// Retrieval limit (top-k)
        #[arg(long, default_value_t = 10)]
        limit: usize,

        /// Timeout threshold per query run in milliseconds (0 disables)
        #[arg(long, default_value_t = 20_000)]
        query_timeout_ms: u64,

        /// Write checkpoint files every N completed queries per profile
        #[arg(long, default_value_t = 1)]
        checkpoint_every: usize,

        /// Skip warm runs for a query when cold run timed out
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        skip_warm_on_timeout: bool,
    },

    /// Quick reranker latency benchmark (no qrels needed)
    #[command(
        name = "benchmark-rerank-quick",
        about = "Quick reranker latency benchmark (10 built-in queries, no qrels needed)"
    )]
    BenchmarkRerankQuick {
        /// Number of warm runs per query (median selected)
        #[arg(long, default_value_t = 3)]
        warm_runs: usize,

        /// Retrieval limit (top-k)
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },

    /// Parse a file and output AST nodes in JSONL format
    #[command(about = "Parse file and output AST as JSON Lines")]
    Parse {
        /// File to parse
        file: PathBuf,

        /// Output file (defaults to stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Maximum depth to traverse
        #[arg(short = 'd', long)]
        max_depth: Option<usize>,

        /// Include all nodes (by default only named nodes are shown, like tree-sitter)
        #[arg(short = 'a', long)]
        all_nodes: bool,
    },

    /// Manage Claude Code plugins
    #[command(
        about = "Install, update, and manage Claude Code plugins from marketplaces",
        long_about = "Manage Claude Code plugins by installing from Git-based marketplaces.\n\nPlugins extend Claude Code with custom commands, agents, hooks, and MCP servers.\n\nNote: Plugins are installed and managed by quarry per-project in .claude/plugins, not managed by claude code CLI directly.",
        after_help = "Examples:\n  quarry plugin add https://github.com/user/marketplace plugin-name\n  quarry plugin remove plugin-name\n  quarry plugin update plugin-name --ref v2.0\n  quarry plugin list\n  quarry plugin verify plugin-name"
    )]
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },

    /// Index and search document collections for RAG
    #[command(
        about = "Index and search document collections",
        long_about = "Index markdown and text documents for semantic search.\n\nDocuments are chunked, embedded, and stored separately from code symbols.",
        after_help = "Examples:\n  quarry documents index --collection docs\n  quarry documents search \"error handling\" --collection docs\n  quarry documents list\n  quarry documents stats docs"
    )]
    Documents {
        #[command(subcommand)]
        action: DocumentAction,
    },

    /// Index with parallel pipeline (experimental)
    #[command(
        name = "index-parallel",
        about = "Index using parallel pipeline with two-phase resolution",
        long_about = "Index source code using the parallel pipeline architecture.\n\nPhase 1: Parallel file discovery, reading, parsing, and indexing.\nPhase 2: Two-pass cross-file relationship resolution.",
        after_help = "Examples:\n  quarry index-parallel src\n  quarry index-parallel --no-progress\n  quarry index-parallel src lib --force"
    )]
    IndexParallel {
        /// Paths to directories to index (uses settings.toml indexed_paths if empty)
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,

        /// Force re-indexing (clears existing index)
        #[arg(short, long)]
        force: bool,

        /// Disable progress bars (overrides settings.toml show_progress)
        #[arg(long)]
        no_progress: bool,
    },

    /// Manage project profiles
    #[command(
        about = "Initialize and manage project profiles",
        long_about = "Manage project profiles for provider-specific initialization.\n\nProfiles set up project structure, configuration files, and provider integration.",
        after_help = "Examples:\n  quarry profile init claude\n  quarry profile install claude --source git@github.com:quarry/profiles.git\n  quarry profile list\n  quarry profile status"
    )]
    Profile {
        #[command(subcommand)]
        action: crate::profiles::commands::ProfileAction,
    },
}

/// Plugin management actions
#[derive(Subcommand)]
pub enum PluginAction {
    /// Install a plugin from a marketplace
    #[command(
        about = "Install a plugin from a marketplace repository",
        after_help = "Examples:\n  quarry plugin add https://github.com/user/marketplace plugin-name\n  quarry plugin add ./local-marketplace my-plugin --ref v1.0"
    )]
    Add {
        /// Marketplace repository URL or local path
        marketplace: String,

        /// Plugin name to install
        plugin_name: String,

        /// Git reference (branch, tag, or commit SHA)
        #[arg(long)]
        r#ref: Option<String>,

        /// Force installation even if conflicts exist
        #[arg(short, long)]
        force: bool,

        /// Perform a dry run without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Remove an installed plugin
    #[command(
        about = "Remove an installed plugin and clean up its files",
        after_help = "Example:\n  quarry plugin remove plugin-name"
    )]
    Remove {
        /// Plugin name to remove
        plugin_name: String,

        /// Force removal even if other plugins depend on it
        #[arg(short, long)]
        force: bool,

        /// Perform a dry run without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Update an installed plugin
    #[command(
        about = "Update a plugin to a newer version",
        after_help = "Examples:\n  quarry plugin update plugin-name\n  quarry plugin update plugin-name --ref v2.0"
    )]
    Update {
        /// Plugin name to update
        plugin_name: String,

        /// Git reference to update to (branch, tag, or commit SHA)
        #[arg(long)]
        r#ref: Option<String>,

        /// Force update even if local modifications exist
        #[arg(short, long)]
        force: bool,

        /// Perform a dry run without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// List installed plugins
    #[command(
        about = "List all installed plugins with their versions",
        after_help = "Example:\n  quarry plugin list"
    )]
    List {
        /// Show detailed information
        #[arg(short, long)]
        verbose: bool,

        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Verify plugin integrity
    #[command(
        about = "Verify that a plugin's files match their expected checksums",
        after_help = "Examples:\n  quarry plugin verify plugin-name\n  quarry plugin verify --all"
    )]
    Verify {
        /// Plugin name to verify (omit to verify all)
        plugin_name: Option<String>,

        /// Verify all installed plugins
        #[arg(long)]
        all: bool,

        /// Show detailed verification results
        #[arg(short, long)]
        verbose: bool,
    },
}

/// Document collection management actions
#[derive(Subcommand)]
pub enum DocumentAction {
    /// Index documents from a collection
    #[command(
        about = "Index documents from a configured collection",
        after_help = "Examples:\n  quarry documents index --collection docs\n  quarry documents index --all\n  quarry documents index --no-progress"
    )]
    Index {
        /// Collection name to index (from settings.toml)
        #[arg(long)]
        collection: Option<String>,

        /// Index all configured collections
        #[arg(long)]
        all: bool,

        /// Force re-indexing of all files
        #[arg(short, long)]
        force: bool,

        /// Disable progress bars (overrides settings.toml show_progress)
        #[arg(long)]
        no_progress: bool,
    },

    /// Search documents
    #[command(
        about = "Search indexed documents using natural language",
        after_help = "Examples:\n  quarry documents search \"error handling\"\n  quarry documents search \"authentication\" --collection docs --limit 5\n  quarry documents search query:\"auth\" limit:3 --json"
    )]
    Search {
        /// Positional arguments (query and/or key:value pairs like limit:5)
        #[arg(num_args = 0..)]
        args: Vec<String>,

        /// Filter by collection name
        #[arg(long)]
        collection: Option<String>,

        /// Maximum results to return
        #[arg(short, long)]
        limit: Option<usize>,

        /// Output in JSON format
        #[arg(long)]
        json: bool,

        /// Select specific fields in JSON output (comma-separated)
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,
    },

    /// List collections
    #[command(
        about = "List all document collections",
        after_help = "Example:\n  quarry documents list"
    )]
    List {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Show collection statistics
    #[command(
        about = "Show statistics for a collection",
        after_help = "Example:\n  quarry documents stats docs"
    )]
    Stats {
        /// Collection name
        collection: String,

        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Add a collection to settings.toml
    #[command(
        about = "Add a document collection to settings.toml",
        after_help = "Examples:\n  quarry documents add-collection docs ./docs\n  quarry documents add-collection api-docs ./api --pattern \"**/*.md\""
    )]
    AddCollection {
        /// Collection name
        name: String,

        /// Path to include in the collection
        path: PathBuf,

        /// Glob pattern for file matching (default: **/*.md)
        #[arg(short, long)]
        pattern: Option<String>,
    },

    /// Remove a collection from settings.toml
    #[command(
        about = "Remove a document collection from settings.toml",
        after_help = "Examples:\n  quarry documents remove-collection docs\n\nNote: Run 'quarry documents index' after to clean the index."
    )]
    RemoveCollection {
        /// Collection name to remove
        name: String,
    },
}

/// Query types for retrieving indexed information.
///
/// Supports symbol lookups, relationship queries, impact analysis, and full-text search.
#[derive(Subcommand)]
pub enum RetrieveQuery {
    /// Find a symbol by name
    #[command(
        after_help = "Examples:\n  quarry retrieve symbol main\n  quarry retrieve symbol symbol_id:1771\n  quarry retrieve symbol name:main --json\n  quarry retrieve symbol MyStruct --json | jq '.file'\n  quarry retrieve symbol main --json --fields=id,name,file_path"
    )]
    Symbol {
        /// Positional arguments (symbol name and/or key:value pairs)
        #[arg(num_args = 0..)]
        args: Vec<String>,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Filter output to specific fields (comma-separated)
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,
    },

    /// Show what functions a given function calls
    #[command(
        after_help = "Examples:\n  quarry retrieve calls process_file\n  quarry retrieve calls symbol_id:1771\n  quarry retrieve calls function:process_file --json\n  quarry retrieve calls main --json --fields=name,file_path"
    )]
    Calls {
        /// Positional arguments (function name and/or key:value pairs)
        #[arg(num_args = 0..)]
        args: Vec<String>,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Filter output to specific fields (comma-separated)
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,
    },

    /// Show what functions call a given function
    #[command(
        after_help = "Examples:\n  quarry retrieve callers main\n  quarry retrieve callers symbol_id:1771\n  quarry retrieve callers function:main --json\n  quarry retrieve callers main --json --fields=name,file_path"
    )]
    Callers {
        /// Positional arguments (function name and/or key:value pairs)
        #[arg(num_args = 0..)]
        args: Vec<String>,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Filter output to specific fields (comma-separated)
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,
    },

    /// Show what types implement a given trait
    #[command(
        after_help = "Examples:\n  quarry retrieve implementations Parser\n  quarry retrieve implementations trait:Parser --json\n  quarry retrieve implementations Parser --json --fields=name,file_path"
    )]
    Implementations {
        /// Positional arguments (trait name and/or key:value pairs)
        #[arg(num_args = 0..)]
        args: Vec<String>,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Filter output to specific fields (comma-separated)
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,
    },

    /// Search for symbols using full-text search
    #[command(
        after_help = "Examples:\n  # Traditional flag format\n  quarry retrieve search \"parse\" --limit 5 --kind function\n  \n  # Key:value format (Unix-style)\n  quarry retrieve search query:parse limit:5 kind:function\n  \n  # Mixed format\n  quarry retrieve search \"parse\" limit:5 --json\n  quarry retrieve search \"parse\" --json --fields=name,file_path"
    )]
    Search {
        /// Positional arguments (query and/or key:value pairs)
        #[arg(num_args = 0..)]
        args: Vec<String>,

        /// Maximum number of results (flag format)
        #[arg(short, long)]
        limit: Option<usize>,

        /// Filter by symbol kind (flag format)
        #[arg(short, long)]
        kind: Option<String>,

        /// Filter by module path (flag format)
        #[arg(short, long)]
        module: Option<String>,

        /// Output in JSON format
        #[arg(long)]
        json: bool,

        /// Filter output to specific fields (comma-separated)
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,
    },

    /// Show information about a symbol
    #[command(
        after_help = "Examples:\n  quarry retrieve describe SimpleIndexer\n  quarry retrieve describe symbol:SimpleIndexer --json\n  quarry retrieve describe main --json --fields=name,kind,calls"
    )]
    Describe {
        /// Positional arguments (symbol name and/or key:value pairs)
        #[arg(num_args = 0..)]
        args: Vec<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Filter output to specific fields (comma-separated)
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,
    },
}
