# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Codanna is a Rust-based code intelligence tool that provides semantic search, call graph analysis, and relationship tracking for AI coding assistants via the MCP (Model Context Protocol). It parses source code using tree-sitter, indexes symbols into Tantivy, generates vector embeddings (optimized int8 static model for model2vec, fastembed for transformer models), and exposes everything through MCP tools.

## Build & Development Commands

```bash
# Build
cargo build --release --all-features

# Run tests
cargo test                              # all tests
cargo test test_name                    # single test
cargo test --test integration_tests     # integration tests only
cargo test --test parsers_tests         # parser tests only

# Lint & format
cargo fmt --check                       # check formatting
cargo fmt                               # auto-format
cargo clippy -- -D warnings             # lint (CI enforcement level)

# Pre-commit workflow (use these scripts)
./contributing/scripts/quick-check.sh   # fmt + clippy + check (~2-3 min)
./contributing/scripts/auto-fix.sh      # auto-fix fmt/clippy issues
./contributing/scripts/full-test.sh     # full CI replication before PR

# Run the CLI
cargo run -- init
cargo run -- index src
cargo run -- mcp semantic_search_with_context query:"search term" limit:5
cargo run -- serve                      # MCP stdio server
cargo run -- serve --http --watch       # HTTP server with file watching

# Benchmarks
cargo bench --bench unified_output_bench
cargo bench --bench kotlin_parser_bench
cargo bench --bench static_embed_bench  # int8 embedding engine benchmark
cargo run -- benchmark all              # parser benchmark via CLI

# Parse AST (useful for parser development)
cargo run -- parse file.rs              # JSONL AST output
cargo run -- parse file.rs --all-nodes  # include anonymous nodes
```

## Architecture

### Core Pipeline: Parse → Index → Store → Search

1. **Parsing** (`src/parsing/`): Tree-sitter based parsers for 14 languages. Each language has its own subdirectory with 6 files: `mod.rs`, `parser.rs`, `behavior.rs`, `resolution.rs`, `definition.rs`, `audit.rs`. All parsers implement `LanguageParser` + `LanguageBehavior` traits and self-register via `LanguageRegistry` (`registry.rs`).

2. **Indexing** (`src/indexing/`): `IndexFacade` is the primary API. It orchestrates file walking (`walker.rs`), parsing, and storing symbols. The `pipeline/` subdirectory implements a parallel indexing pipeline with configurable stages. `transaction.rs` handles atomic index updates.

3. **Storage** (`src/storage/`): Dual storage layer:
   - **Tantivy** (`tantivy.rs`): Full-text search index for symbol lookup via `DocumentIndex`
   - **Memory-mapped cache** (`memory.rs`): Fast binary symbol cache via `IndexPersistence` (`persistence.rs`)
   - **Metadata** (`metadata.rs`): Index state tracking (indexed paths, timestamps)

4. **Vector Search** (`src/vector/`): Semantic search using fastembed embeddings for transformer models. `VectorSearchEngine` manages embedding generation (`FastEmbedGenerator`), IVFFlat clustering (`clustering.rs`), and memory-mapped vector storage (`MmapVectorStorage`).

5. **Semantic Search** (`src/semantic/`): Embedding pool management (`pool.rs`) with concurrent model instances. Default backend is `OptimizedStaticModel` (`static_model.rs`) — a custom int8 embedding engine that keeps embeddings in native int8 format at runtime (31MB vs 123MB for f32), uses i32 accumulation with LLVM autovectorization, and rayon-parallel batch encoding (~201K symbols/sec). HuggingFace model names are auto-resolved to local paths via `resolve_model_path()`. Bridges vector search with Tantivy results.

6. **MCP Server** (`src/mcp/mod.rs`): Implements MCP protocol via `rmcp` crate. Supports stdio, HTTP (`http_server.rs`), and HTTPS (`https_server.rs`) transports. Exposes tools: `find_symbol`, `search_symbols`, `get_calls`, `find_callers`, `analyze_impact`, `semantic_search_docs`, `semantic_search_with_context`, `search_documents`, `get_index_info`, `get_source`.

7. **Documents/RAG** (`src/documents/`): Indexes markdown/text files for retrieval-augmented generation. Chunks documents (`chunker.rs`), stores in `DocumentStore`, searchable via MCP.

### Key Supporting Modules

- **`src/config.rs`**: Layered config system (defaults → TOML → env vars `CI_*` → CLI args) via `figment`. Central `Settings` struct.
- **`src/relationship/mod.rs`**: `Relationship` and `RelationshipEdge` types for call graphs and dependency tracking.
- **`src/symbol/mod.rs`**: `Symbol` and `CompactSymbol` types. `StringTable` for interned strings.
- **`src/types/`**: Core types including `SymbolId`, `FileId`, `SymbolKind`, `CompactString`.
- **`src/project_resolver/`**: Language-specific project config resolution (tsconfig.json, go.mod, pom.xml, etc.) for enhanced import path resolution.
- **`src/cli/`**: Clap-based CLI. Commands defined in `args.rs`, handlers in `commands/`.
- **`src/display/`**: Terminal output formatting and theming.
- **`src/io/`**: Output formatting and guidance engine for MCP responses.
- **`src/plugins/`**: Claude Code plugin management system.
- **`src/profiles/`**: Project profile management for provider-specific setup.

### Configuration

Settings live in `.codanna/settings.toml`. Key config sections: `indexing`, `semantic_search`, `mcp`, `server`, `file_watch`, `logging`, `documents`, `guidance`, and per-language `[languages.*]` with optional `config_files` for project resolution.

Environment variable overrides use `CI_` prefix with double underscore nesting: `CI_INDEXING__PARALLELISM=8`.

## Rust Coding Conventions (Project-Specific)

- **Zero-cost abstractions**: Use `&str` over `String`, `&[T]` over `Vec<T>` in function params. Return iterators when callers vary in needs, `Vec<T>` when callers always collect.
- **Newtypes for domain values**: `SymbolId`, `FileId`, `ClusterId`, `Score`, etc. No raw primitives for IDs.
- **Error handling**: `thiserror` for library errors, `anyhow` at binary level. Include "Suggestion:" in error messages for user-facing errors.
- **Performance targets**: >10K symbols/sec parsing, <10ms symbol lookups, <300ms semantic search, ~100 bytes/symbol in cache, ~201K symbols/sec embedding (int8 rayon batch).
- **Clippy**: CI enforces `cargo clippy -- -D warnings`. See `clippy.toml` for thresholds (cognitive complexity: 30, too-many-args: 12).
- **Edition 2024**, MSRV 1.85.

## Adding a New Language Parser

Each language requires 6 files in `src/parsing/{language}/`:
- `mod.rs` - module exports and registration via `LanguageRegistry`
- `parser.rs` - implements `LanguageParser` trait (AST traversal)
- `behavior.rs` - implements `LanguageBehavior` trait (language semantics)
- `resolution.rs` - language-specific symbol resolution context
- `definition.rs` - language definition for the registry
- `audit.rs` - node coverage tracking

Also requires: tree-sitter grammar dependency in `Cargo.toml`, registration in `src/parsing/mod.rs`, and a project resolver provider in `src/project_resolver/providers/` if the language has build config files.

## Test Organization

Tests live in `tests/` with separate test files per domain:
- `parsers_tests.rs` / `tests/parsers/` - language parser tests
- `integration_tests.rs` / `tests/integration/` - end-to-end indexing tests
- `semantic_tests.rs` / `tests/semantic/` - semantic search tests
- `cli_tests.rs` / `tests/cli/` - CLI command tests
- `plugins_tests.rs` / `tests/plugins/` - plugin system tests
- `tests/fixtures/` - test fixture files

## Working Style

- **No subagent for analysis**: When exploring, searching, or analyzing code, do it directly with Glob/Grep/Read tools — never delegate research to subagents. Subagents are only for truly independent parallel tasks, not for code exploration.

## Retrieval quality — measure, never guess

Any change touching chunk search, embeddings, fusion or ranking is measured with
`benchmarks/retrieval/` (leakage-free ground truth from git history; primary metric R@10).
Read that README before tuning anything — it holds the discipline rules and the
measured verdicts. Active plan: `docs/plans/retrieval-tuning.md`.

Two settings carry the retrieval quality and were both derived from measurement, not taste
(`[chunk_search]`): `fusion_alpha` weights the dense arm in a min-max normalised convex
combination instead of RRF, and `file_evidence_alpha` lets several matching chunks of one
file compound its score. Changing either without re-running the suite is a regression risk.

Foot-guns learned the hard way:
- **Bool env overrides must be lowercase** (`CI_..._ENABLED=true|false`). `"True"/"False"`
  fails figment parsing and silently drops **all** Settings to defaults — including
  `reranking.enabled=true`. Symptom: query latency jumps from ~4 ms to ~2000 ms.
- **`post_rerank_heuristics_enabled` is false by default**, and `facade.rs` returns early
  when it is false — so source weights, penalties, symbol-aware scoring, diversity and the
  result filter are all dead code in the default path. Turning them on gains nothing
  (measured d=0.000) and doubles latency.
- **int8 quantization of a model2vec table must use a single global symmetric scale.**
  Per-row scales do not cancel under L2 normalisation and corrupt the model.
- A static embedding model must live at `~/.codanna/models/<name>-int8/` with
  `model.safetensors` (I8 `embeddings` tensor), `tokenizer.json`, `config.json`.
- Noise floor on the 123-query holdout is **±0.016**; treat anything under 0.05 as noise
  and confirm with paired win/loss counts, never with a mean difference alone.
- **`codanna init` freezes every default into `settings.toml`** (`toml::to_string_pretty`
  over the whole `Settings::default()`), so changing a Rust default reaches new projects
  only — every already-initialised project stays pinned to the config it got on day one.
  After changing a default, update the eval repos' `.codanna/settings.toml` too, or the
  measurement silently reports the old behaviour.
- **`top_k_fused` is the ceiling on file-level recall,** not a ranking knob. With
  `diversity_max_per_file = 1` the fused pool caps how many *distinct files* can ever be
  returned; at the old value of 50 no amount of reranking could pass R@50 ≈ 0.80.

## Features

- `default = ["http-server"]` - includes HTTP server (axum)
- `https-server` - adds TLS support (axum-server + rustls + rcgen)
- GPU features commented out in Cargo.toml (cuda, coreml, etc.)
