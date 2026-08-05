//! Collect stage - ID assignment and batching
//!
//! Single-threaded stage that:
//! - Assigns FileId and SymbolId sequentially
//! - Converts RawSymbol -> Symbol
//! - Converts RawImport -> Import
//! - Converts RawRelationship -> UnresolvedRelationship (resolving from_id)
//! - Batches output for efficient Tantivy writes

use crate::CompactString;
use crate::indexing::pipeline::types::{
    EmbeddingBatch, FileRegistration, IndexBatch, ParsedFile, PipelineResult, RawRelationship,
    RawSymbol, UnresolvedRelationship,
};
use crate::semantic::SimpleSemanticSearch;
use crate::symbol::{ScopeContext, Symbol};
use crate::types::{FileId, Range, SymbolId, SymbolKind};
use crate::utils::get_utc_timestamp;
use crossbeam_channel::{Receiver, Sender};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Callback type for reporting embedding candidate totals to progress display.
pub type EmbedTotalCallback = Arc<dyn Fn(u64) + Send + Sync>;

/// Collect stage for ID assignment and batching.
pub struct CollectStage {
    batch_size: usize,
    max_embedding_chars: usize,
    embed_batch_token_budget: usize,
    embed_batch_byte_budget: usize,
    exclude_generated_for_semantic: bool,
    semantic_symbol_kinds: Option<HashSet<SymbolKind>>,
    /// Starting file counter (for continuing from existing index)
    start_file_counter: u32,
    /// Starting symbol counter (for continuing from existing index)
    start_symbol_counter: u32,
}

/// Ephemeral caches for relationship reconnection.
///
/// Design from decisions.md:
/// - `symbol_lookup`: (name, file_id, range) -> SymbolId for disambiguation
/// - `name_in_file`: Fallback when range doesn't match exactly
/// - `file_ids`: PathBuf -> FileId for Phase 2 cross-file resolution
/// - `symbols_by_file`: FileId -> Vec<(Range, SymbolId)> for range containment fallback
///
/// Uses Arc<str> for zero-copy name sharing.
struct CollectorCaches {
    /// Primary lookup: (name, file_id, range) -> SymbolId
    symbol_lookup: HashMap<(Arc<str>, FileId, Range), SymbolId>,
    /// Fallback: (name, file_id) -> SymbolId for when range doesn't match exactly
    name_in_file: HashMap<(Arc<str>, FileId), SymbolId>,
    /// Path to FileId mapping for Phase 2 resolution
    file_ids: HashMap<PathBuf, FileId>,
    /// All symbols in a file with their ranges, for range containment fallback.
    /// Used when from_name doesn't match any symbol — find the enclosing symbol by range.
    symbols_by_file: HashMap<FileId, Vec<(Range, SymbolId)>>,
}

impl CollectorCaches {
    fn new() -> Self {
        Self {
            symbol_lookup: HashMap::new(),
            name_in_file: HashMap::new(),
            file_ids: HashMap::new(),
            symbols_by_file: HashMap::new(),
        }
    }

    /// Register a file path -> FileId mapping for Phase 2 resolution.
    fn insert_file(&mut self, path: PathBuf, file_id: FileId) {
        self.file_ids.insert(path, file_id);
    }

    /// Lookup FileId by path (for Phase 2 cross-file resolution).
    #[allow(dead_code)] // Used in Phase 2
    fn lookup_file(&self, path: &PathBuf) -> Option<FileId> {
        self.file_ids.get(path).copied()
    }

    /// Insert a symbol into the cache. Uses Arc::clone for zero-copy.
    fn insert(&mut self, name: Arc<str>, file_id: FileId, range: Range, symbol_id: SymbolId) {
        self.symbol_lookup
            .insert((Arc::clone(&name), file_id, range), symbol_id);
        self.name_in_file.insert((name, file_id), symbol_id);
        self.symbols_by_file
            .entry(file_id)
            .or_default()
            .push((range, symbol_id));
    }

    /// Lookup by exact (name, file_id, range) match.
    fn lookup(&self, name: &Arc<str>, file_id: FileId, range: Range) -> Option<SymbolId> {
        self.symbol_lookup
            .get(&(Arc::clone(name), file_id, range))
            .copied()
    }

    /// Fallback lookup by (name, file_id) when range doesn't match.
    fn lookup_by_name_in_file(&self, name: &Arc<str>, file_id: FileId) -> Option<SymbolId> {
        self.name_in_file.get(&(Arc::clone(name), file_id)).copied()
    }

    /// Find the smallest enclosing symbol for a given range in a file.
    ///
    /// When from_name doesn't match any symbol (e.g., callback names like "handleClick"
    /// that aren't top-level symbols), fall back to finding the symbol whose range
    /// contains the from_range. Prefers the smallest (most specific) enclosing symbol.
    fn find_enclosing_symbol(&self, file_id: FileId, from_range: Range) -> Option<SymbolId> {
        let symbols = self.symbols_by_file.get(&file_id)?;

        let mut best: Option<(SymbolId, u32)> = None; // (id, range_size)

        for (sym_range, sym_id) in symbols {
            // Check if sym_range contains from_range
            if sym_range.start_line <= from_range.start_line
                && sym_range.end_line >= from_range.end_line
            {
                let range_size = sym_range.end_line - sym_range.start_line;
                match best {
                    Some((_, best_size)) if range_size < best_size => {
                        // Smaller (more specific) enclosing symbol
                        best = Some((*sym_id, range_size));
                    }
                    None => {
                        best = Some((*sym_id, range_size));
                    }
                    _ => {}
                }
            }
        }

        best.map(|(id, _)| id)
    }
}

/// State for the collector.
struct CollectorState {
    file_counter: u32,
    symbol_counter: u32,
    caches: CollectorCaches,
    current_batch: IndexBatch,
    current_embed_batch: EmbeddingBatch,
    batch_size: usize,
    embed_batch_token_budget: usize,
    embed_batch_byte_budget: usize,
    current_embed_tokens: usize,
    current_embed_bytes: usize,
    /// Current file's language_id for embedding metadata
    current_language: Box<str>,
}

impl CollectorState {
    fn new(
        batch_size: usize,
        embed_batch_token_budget: usize,
        embed_batch_byte_budget: usize,
    ) -> Self {
        Self {
            file_counter: 0,
            symbol_counter: 0,
            caches: CollectorCaches::new(),
            current_batch: IndexBatch::new(),
            current_embed_batch: EmbeddingBatch::new(),
            batch_size,
            embed_batch_token_budget,
            embed_batch_byte_budget,
            current_embed_tokens: 0,
            current_embed_bytes: 0,
            current_language: "unknown".into(),
        }
    }

    fn next_file_id(&mut self) -> FileId {
        self.file_counter += 1;
        FileId::new(self.file_counter).expect("FileId overflow")
    }

    fn next_symbol_id(&mut self) -> SymbolId {
        self.symbol_counter += 1;
        SymbolId::new(self.symbol_counter).expect("SymbolId overflow")
    }

    fn should_flush_index(&self) -> bool {
        self.current_batch.symbol_count() >= self.batch_size
    }

    fn should_flush_embed(&self) -> bool {
        !self.current_embed_batch.is_empty()
            && (self.current_embed_tokens >= self.embed_batch_token_budget
                || self.current_embed_bytes >= self.embed_batch_byte_budget)
    }

    fn take_batch(&mut self) -> IndexBatch {
        std::mem::take(&mut self.current_batch)
    }

    fn take_embed_batch(&mut self) -> EmbeddingBatch {
        self.current_embed_tokens = 0;
        self.current_embed_bytes = 0;
        std::mem::take(&mut self.current_embed_batch)
    }
}

impl CollectStage {
    const DEFAULT_MAX_CHUNK_TOKENS: usize = 4_000;
    const DEFAULT_EMBED_BATCH_TOKEN_BUDGET: usize = 4_096;
    const DEFAULT_EMBED_BATCH_BYTE_BUDGET: usize = 2_000_000;

    fn embedding_chars_for_tokens(tokens: usize) -> usize {
        // Conservative upper bound for code token density (~2 chars/token).
        tokens.saturating_mul(2)
    }

    /// Create a new collect stage.
    /// Minimum batch size is 1 (for testing). Production default is 5000.
    pub fn new(batch_size: usize) -> Self {
        Self {
            batch_size: batch_size.max(1),
            max_embedding_chars: Self::embedding_chars_for_tokens(Self::DEFAULT_MAX_CHUNK_TOKENS),
            embed_batch_token_budget: Self::DEFAULT_EMBED_BATCH_TOKEN_BUDGET,
            embed_batch_byte_budget: Self::DEFAULT_EMBED_BATCH_BYTE_BUDGET,
            exclude_generated_for_semantic: true,
            semantic_symbol_kinds: None,
            start_file_counter: 0,
            start_symbol_counter: 0,
        }
    }

    /// Configure semantic embedding limits for bounded-memory operation.
    pub fn with_semantic_limits(
        mut self,
        max_chunk_tokens: usize,
        embed_batch_token_budget: usize,
        embed_batch_byte_budget: usize,
        exclude_generated_for_semantic: bool,
    ) -> Self {
        self.max_embedding_chars = Self::embedding_chars_for_tokens(max_chunk_tokens.max(256));
        self.embed_batch_token_budget = embed_batch_token_budget.max(512);
        self.embed_batch_byte_budget = embed_batch_byte_budget.max(64 * 1024);
        self.exclude_generated_for_semantic = exclude_generated_for_semantic;
        self
    }

    /// Configure allowed symbol kinds for semantic embedding candidates.
    ///
    /// Unknown kinds are ignored with a warning. If no valid kinds remain,
    /// semantic candidate filtering is disabled to avoid dropping all embeddings.
    pub fn with_semantic_symbol_kinds(mut self, symbol_kinds: &[String]) -> Self {
        if symbol_kinds.is_empty() {
            self.semantic_symbol_kinds = None;
            return self;
        }

        let mut parsed = HashSet::new();
        let mut unknown = Vec::new();
        for raw in symbol_kinds {
            match parse_semantic_symbol_kind(raw) {
                Some(kind) => {
                    parsed.insert(kind);
                }
                None => unknown.push(raw.clone()),
            }
        }

        if !unknown.is_empty() {
            tracing::warn!(
                target: "semantic",
                "Ignoring unknown semantic_symbol_kinds: {}",
                unknown.join(", ")
            );
        }

        self.semantic_symbol_kinds = if parsed.is_empty() {
            None
        } else {
            Some(parsed)
        };
        self
    }

    /// Set starting counters from existing index.
    ///
    /// Call this before `run()` to continue ID assignment from where the index left off.
    /// This is critical for multi-directory indexing to avoid ID collisions.
    pub fn with_start_counters(mut self, file_counter: u32, symbol_counter: u32) -> Self {
        self.start_file_counter = file_counter;
        self.start_symbol_counter = symbol_counter;
        self
    }

    /// Create with default batch size (5000 symbols).
    pub fn default_batch_size() -> Self {
        Self::new(5000)
    }

    /// Process a single parsed file (for single-file indexing).
    ///
    /// [PIPELINE API] Used by `Pipeline::index_file_single()` for watcher reindex.
    /// Assigns FileId and SymbolIds using the next available IDs from DocumentIndex.
    ///
    /// Returns (IndexBatch, `Vec<UnresolvedRelationship>`, EmbeddingBatch) for indexing.
    pub fn process_single(
        &self,
        parsed: ParsedFile,
        index: Arc<crate::storage::DocumentIndex>,
    ) -> PipelineResult<(IndexBatch, Vec<UnresolvedRelationship>, EmbeddingBatch)> {
        // Get next available IDs from the index
        let next_file_id = index.get_next_file_id()?;
        let next_symbol_id = index.get_next_symbol_id()?;

        let mut state = CollectorState::new(
            self.batch_size,
            self.embed_batch_token_budget,
            self.embed_batch_byte_budget,
        );
        // Set counters to continue from existing index
        state.file_counter = next_file_id.saturating_sub(1);
        state.symbol_counter = next_symbol_id.saturating_sub(1);

        // Process the file
        self.process_file(&mut state, parsed);

        // Extract relationships and embedding candidates from batch
        let unresolved = std::mem::take(&mut state.current_batch.unresolved_relationships);
        let embed_batch = state.take_embed_batch();

        Ok((state.current_batch, unresolved, embed_batch))
    }

    /// Run the collect stage.
    ///
    /// Returns (total_files, total_symbols, embedding_candidates, input_wait, output_wait).
    ///
    /// # Arguments
    /// * `receiver` - Channel receiving ParsedFile from PARSE stage
    /// * `index_sender` - Channel sending IndexBatch to INDEX stage
    /// * `embed_sender` - Optional channel sending EmbeddingBatch to EMBED stage (parallel)
    /// * `embed_total_callback` - Optional callback to report embed candidate counts for progress
    pub fn run(
        &self,
        receiver: Receiver<ParsedFile>,
        index_sender: Sender<IndexBatch>,
        embed_sender: Option<Sender<EmbeddingBatch>>,
        embed_total_callback: Option<EmbedTotalCallback>,
    ) -> PipelineResult<(u32, u32, u32, std::time::Duration, std::time::Duration)> {
        use std::time::{Duration, Instant};

        let mut state = CollectorState::new(
            self.batch_size,
            self.embed_batch_token_budget,
            self.embed_batch_byte_budget,
        );
        // Continue from existing index counters (critical for multi-directory indexing)
        state.file_counter = self.start_file_counter;
        state.symbol_counter = self.start_symbol_counter;

        let mut input_wait = Duration::ZERO;
        let mut output_wait = Duration::ZERO;
        let mut total_embed_candidates = 0u32;

        loop {
            // Track input wait (time blocked on recv)
            let recv_start = Instant::now();
            let parsed = match receiver.recv() {
                Ok(p) => p,
                Err(_) => break, // Channel closed
            };
            input_wait += recv_start.elapsed();

            self.process_file(&mut state, parsed);

            if state.should_flush_embed() {
                let embed_batch = state.take_embed_batch();
                let batch_candidates = embed_batch.len() as u32;
                total_embed_candidates += batch_candidates;

                let send_start = Instant::now();
                if let Some(ref tx) = embed_sender {
                    if !embed_batch.is_empty() {
                        if let Some(ref callback) = embed_total_callback {
                            callback(batch_candidates as u64);
                        }
                        let _ = tx.send(embed_batch);
                    }
                }
                output_wait += send_start.elapsed();
            }

            if state.should_flush_index() {
                let send_start = Instant::now();
                if index_sender.send(state.take_batch()).is_err() {
                    break; // Channel closed
                }
                output_wait += send_start.elapsed();
            }
        }

        // Flush remaining batches
        if !state.current_embed_batch.is_empty() {
            let send_start = Instant::now();
            let embed_batch = state.take_embed_batch();
            let batch_candidates = embed_batch.len() as u32;
            total_embed_candidates += batch_candidates;
            if let Some(ref tx) = embed_sender {
                if !embed_batch.is_empty() {
                    // Report candidate count for progress display
                    if let Some(ref callback) = embed_total_callback {
                        callback(batch_candidates as u64);
                    }
                    let _ = tx.send(embed_batch);
                }
            }
            output_wait += send_start.elapsed();
        }

        if !state.current_batch.is_empty() {
            let send_start = Instant::now();
            let _ = index_sender.send(state.take_batch());
            output_wait += send_start.elapsed();
        }

        Ok((
            state.file_counter,
            state.symbol_counter,
            total_embed_candidates,
            input_wait,
            output_wait,
        ))
    }

    /// Process a single parsed file.
    fn process_file(&self, state: &mut CollectorState, parsed: ParsedFile) {
        let file_id = state.next_file_id();
        let file_path: Box<str> = parsed.path.to_string_lossy().into();
        let file_key = SimpleSemanticSearch::stable_file_key(&parsed.path);

        // Set current language for embedding metadata
        state.current_language = parsed.language_id.as_str().into();

        // Cache path -> FileId for Phase 2 resolution (per decisions.md)
        state.caches.insert_file(parsed.path.clone(), file_id);

        // Register file
        let mtime = crate::indexing::file_info::get_file_mtime(&parsed.path).unwrap_or(0);

        // Track file hash for lazy embedding skip
        state
            .current_embed_batch
            .file_keys
            .push((file_id, file_key.clone().into_boxed_str()));
        state.current_embed_batch.file_hashes.push((
            file_key.clone().into_boxed_str(),
            parsed.content_hash.clone(),
        ));

        state
            .current_batch
            .file_registrations
            .push(FileRegistration {
                path: parsed.path.clone(),
                file_id,
                content_hash: parsed.content_hash,
                language_id: parsed.language_id,
                timestamp: get_utc_timestamp(),
                mtime,
            });

        // First pass: collect class members for enrichment (Katman 2)
        let class_members = build_class_member_map(&parsed.raw_symbols);

        // Process symbols
        for raw_sym in parsed.raw_symbols {
            let symbol_id = state.next_symbol_id();

            // Cache for relationship resolution
            let name: Arc<str> = raw_sym.name.as_ref().into();
            state
                .caches
                .insert(name.clone(), file_id, raw_sym.range, symbol_id);

            // Build enriched embedding text for all meaningful symbols
            if !should_skip_embedding(&raw_sym, self.semantic_symbol_kinds.as_ref()) {
                let embedding_text = build_embedding_text(
                    &raw_sym,
                    &parsed.path,
                    &class_members,
                    self.max_embedding_chars,
                );
                if !embedding_text.trim().is_empty()
                    && !should_skip_generated_or_minified_semantic(
                        &parsed.path,
                        &embedding_text,
                        self.exclude_generated_for_semantic,
                    )
                {
                    let text_bytes = embedding_text.len();
                    let estimated_tokens = estimate_tokens_from_bytes(text_bytes);
                    state.current_embed_batch.candidates.push((
                        file_id,
                        symbol_id,
                        embedding_text.into(),
                        state.current_language.clone(),
                    ));
                    state.current_embed_bytes += text_bytes;
                    state.current_embed_tokens += estimated_tokens;
                }
            }

            // Create Symbol
            let symbol = create_symbol(
                symbol_id,
                &raw_sym,
                file_id,
                file_path.clone(),
                parsed.module_path.as_deref(),
                parsed.language_id,
            );

            state
                .current_batch
                .symbols
                .push((symbol, parsed.path.clone()));
        }

        // Process imports
        for raw_import in parsed.raw_imports {
            let import = raw_import.into_import(file_id);
            state.current_batch.imports.push(import);
        }

        // Process relationships
        for raw_rel in parsed.raw_relationships {
            let unresolved = create_unresolved_relationship(&state.caches, raw_rel, file_id);
            state
                .current_batch
                .unresolved_relationships
                .push(unresolved);
        }
    }
}

fn estimate_tokens_from_bytes(bytes: usize) -> usize {
    // Conservative approximation for code: ~2 chars per token.
    // Keeps memory bounded without collapsing throughput as harshly as 1-byte=1-token.
    bytes.saturating_add(1).saturating_div(2).max(1)
}

fn parse_semantic_symbol_kind(raw: &str) -> Option<SymbolKind> {
    let normalized = raw.trim().to_ascii_lowercase().replace('-', "_");
    match normalized.as_str() {
        "function" => Some(SymbolKind::Function),
        "method" => Some(SymbolKind::Method),
        "struct" => Some(SymbolKind::Struct),
        "enum" => Some(SymbolKind::Enum),
        "trait" => Some(SymbolKind::Trait),
        "interface" => Some(SymbolKind::Interface),
        "class" => Some(SymbolKind::Class),
        "module" => Some(SymbolKind::Module),
        "variable" => Some(SymbolKind::Variable),
        "constant" => Some(SymbolKind::Constant),
        "field" => Some(SymbolKind::Field),
        "parameter" => Some(SymbolKind::Parameter),
        "type_alias" | "typealias" | "type" => Some(SymbolKind::TypeAlias),
        "macro" => Some(SymbolKind::Macro),
        _ => None,
    }
}

fn should_skip_generated_or_minified_semantic(
    path: &Path,
    embedding_text: &str,
    exclude_generated_for_semantic: bool,
) -> bool {
    if !exclude_generated_for_semantic {
        return false;
    }

    if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
        if file_name.ends_with(".min.js")
            || file_name.ends_with(".bundle.js")
            || file_name.ends_with(".generated.js")
            || file_name.ends_with(".generated.ts")
            || file_name.ends_with(".gen.js")
            || file_name.ends_with(".gen.ts")
        {
            return true;
        }
    }

    let normalized_path = path.to_string_lossy().replace('\\', "/");
    if normalized_path.contains("/src/gen/")
        || normalized_path.contains("/src/v2/gen/")
        || normalized_path.contains("/src/v3/gen/")
    {
        return true;
    }

    // Fast directory-based exclusion for generated/vendor artifacts.
    if path.components().any(|c| {
        matches!(
            c.as_os_str().to_string_lossy().as_ref(),
            "node_modules" | "vendor" | "dist" | "out" | "build" | ".next" | ".nuxt"
        )
    }) {
        return true;
    }

    // Heuristic for minified content: huge text with very low newline density.
    embedding_text.len() > 8_000 && embedding_text.lines().take(3).count() <= 1
}

/// Build a map of class/struct/interface/trait names to their member signatures.
///
/// Used for Katman 2 enrichment: class symbols get their members' signatures
/// appended to the embedding text for better semantic matching.
fn build_class_member_map(symbols: &[RawSymbol]) -> HashMap<CompactString, Vec<Box<str>>> {
    let mut map: HashMap<CompactString, Vec<Box<str>>> = HashMap::new();
    for sym in symbols {
        if let Some(ScopeContext::ClassMember {
            class_name: Some(ref cn),
        }) = sym.scope_context
        {
            if let Some(ref sig) = sym.signature {
                map.entry(cn.clone()).or_default().push(sig.clone());
            }
        }
    }
    map
}

/// Determine if a symbol should be skipped for embedding.
fn should_skip_embedding(sym: &RawSymbol, allowed_kinds: Option<&HashSet<SymbolKind>>) -> bool {
    if let Some(kinds) = allowed_kinds {
        if !kinds.contains(&sym.kind) {
            return true;
        }
    }

    // Parameters are too granular for useful embeddings
    if sym.kind == SymbolKind::Parameter {
        return true;
    }
    // Fields without doc, signature, or body provide minimal value
    if sym.kind == SymbolKind::Field
        && sym.doc_comment.is_none()
        && sym.signature.is_none()
        && sym.body.is_none()
    {
        return true;
    }
    false
}

/// Build enriched embedding text for a symbol.
///
/// Format:
/// ```text
/// # file/path.rs
/// # Scope: ClassName       (if class member)
/// # function fn_name(args) (kind + sig or name)
/// # Methods: sig1, sig2    (if class/struct/interface/trait with members)
/// doc comment text
/// signature
/// body (truncated to configured token budget)
/// ```
fn build_embedding_text(
    sym: &RawSymbol,
    file_path: &Path,
    class_member_sigs: &HashMap<CompactString, Vec<Box<str>>>,
    max_embedding_chars: usize,
) -> String {
    let mut text = String::with_capacity(512);

    // Context header: file path
    text.push_str("# ");
    text.push_str(&file_path.display().to_string());
    text.push('\n');

    // Scope info (class membership)
    if let Some(ScopeContext::ClassMember {
        class_name: Some(ref cn),
    }) = sym.scope_context
    {
        text.push_str("# Scope: ");
        text.push_str(cn.as_ref());
        text.push('\n');
    }

    // Kind + signature/name header
    let kind_str = symbol_kind_label(sym.kind);
    if let Some(ref sig) = sym.signature {
        text.push_str("# ");
        text.push_str(kind_str);
        text.push(' ');
        text.push_str(sig);
        text.push('\n');
    } else {
        text.push_str("# ");
        text.push_str(kind_str);
        text.push(' ');
        text.push_str(sym.name.as_ref());
        text.push('\n');
    }

    // Class/Struct/Interface/Trait enrichment: append member signatures
    if matches!(
        sym.kind,
        SymbolKind::Class | SymbolKind::Struct | SymbolKind::Interface | SymbolKind::Trait
    ) {
        if let Some(member_sigs) = class_member_sigs.get(&sym.name) {
            if !member_sigs.is_empty() {
                text.push_str("# Methods: ");
                let joined: String = member_sigs
                    .iter()
                    .map(|s| s.as_ref())
                    .collect::<Vec<&str>>()
                    .join(", ");
                text.push_str(&joined);
                text.push('\n');
            }
        }
    }

    // Doc comment
    if let Some(ref doc) = sym.doc_comment {
        text.push_str(doc);
        text.push('\n');
    }

    // Signature (as searchable code text, separate from header)
    if let Some(ref sig) = sym.signature {
        text.push_str(sig);
        text.push('\n');
    }

    // Body (truncated to fit within token budget)
    if let Some(ref body) = sym.body {
        let remaining = max_embedding_chars.saturating_sub(text.len());
        if remaining > 0 {
            if body.len() <= remaining {
                text.push_str(body);
            } else {
                // UTF-8 safe truncation
                let truncation_point = body
                    .char_indices()
                    .take_while(|(i, _)| *i < remaining)
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(0);
                text.push_str(&body[..truncation_point]);
                text.push_str("\n// ... truncated");
            }
        }
    }

    text
}

/// Human-readable label for a SymbolKind, used in embedding context headers.
fn symbol_kind_label(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
        SymbolKind::Struct => "struct",
        SymbolKind::Enum => "enum",
        SymbolKind::Trait => "trait",
        SymbolKind::Interface => "interface",
        SymbolKind::Class => "class",
        SymbolKind::Module => "module",
        SymbolKind::Variable => "variable",
        SymbolKind::Constant => "constant",
        SymbolKind::Field => "field",
        SymbolKind::Parameter => "parameter",
        SymbolKind::TypeAlias => "type",
        SymbolKind::Macro => "macro",
    }
}

/// Create a Symbol from RawSymbol.
fn create_symbol(
    id: SymbolId,
    raw: &RawSymbol,
    file_id: FileId,
    file_path: Box<str>,
    module_path: Option<&str>,
    language_id: crate::parsing::LanguageId,
) -> Symbol {
    let mut symbol = Symbol::new(id, raw.name.clone(), raw.kind, file_id, raw.range)
        .with_file_path(file_path)
        .with_visibility(raw.visibility)
        .with_language_id(language_id);

    if let Some(sig) = &raw.signature {
        symbol = symbol.with_signature(sig.clone());
    }
    if let Some(doc) = &raw.doc_comment {
        symbol = symbol.with_doc(doc.clone());
    }
    if let Some(path) = module_path {
        symbol = symbol.with_module_path(path);
    }
    if let Some(scope) = raw.scope_context.clone() {
        symbol = symbol.with_scope(scope);
    }

    symbol
}

/// Create an UnresolvedRelationship from RawRelationship.
/// Uses Arc references for zero-copy lookup.
fn create_unresolved_relationship(
    caches: &CollectorCaches,
    raw: RawRelationship,
    file_id: FileId,
) -> UnresolvedRelationship {
    // Try to resolve from_id using the cache:
    // 1. Exact match by (name, file_id, range)
    // 2. Fallback by (name, file_id) when range doesn't match
    // 3. Range containment: find the smallest indexed symbol enclosing from_range
    let from_id = caches
        .lookup(&raw.from_name, file_id, raw.from_range)
        .or_else(|| caches.lookup_by_name_in_file(&raw.from_name, file_id))
        .or_else(|| caches.find_enclosing_symbol(file_id, raw.from_range));

    UnresolvedRelationship {
        from_id,
        from_name: raw.from_name,
        to_name: raw.to_name,
        file_id,
        kind: raw.kind,
        metadata: raw.metadata,
        to_range: Some(raw.to_range),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::pipeline::types::RawImport;
    use crate::parsing::LanguageId;
    use crate::{RelationKind, SymbolKind};
    use crossbeam_channel::bounded;

    fn make_raw_symbol(name: &str, kind: SymbolKind, line: u32) -> RawSymbol {
        RawSymbol::new(name, kind, Range::new(line, 0, line, 10))
    }

    fn make_parsed_file(name: &str, symbols: Vec<RawSymbol>) -> ParsedFile {
        ParsedFile {
            path: PathBuf::from(name),
            content_hash: "abc123def456".to_string(),
            language_id: LanguageId::new("rust"),
            module_path: None,
            raw_symbols: symbols,
            raw_imports: Vec::new(),
            raw_relationships: Vec::new(),
        }
    }

    #[test]
    fn test_collect_assigns_sequential_ids() {
        let (parsed_tx, parsed_rx) = bounded(100);
        let (batch_tx, batch_rx) = bounded(100);

        // Send 2 files with 3 symbols total
        parsed_tx
            .send(make_parsed_file(
                "file1.rs",
                vec![
                    make_raw_symbol("foo", SymbolKind::Function, 1),
                    make_raw_symbol("bar", SymbolKind::Function, 2),
                ],
            ))
            .unwrap();
        parsed_tx
            .send(make_parsed_file(
                "file2.rs",
                vec![make_raw_symbol("baz", SymbolKind::Struct, 1)],
            ))
            .unwrap();
        drop(parsed_tx);

        let stage = CollectStage::new(100);
        let result = stage.run(parsed_rx, batch_tx, None, None);

        assert!(result.is_ok());
        let (files, symbols, _, _, _) = result.unwrap();

        let batches: Vec<_> = batch_rx.iter().collect();

        println!("Processed {files} files, {symbols} symbols");
        for batch in &batches {
            for (sym, path) in &batch.symbols {
                println!(
                    "  - {} (id={}, file_id={}) in {}",
                    sym.name,
                    sym.id.value(),
                    sym.file_id.value(),
                    path.display()
                );
            }
        }

        assert_eq!(files, 2, "Should process 2 files");
        assert_eq!(symbols, 3, "Should process 3 symbols");

        // Collect all symbol IDs
        let all_ids: Vec<u32> = batches
            .iter()
            .flat_map(|b| b.symbols.iter().map(|(s, _)| s.id.value()))
            .collect();

        assert_eq!(all_ids, vec![1, 2, 3], "IDs should be sequential 1, 2, 3");
    }

    #[test]
    fn test_collect_batches_by_symbol_count() {
        let (parsed_tx, parsed_rx) = bounded(100);
        let (batch_tx, batch_rx) = bounded(100);

        // Create files with many symbols to trigger batching
        for i in 0..5 {
            let symbols: Vec<_> = (0..10)
                .map(|j| make_raw_symbol(&format!("sym_{i}_{j}"), SymbolKind::Function, j as u32))
                .collect();
            parsed_tx
                .send(make_parsed_file(&format!("file{i}.rs"), symbols))
                .unwrap();
        }
        drop(parsed_tx);

        // Small batch size to force multiple batches
        let stage = CollectStage::new(15);
        let result = stage.run(parsed_rx, batch_tx, None, None);

        assert!(result.is_ok());
        let (files, symbols, _, _, _) = result.unwrap();

        let batches: Vec<_> = batch_rx.iter().collect();

        println!(
            "Processed {files} files, {symbols} symbols in {} batches",
            batches.len()
        );
        for (i, batch) in batches.iter().enumerate() {
            println!("  Batch {i}: {} symbols", batch.symbol_count());
        }

        assert_eq!(files, 5);
        assert_eq!(symbols, 50);
        assert!(batches.len() > 1, "Should create multiple batches");
    }

    #[test]
    fn test_collect_resolves_relationship_from_id() {
        let (parsed_tx, parsed_rx) = bounded(100);
        let (batch_tx, batch_rx) = bounded(100);

        // Create file with symbols and relationships
        let mut parsed = make_parsed_file(
            "test.rs",
            vec![
                make_raw_symbol("caller", SymbolKind::Function, 1),
                make_raw_symbol("callee", SymbolKind::Function, 5),
            ],
        );

        // Add relationship: caller -> callee
        parsed.raw_relationships.push(RawRelationship::new(
            "caller",
            Range::new(1, 0, 1, 10), // from_range = caller's definition
            "callee",
            Range::new(2, 4, 2, 12), // to_range = call site
            RelationKind::Calls,
        ));

        parsed_tx.send(parsed).unwrap();
        drop(parsed_tx);

        let stage = CollectStage::new(100);
        let _ = stage.run(parsed_rx, batch_tx, None, None);

        let batches: Vec<_> = batch_rx.iter().collect();

        println!("Relationships:");
        for batch in &batches {
            for rel in &batch.unresolved_relationships {
                println!(
                    "  {} (from_id={:?}) -> {} ({:?})",
                    rel.from_name, rel.from_id, rel.to_name, rel.kind
                );
            }
        }

        // Check that from_id was resolved
        let rel = &batches[0].unresolved_relationships[0];
        assert!(rel.from_id.is_some(), "from_id should be resolved");
        assert_eq!(rel.from_id.unwrap().value(), 1, "caller should have id=1");
        assert_eq!(rel.from_name.as_ref(), "caller");
        assert_eq!(rel.to_name.as_ref(), "callee");
    }

    #[test]
    fn test_collect_converts_imports() {
        let (parsed_tx, parsed_rx) = bounded(100);
        let (batch_tx, batch_rx) = bounded(100);

        let mut parsed = make_parsed_file("test.rs", vec![]);
        parsed
            .raw_imports
            .push(RawImport::new("std::collections::HashMap"));
        parsed
            .raw_imports
            .push(RawImport::new("crate::module::Thing").with_alias("Thing".to_string()));

        parsed_tx.send(parsed).unwrap();
        drop(parsed_tx);

        let stage = CollectStage::new(100);
        let _ = stage.run(parsed_rx, batch_tx, None, None);

        let batches: Vec<_> = batch_rx.iter().collect();

        println!("Imports:");
        for batch in &batches {
            for imp in &batch.imports {
                println!(
                    "  {} (file_id={}, alias={:?})",
                    imp.path,
                    imp.file_id.value(),
                    imp.alias
                );
            }
        }

        assert_eq!(batches[0].imports.len(), 2);

        let imp1 = &batches[0].imports[0];
        assert_eq!(imp1.path, "std::collections::HashMap");
        assert_eq!(imp1.file_id.value(), 1);

        let imp2 = &batches[0].imports[1];
        assert_eq!(imp2.alias.as_deref(), Some("Thing"));
    }

    #[test]
    fn test_collect_tracks_file_registrations() {
        let (parsed_tx, parsed_rx) = bounded(100);
        let (batch_tx, batch_rx) = bounded(100);

        // Send 3 files
        parsed_tx
            .send(make_parsed_file(
                "src/main.rs",
                vec![make_raw_symbol("main", SymbolKind::Function, 1)],
            ))
            .unwrap();
        parsed_tx
            .send(make_parsed_file(
                "src/lib.rs",
                vec![make_raw_symbol("lib_fn", SymbolKind::Function, 1)],
            ))
            .unwrap();
        parsed_tx
            .send(make_parsed_file(
                "src/utils.rs",
                vec![make_raw_symbol("helper", SymbolKind::Function, 1)],
            ))
            .unwrap();
        drop(parsed_tx);

        let stage = CollectStage::new(100);
        let _ = stage.run(parsed_rx, batch_tx, None, None);

        let batches: Vec<_> = batch_rx.iter().collect();
        let registrations = &batches[0].file_registrations;

        println!("File registrations:");
        for reg in registrations {
            println!(
                "  {} -> file_id={}, hash={}, timestamp={}",
                reg.path.display(),
                reg.file_id.value(),
                reg.content_hash,
                reg.timestamp
            );
        }

        assert_eq!(registrations.len(), 3, "Should register 3 files");

        // Verify sequential FileIds
        assert_eq!(registrations[0].file_id.value(), 1);
        assert_eq!(registrations[0].path, PathBuf::from("src/main.rs"));

        assert_eq!(registrations[1].file_id.value(), 2);
        assert_eq!(registrations[1].path, PathBuf::from("src/lib.rs"));

        assert_eq!(registrations[2].file_id.value(), 3);
        assert_eq!(registrations[2].path, PathBuf::from("src/utils.rs"));

        // Verify timestamps are set (after 2020-01-01)
        for reg in registrations {
            assert!(reg.timestamp > 1577836800, "Timestamp should be after 2020");
        }
    }

    #[test]
    fn test_collect_preserves_doc_comment() {
        let (parsed_tx, parsed_rx) = bounded(100);
        let (batch_tx, batch_rx) = bounded(100);

        // Create symbols with doc_comments
        let sym_with_doc = RawSymbol::new(
            "documented_fn",
            SymbolKind::Function,
            Range::new(1, 0, 5, 1),
        )
        .with_signature("fn documented_fn() -> Result<()>")
        .with_doc_comment("This function does important work.\n\nIt handles the core logic.");

        let sym_without_doc =
            RawSymbol::new("plain_fn", SymbolKind::Function, Range::new(10, 0, 12, 1))
                .with_signature("fn plain_fn()");

        let sym_with_short_doc =
            RawSymbol::new("helper", SymbolKind::Function, Range::new(20, 0, 22, 1))
                .with_doc_comment("Helper utility");

        let parsed = ParsedFile {
            path: PathBuf::from("src/lib.rs"),
            content_hash: "abc123def456".to_string(),
            language_id: LanguageId::new("rust"),
            module_path: Some("mylib".to_string()),
            raw_symbols: vec![sym_with_doc, sym_without_doc, sym_with_short_doc],
            raw_imports: Vec::new(),
            raw_relationships: Vec::new(),
        };

        parsed_tx.send(parsed).unwrap();
        drop(parsed_tx);

        let stage = CollectStage::new(100);
        let result = stage.run(parsed_rx, batch_tx, None, None);
        assert!(result.is_ok());

        let batches: Vec<_> = batch_rx.iter().collect();
        assert_eq!(batches.len(), 1);

        let symbols = &batches[0].symbols;
        assert_eq!(symbols.len(), 3);

        // Verify doc_comment is preserved on Symbol
        let (sym1, _) = &symbols[0];
        assert_eq!(sym1.name.as_ref(), "documented_fn");
        assert!(
            sym1.doc_comment.is_some(),
            "doc_comment should be preserved"
        );
        assert_eq!(
            sym1.doc_comment.as_deref(),
            Some("This function does important work.\n\nIt handles the core logic.")
        );

        let (sym2, _) = &symbols[1];
        assert_eq!(sym2.name.as_ref(), "plain_fn");
        assert!(
            sym2.doc_comment.is_none(),
            "Symbol without doc should have None"
        );

        let (sym3, _) = &symbols[2];
        assert_eq!(sym3.name.as_ref(), "helper");
        assert_eq!(sym3.doc_comment.as_deref(), Some("Helper utility"));

        println!("doc_comment preservation verified:");
        for (sym, path) in symbols {
            println!(
                "  {} (id={}) in {} doc={:?}",
                sym.name,
                sym.id.value(),
                path.display(),
                sym.doc_comment.as_ref().map(|d| if d.len() > 30 {
                    format!("{}...", &d[..30])
                } else {
                    d.to_string()
                })
            );
        }
    }
}
