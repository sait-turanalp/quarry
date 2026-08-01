//! IndexFacade - Bridge component wrapping DocumentIndex + Pipeline + SemanticSearch
//!
//! Provides a unified API that matches SimpleIndexer's interface while using Pipeline
//! for indexing and DocumentIndex for queries. This enables gradual migration from
//! SimpleIndexer to the parallel Pipeline architecture.
//!
//! ## Architecture
//!
//! ```text
//! IndexFacade
//!   ├── DocumentIndex (Arc) - All query operations
//!   ├── Pipeline - All mutation/indexing operations
//!   ├── SimpleSemanticSearch (Option<Arc<Mutex>>) - Semantic search
//!   ├── SymbolCache (Option<Arc>) - O(1) symbol lookups
//!   └── indexed_paths (HashSet) - Directory tracking
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! let facade = IndexFacade::new(settings)?;
//! facade.index_directory(&path)?;  // Uses Pipeline
//! let symbols = facade.find_symbols_by_name("main")?;  // Uses DocumentIndex
//! ```

use crate::chunks::{
    ActiveRecallBackend, ChunkSearchBackend, ChunkSearchResult, ChunkVectorBackend,
    CodeChunkIndexer, CodeChunkRecord, build_rerank_text as build_chunk_rerank_text,
};
use crate::config::{SemanticBackend, Settings};
use crate::indexing::pipeline::Pipeline;
use crate::semantic::{EmbeddingPool, SimpleSemanticSearch, SymbolVectorBackend};
use crate::storage::{DocumentIndex, SearchResult};
use crate::symbol::context::{ContextIncludes, SymbolContext, SymbolRelationships};
use crate::vector::EmbeddingRuntimeConfig;
use crate::{FileId, IndexError, RelationKind, Relationship, Symbol, SymbolId, SymbolKind};
use glob::Pattern;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Result type for facade operations
pub type FacadeResult<T> = Result<T, IndexError>;

/// Statistics for indexing operations
#[derive(Debug, Clone, Default)]
pub struct IndexingStats {
    pub files_indexed: usize,
    pub symbols_found: usize,
    pub relationships_resolved: usize,
}

/// Statistics for sync operations
#[derive(Debug, Clone, Default)]
pub struct SyncStats {
    pub added_dirs: usize,
    pub removed_dirs: usize,
    pub files_indexed: usize,
    pub symbols_found: usize,
    pub files_modified: usize,
    pub files_added: usize,
}

impl SyncStats {
    pub fn has_changes(&self) -> bool {
        self.added_dirs > 0
            || self.removed_dirs > 0
            || self.files_modified > 0
            || self.files_added > 0
    }
}

#[derive(Debug, Clone, Default)]
struct ChunkRefreshDelta {
    changed_files: Vec<PathBuf>,
    deleted_files: Vec<PathBuf>,
    force_full: bool,
}

/// Per-stage latency breakdown from chunk search.
#[derive(Debug, Default, Clone)]
pub struct SearchTimingMs {
    pub bm25: u64,
    pub vector: u64,
    pub rrf: u64,
    pub rerank: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ChunkSearchOutcome {
    pub results: Vec<ChunkSearchResult>,
    pub weak_count: usize,
    pub pruned_by: Vec<String>,
    pub bm25_only_fallback: bool,
    pub timing: SearchTimingMs,
}

enum RerankExecution {
    Completed(Vec<(usize, f32)>),
    Failed,
    TimedOut,
    SkippedBusy,
}

/// IndexFacade - Unified interface for code intelligence operations
///
/// This facade wraps DocumentIndex (for queries) and Pipeline (for indexing),
/// providing an API compatible with SimpleIndexer for gradual migration.
pub struct IndexFacade {
    /// Document storage (Tantivy-based) - used for all queries
    document_index: Arc<DocumentIndex>,

    /// Parallel indexing pipeline - used for mutations
    pipeline: Pipeline,

    /// Optional semantic search for doc comment embeddings (write path only).
    /// Loaded lazily on first write via `ensure_semantic_for_write()`.
    semantic_search: Option<Arc<Mutex<SimpleSemanticSearch>>>,

    /// True when semantic data exists on disk (set by `load_semantic_search`).
    semantic_data_available: bool,

    /// Optional embedding pool for parallel embedding generation
    embedding_pool: Option<Arc<EmbeddingPool>>,

    /// Lazy-initialized symbol vector search (binary pre-filter + mmap random access).
    symbol_vector_backend: std::sync::OnceLock<Option<SymbolVectorBackend>>,

    /// Lazy-initialized cached chunk Tantivy reader (BM25 + stored-field lookup).
    chunk_backend: std::sync::OnceLock<Option<ChunkSearchBackend>>,

    /// Lazy-initialized cached chunk vector search (binary pre-filter + mmap random access).
    chunk_vector_backend: std::sync::OnceLock<Option<ChunkVectorBackend>>,

    /// Lazy-initialized cross-encoder reranker
    reranker: std::sync::OnceLock<Option<Arc<crate::reranking::Reranker>>>,

    /// Prevent reranker task pile-up when timeouts happen.
    ///
    /// If a rerank task is already running, new rerank attempts fall back to
    /// fused retrieval results instead of spawning more CPU-heavy work.
    rerank_inflight: Arc<AtomicBool>,

    /// Configuration
    settings: Arc<Settings>,

    /// Tracked indexed directories (canonicalized paths)
    indexed_paths: HashSet<PathBuf>,

    /// Base path for index storage
    index_base: PathBuf,
}

impl IndexFacade {
    fn effective_semantic_backend(&self, model: &str) -> SemanticBackend {
        let configured = self.settings.semantic_search.backend;
        let looks_model2vec = SimpleSemanticSearch::looks_like_model2vec_model(model);

        match (configured, looks_model2vec) {
            (SemanticBackend::Fastembed, true) => {
                tracing::warn!(
                    target: "semantic",
                    "semantic_search.backend=fastembed but model '{}' looks like model2vec; switching backend to model2vec",
                    model
                );
                SemanticBackend::Model2vec
            }
            (SemanticBackend::Model2vec, false) => {
                tracing::warn!(
                    target: "semantic",
                    "semantic_search.backend=model2vec but model '{}' is not model2vec; switching backend to fastembed",
                    model
                );
                SemanticBackend::Fastembed
            }
            (backend, _) => backend,
        }
    }

    /// Compute effective semantic runtime limits.
    ///
    /// Granite has a large context window, so we enforce bounded-memory defaults
    /// at runtime even when a workspace has older aggressive settings.
    fn effective_semantic_pool_config(
        &self,
        model: &str,
        backend: SemanticBackend,
    ) -> (usize, usize, usize) {
        let mut pool_size = self.settings.semantic_search.embedding_threads.max(1);
        let mut max_batch_tokens = self
            .settings
            .semantic_search
            .embed_batch_token_budget
            .max(512);
        let mut max_sequence_length = self.settings.semantic_search.max_chunk_tokens.max(256);

        if backend == SemanticBackend::Model2vec {
            if pool_size != 1 {
                tracing::info!(
                    target: "semantic",
                    "Model2Vec runtime clamp: embedding_threads={} -> 1",
                    pool_size
                );
                pool_size = 1;
            }
            let clamped_seq = max_sequence_length.clamp(128, 4_096);
            if clamped_seq != max_sequence_length {
                tracing::info!(
                    target: "semantic",
                    "Model2Vec runtime clamp: max_chunk_tokens={} -> {}",
                    max_sequence_length,
                    clamped_seq
                );
                max_sequence_length = clamped_seq;
            }
        }

        if model == "GraniteSmallEnglishR2" {
            if pool_size != 1 {
                tracing::warn!(
                    target: "semantic",
                    "Granite runtime clamp: embedding_threads={} -> 1 for bounded memory",
                    pool_size
                );
                pool_size = 1;
            }

            let clamped_seq = max_sequence_length.clamp(512, 4_000);
            if clamped_seq != max_sequence_length {
                tracing::warn!(
                    target: "semantic",
                    "Granite runtime clamp: max_chunk_tokens={} -> {}",
                    max_sequence_length,
                    clamped_seq
                );
                max_sequence_length = clamped_seq;
            }

            let clamped_budget = max_batch_tokens.clamp(1_024, 4_096);
            if clamped_budget != max_batch_tokens {
                tracing::warn!(
                    target: "semantic",
                    "Granite runtime clamp: embed_batch_token_budget={} -> {}",
                    max_batch_tokens,
                    clamped_budget
                );
                max_batch_tokens = clamped_budget;
            }
        }

        (pool_size, max_batch_tokens, max_sequence_length)
    }

    fn refresh_code_chunk_index(&self, delta: Option<&ChunkRefreshDelta>) -> FacadeResult<()> {
        if !self.settings.chunk_search.enabled {
            return Ok(());
        }

        let use_incremental = self.settings.indexing.chunk_incremental_rebuild_enabled
            && delta.is_some_and(|d| !d.force_full);
        let changed_files = delta.map_or(&[][..], |d| d.changed_files.as_slice());
        let deleted_files = delta.map_or(&[][..], |d| d.deleted_files.as_slice());
        if use_incremental && changed_files.is_empty() && deleted_files.is_empty() {
            return Ok(());
        }

        let symbol_count = self.document_index.count_symbols().unwrap_or(0);
        if !use_incremental && symbol_count == 0 {
            return Ok(());
        }
        let symbols = if symbol_count == 0 {
            Vec::new()
        } else {
            self.document_index.get_all_symbols(symbol_count)?
        };
        if !use_incremental && symbols.is_empty() {
            return Ok(());
        }
        let workspace_root = self
            .settings
            .workspace_root
            .clone()
            .or_else(|| std::env::current_dir().ok());
        let indexed_paths = self
            .document_index
            .get_all_indexed_paths()
            .unwrap_or_default();

        let model = self
            .semantic_search
            .as_ref()
            .and_then(|s| {
                s.lock()
                    .ok()
                    .and_then(|sem| sem.metadata().map(|m| m.model_name.clone()))
            })
            .unwrap_or_else(|| self.settings.semantic_search.model.clone());
        let backend = self.effective_semantic_backend(&model);
        let runtime = EmbeddingRuntimeConfig::from_semantic_settings(
            &self.settings.semantic_search,
            self.settings.workspace_root.as_deref(),
        );
        let recall_backend = ActiveRecallBackend::new(
            &model,
            backend,
            runtime,
            self.settings.semantic_search.max_chunk_tokens.max(128),
        )?;

        let chunk_indexer = CodeChunkIndexer::new(&self.index_base);
        let stats = if use_incremental {
            chunk_indexer.rebuild_incremental_from_symbols(
                &symbols,
                &recall_backend,
                workspace_root.as_deref(),
                Some(self.settings.as_ref()),
                self.settings.chunk_search.max_snippet_chars,
                self.settings.chunk_search.snippet_context_lines,
                self.settings.chunk_search.snippet_min_lines,
                self.settings.chunk_search.flow_chunk_enabled,
                &self.settings.chunk_search.flow_chunk_languages,
                self.settings.chunk_search.flow_chunk_max_per_symbol,
                self.settings.chunk_search.chunk_token_target,
                self.settings.chunk_search.chunk_token_max,
                self.settings.chunk_search.chunk_token_overlap,
                self.settings.chunk_search.embedding_dimension,
                changed_files,
                deleted_files,
                self.settings.chunk_search.rebuild_logging_verbose,
            )?
        } else {
            chunk_indexer.rebuild_from_symbols(
                &symbols,
                &recall_backend,
                workspace_root.as_deref(),
                Some(self.settings.as_ref()),
                self.settings.chunk_search.max_snippet_chars,
                self.settings.chunk_search.snippet_context_lines,
                self.settings.chunk_search.snippet_min_lines,
                Some(indexed_paths.as_slice()),
                self.settings.chunk_search.flow_chunk_enabled,
                &self.settings.chunk_search.flow_chunk_languages,
                self.settings.chunk_search.flow_chunk_max_per_symbol,
                self.settings.chunk_search.chunk_token_target,
                self.settings.chunk_search.chunk_token_max,
                self.settings.chunk_search.chunk_token_overlap,
                self.settings.chunk_search.embedding_dimension,
            )?
        };
        tracing::info!(
            target: "chunk_search",
            "code chunk index rebuilt (mode={}): chunks={}, embeddings={}",
            if use_incremental { "incremental" } else { "full" },
            stats.chunks_indexed,
            stats.embeddings_indexed
        );
        Ok(())
    }

    /// Build chunk rebuild config for parallel execution inside pipeline.
    /// Returns None if chunk search is disabled or no embedding pool is available.
    fn build_chunk_config(&self) -> Option<crate::indexing::pipeline::ChunkRebuildConfig> {
        if !self.settings.chunk_search.enabled {
            return None;
        }
        self.embedding_pool
            .as_ref()
            .map(|pool| crate::indexing::pipeline::ChunkRebuildConfig {
                index_base: self.index_base.clone(),
                settings: Arc::clone(&self.settings),
                workspace_root: self
                    .settings
                    .workspace_root
                    .clone()
                    .or_else(|| std::env::current_dir().ok()),
                embedding_pool: Arc::clone(pool),
                indexed_paths: self
                    .document_index
                    .get_all_indexed_paths()
                    .unwrap_or_default(),
            })
    }

    /// Create a new IndexFacade with the given settings.
    ///
    /// Creates or opens the DocumentIndex and initializes the Pipeline.
    pub fn new(settings: Arc<Settings>) -> FacadeResult<Self> {
        // Construct the full index path
        let index_base = if let Some(ref workspace_root) = settings.workspace_root {
            workspace_root.join(&settings.index_path)
        } else {
            settings.index_path.clone()
        };

        // Tantivy data goes under index_path/tantivy
        let tantivy_path = index_base.join("tantivy");

        let document_index = Arc::new(DocumentIndex::new(&tantivy_path, &settings)?);

        let pipeline = Pipeline::with_settings(settings.clone());

        Ok(Self {
            document_index,
            pipeline,
            semantic_search: None,
            semantic_data_available: false,
            embedding_pool: None,
            symbol_vector_backend: std::sync::OnceLock::new(),
            chunk_backend: std::sync::OnceLock::new(),
            chunk_vector_backend: std::sync::OnceLock::new(),
            reranker: std::sync::OnceLock::new(),
            rerank_inflight: Arc::new(AtomicBool::new(false)),
            settings,
            indexed_paths: HashSet::new(),
            index_base,
        })
    }

    /// Create facade from existing components (for server integration).
    pub fn from_components(
        document_index: Arc<DocumentIndex>,
        pipeline: Pipeline,
        semantic_search: Option<Arc<Mutex<SimpleSemanticSearch>>>,
        settings: Arc<Settings>,
    ) -> Self {
        let index_base = if let Some(ref workspace_root) = settings.workspace_root {
            workspace_root.join(&settings.index_path)
        } else {
            settings.index_path.clone()
        };

        let semantic_data_available = semantic_search.is_some();
        Self {
            document_index,
            pipeline,
            semantic_search,
            semantic_data_available,
            embedding_pool: None,
            symbol_vector_backend: std::sync::OnceLock::new(),
            chunk_backend: std::sync::OnceLock::new(),
            chunk_vector_backend: std::sync::OnceLock::new(),
            reranker: std::sync::OnceLock::new(),
            rerank_inflight: Arc::new(AtomicBool::new(false)),
            settings,
            indexed_paths: HashSet::new(),
            index_base,
        }
    }

    /// Get a reference to the underlying DocumentIndex.
    pub fn document_index(&self) -> &Arc<DocumentIndex> {
        &self.document_index
    }

    /// Get a reference to the Pipeline.
    pub fn pipeline(&self) -> &Pipeline {
        &self.pipeline
    }

    /// Get a reference to the settings.
    pub fn settings(&self) -> &Arc<Settings> {
        &self.settings
    }

    /// Get the index base path.
    pub fn index_base(&self) -> &Path {
        &self.index_base
    }

    // =========================================================================
    // Semantic Search Management
    // =========================================================================

    /// Enable semantic search with the configured model.
    pub fn enable_semantic_search(&mut self) -> FacadeResult<()> {
        let semantic_path = self.index_base.join("semantic");
        std::fs::create_dir_all(&semantic_path)?;

        let model = &self.settings.semantic_search.model;
        let backend = self.effective_semantic_backend(model);
        let (pool_size, max_batch_tokens, max_sequence_length) =
            self.effective_semantic_pool_config(model, backend);

        let semantic = SimpleSemanticSearch::from_model_name(model)?;
        self.semantic_search = Some(Arc::new(Mutex::new(semantic)));

        if self.settings.semantic_search.worker_enabled {
            self.embedding_pool = None;
            tracing::info!(
                target: "semantic",
                "Semantic worker mode enabled: embedding pool initialization skipped"
            );
        } else {
            let runtime = EmbeddingRuntimeConfig::from_semantic_settings(
                &self.settings.semantic_search,
                self.settings.workspace_root.as_deref(),
            );
            // Create embedding pool for parallel generation
            let pool = EmbeddingPool::new(
                pool_size,
                model,
                backend,
                max_batch_tokens,
                max_sequence_length,
                Some(runtime),
            )?;
            self.embedding_pool = Some(Arc::new(pool));
        }

        Ok(())
    }

    /// Check if semantic search data is available.
    pub fn has_semantic_search(&self) -> bool {
        self.semantic_data_available
    }

    /// Whether this facade uses single-save mode for semantic persistence.
    pub fn semantic_single_save_mode(&self) -> bool {
        self.settings.indexing.semantic_single_save_mode
    }

    /// Save semantic search data to disk.
    pub fn save_semantic_search(&self, path: &Path) -> FacadeResult<()> {
        if let Some(ref semantic) = self.semantic_search {
            let sem = semantic.lock().map_err(|_| IndexError::lock_error())?;
            sem.save(path)?;
        }
        Ok(())
    }

    /// Load semantic search data from disk.
    ///
    /// Initializes the lightweight SymbolVectorBackend for queries (~15 MB).
    /// The full SimpleSemanticSearch (~148 MB) is loaded lazily only when the
    /// first write operation (incremental indexing) requires it.
    pub fn load_semantic_search(&mut self, path: &Path) -> FacadeResult<bool> {
        if path.join("metadata.json").exists() {
            // Eagerly init the lightweight query backend.
            let _ = self
                .symbol_vector_backend
                .get_or_init(|| SymbolVectorBackend::open(path).ok());
            if self
                .symbol_vector_backend
                .get()
                .and_then(|o| o.as_ref())
                .is_some()
            {
                self.semantic_data_available = true;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Lazily load `SimpleSemanticSearch` for write operations (incremental indexing).
    ///
    /// Query-time reads use the lightweight `SymbolVectorBackend` (~15 MB).
    /// This method loads the full embedding HashMap (~148 MB) only when a
    /// mutation needs to update the semantic store.
    fn ensure_semantic_for_write(&mut self) -> FacadeResult<()> {
        if self.semantic_search.is_some() {
            return Ok(());
        }
        if !self.semantic_data_available {
            return Ok(());
        }
        let semantic_path = self.index_base.join("semantic");
        if !semantic_path.join("metadata.json").exists() {
            return Ok(());
        }
        let sem = SimpleSemanticSearch::load(&semantic_path).map_err(|e| {
            IndexError::General(format!("failed to load semantic search for write: {e}"))
        })?;
        tracing::debug!(
            target: "semantic",
            "Lazily loaded SimpleSemanticSearch for write path ({} embeddings)",
            sem.embedding_count()
        );
        self.semantic_search = Some(Arc::new(Mutex::new(sem)));
        Ok(())
    }

    /// Ensure embedding pool is initialized for generating new embeddings.
    ///
    /// Called lazily by methods that need to compute embeddings (reindexing, watcher).
    pub fn ensure_embedding_pool(&mut self) -> FacadeResult<()> {
        // Prefer the loaded semantic index model when available so newly generated
        // embeddings always match the on-disk embedding space.
        let model = self
            .semantic_search
            .as_ref()
            .and_then(|s| {
                s.lock()
                    .ok()
                    .and_then(|sem| sem.metadata().map(|m| m.model_name.clone()))
            })
            .unwrap_or_else(|| self.settings.semantic_search.model.clone());

        if self.settings.semantic_search.worker_enabled {
            self.embedding_pool = None;
            tracing::debug!(
                target: "semantic",
                "Semantic worker mode enabled: skipping in-process embedding pool"
            );
            return Ok(());
        }

        if let Some(existing) = self.embedding_pool.as_ref() {
            if existing.model_name() == model.as_str() {
                return Ok(());
            }
            tracing::warn!(
                target: "semantic",
                "Reinitializing embedding pool with model '{}' (was '{}')",
                model,
                existing.model_name()
            );
        }

        let backend = self.effective_semantic_backend(&model);
        let (pool_size, max_batch_tokens, max_sequence_length) =
            self.effective_semantic_pool_config(&model, backend);
        let runtime = EmbeddingRuntimeConfig::from_semantic_settings(
            &self.settings.semantic_search,
            self.settings.workspace_root.as_deref(),
        );
        let pool = EmbeddingPool::new(
            pool_size,
            &model,
            backend,
            max_batch_tokens,
            max_sequence_length,
            Some(runtime),
        )?;
        self.embedding_pool = Some(Arc::new(pool));
        tracing::debug!(
            target: "semantic",
            "Initialized embedding pool for indexing updates (model: {})",
            model
        );
        Ok(())
    }

    /// Get semantic search embedding count.
    pub fn semantic_search_embedding_count(&self) -> usize {
        let vb = self.symbol_vector_backend.get_or_init(|| {
            let semantic_path = self.index_base.join("semantic");
            SymbolVectorBackend::open(&semantic_path).ok()
        });
        vb.as_ref().map(|b| b.embedding_count()).unwrap_or(0)
    }

    /// Get binary index count.
    pub fn semantic_binary_index_count(&self) -> usize {
        let vb = self.symbol_vector_backend.get_or_init(|| {
            let semantic_path = self.index_base.join("semantic");
            SymbolVectorBackend::open(&semantic_path).ok()
        });
        vb.as_ref().map(|b| b.binary_index_count()).unwrap_or(0)
    }

    /// Get number of files with cached embeddings.
    pub fn semantic_embedded_file_count(&self) -> usize {
        self.semantic_search
            .as_ref()
            .map(|s| s.lock().map(|sem| sem.embedded_file_count()).unwrap_or(0))
            .unwrap_or(0)
    }

    /// Get semantic memory usage: (float_bytes, binary_bytes).
    pub fn semantic_memory_usage(&self) -> (usize, usize) {
        self.semantic_search
            .as_ref()
            .map(|s| s.lock().map(|sem| sem.memory_usage()).unwrap_or((0, 0)))
            .unwrap_or((0, 0))
    }

    /// Get semantic search metadata.
    ///
    /// Reads directly from disk metadata when SimpleSemanticSearch is not loaded.
    pub fn get_semantic_metadata(&self) -> Option<crate::semantic::SemanticMetadata> {
        // Try in-memory first (available after ensure_semantic_for_write)
        if let Some(ref sem) = self.semantic_search {
            if let Ok(guard) = sem.lock() {
                if let Some(m) = guard.metadata() {
                    return Some(m.clone());
                }
            }
        }
        // Fall back to disk metadata
        let semantic_path = self.index_base.join("semantic");
        crate::semantic::SemanticMetadata::load(&semantic_path).ok()
    }

    // =========================================================================
    // Symbol Query Methods (delegate to DocumentIndex)
    // =========================================================================

    /// Find a symbol by name.
    pub fn find_symbol(&self, name: &str) -> Option<SymbolId> {
        self.document_index
            .find_symbols_by_name(name, None)
            .ok()
            .and_then(|symbols| symbols.first().map(|s| s.id))
    }

    /// Find all symbols by name with optional language filter.
    pub fn find_symbols_by_name(&self, name: &str, language_filter: Option<&str>) -> Vec<Symbol> {
        self.document_index
            .find_symbols_by_name(name, language_filter)
            .unwrap_or_default()
    }

    /// Get a symbol by ID.
    pub fn get_symbol(&self, id: SymbolId) -> Option<Symbol> {
        self.document_index.find_symbol_by_id(id).ok().flatten()
    }

    /// Get all symbols (with limit).
    ///
    /// Returns empty vec on error for SimpleIndexer API compatibility.
    pub fn get_all_symbols(&self) -> Vec<Symbol> {
        self.document_index
            .get_all_symbols(10000)
            .unwrap_or_else(|e| {
                tracing::warn!(target: "facade", "get_all_symbols error: {e}");
                Vec::new()
            })
    }

    /// Get symbols by file ID.
    ///
    /// Returns empty vec on error for SimpleIndexer API compatibility.
    pub fn get_symbols_by_file(&self, file_id: FileId) -> Vec<Symbol> {
        self.document_index
            .find_symbols_by_file(file_id)
            .unwrap_or_default()
    }

    // =========================================================================
    // Relationship Query Methods (delegate to DocumentIndex)
    // =========================================================================

    /// Get functions called by a symbol.
    pub fn get_called_functions(&self, symbol_id: SymbolId) -> Vec<Symbol> {
        let relationships = self
            .document_index
            .get_relationships_from(symbol_id, RelationKind::Calls)
            .unwrap_or_default();

        let mut symbols = Vec::new();
        for (_, to_id, _) in relationships {
            if let Some(symbol) = self.get_symbol(to_id) {
                symbols.push(symbol);
            }
        }
        symbols
    }

    /// Get functions called by a symbol with metadata.
    pub fn get_called_functions_with_metadata(
        &self,
        symbol_id: SymbolId,
    ) -> Vec<(Symbol, Option<crate::relationship::RelationshipMetadata>)> {
        let relationships = self
            .document_index
            .get_relationships_from(symbol_id, RelationKind::Calls)
            .unwrap_or_default();

        let mut results = Vec::new();
        for (_, to_id, rel) in relationships {
            if let Some(symbol) = self.get_symbol(to_id) {
                results.push((symbol, rel.metadata));
            }
        }
        results
    }

    /// Get functions that call a symbol.
    pub fn get_calling_functions(&self, symbol_id: SymbolId) -> Vec<Symbol> {
        let relationships = self
            .document_index
            .get_relationships_to(symbol_id, RelationKind::Calls)
            .unwrap_or_default();

        let mut symbols = Vec::new();
        for (from_id, _, _) in relationships {
            if let Some(symbol) = self.get_symbol(from_id) {
                symbols.push(symbol);
            }
        }
        symbols
    }

    /// Get functions that call a symbol with metadata.
    pub fn get_calling_functions_with_metadata(
        &self,
        symbol_id: SymbolId,
    ) -> Vec<(Symbol, Option<crate::relationship::RelationshipMetadata>)> {
        let relationships = self
            .document_index
            .get_relationships_to(symbol_id, RelationKind::Calls)
            .unwrap_or_default();

        let mut results = Vec::new();
        for (from_id, _, rel) in relationships {
            if let Some(symbol) = self.get_symbol(from_id) {
                results.push((symbol, rel.metadata));
            }
        }
        results
    }

    /// Get implementations of a trait/interface.
    pub fn get_implementations(&self, trait_id: SymbolId) -> Vec<Symbol> {
        let relationships = self
            .document_index
            .get_relationships_to(trait_id, RelationKind::Implements)
            .unwrap_or_default();

        let mut symbols = Vec::new();
        for (from_id, _, _) in relationships {
            if let Some(symbol) = self.get_symbol(from_id) {
                symbols.push(symbol);
            }
        }
        symbols
    }

    /// Get traits implemented by a type.
    pub fn get_implemented_traits(&self, type_id: SymbolId) -> Vec<Symbol> {
        let relationships = self
            .document_index
            .get_relationships_from(type_id, RelationKind::Implements)
            .unwrap_or_default();

        let mut symbols = Vec::new();
        for (_, to_id, _) in relationships {
            if let Some(symbol) = self.get_symbol(to_id) {
                symbols.push(symbol);
            }
        }
        symbols
    }

    /// Get classes/types extended by a class.
    pub fn get_extends(&self, class_id: SymbolId) -> Vec<Symbol> {
        let relationships = self
            .document_index
            .get_relationships_from(class_id, RelationKind::Extends)
            .unwrap_or_default();

        let mut symbols = Vec::new();
        for (_, to_id, _) in relationships {
            if let Some(symbol) = self.get_symbol(to_id) {
                symbols.push(symbol);
            }
        }
        symbols
    }

    /// Get classes that extend a base class.
    pub fn get_extended_by(&self, base_class_id: SymbolId) -> Vec<Symbol> {
        let relationships = self
            .document_index
            .get_relationships_to(base_class_id, RelationKind::Extends)
            .unwrap_or_default();

        let mut symbols = Vec::new();
        for (from_id, _, _) in relationships {
            if let Some(symbol) = self.get_symbol(from_id) {
                symbols.push(symbol);
            }
        }
        symbols
    }

    /// Get types/symbols used by a symbol.
    pub fn get_uses(&self, symbol_id: SymbolId) -> Vec<Symbol> {
        let relationships = self
            .document_index
            .get_relationships_from(symbol_id, RelationKind::Uses)
            .unwrap_or_default();

        let mut symbols = Vec::new();
        for (_, to_id, _) in relationships {
            if let Some(symbol) = self.get_symbol(to_id) {
                symbols.push(symbol);
            }
        }
        symbols
    }

    /// Get symbols that use a type.
    pub fn get_used_by(&self, type_id: SymbolId) -> Vec<Symbol> {
        let relationships = self
            .document_index
            .get_relationships_to(type_id, RelationKind::Uses)
            .unwrap_or_default();

        let mut symbols = Vec::new();
        for (from_id, _, _) in relationships {
            if let Some(symbol) = self.get_symbol(from_id) {
                symbols.push(symbol);
            }
        }
        symbols
    }

    /// Get relationships for a symbol (by symbol ID).
    pub fn get_relationships_for_symbol(
        &self,
        symbol_id: SymbolId,
    ) -> FacadeResult<Vec<(SymbolId, SymbolId, Relationship)>> {
        let mut all_rels = Vec::new();

        // Get outgoing relationships
        for kind in &[
            RelationKind::Calls,
            RelationKind::Uses,
            RelationKind::Implements,
            RelationKind::Extends,
            RelationKind::Defines,
        ] {
            if let Ok(rels) = self.document_index.get_relationships_from(symbol_id, *kind) {
                all_rels.extend(rels);
            }
        }

        // Get incoming relationships
        for kind in &[
            RelationKind::Calls,
            RelationKind::Uses,
            RelationKind::Implements,
            RelationKind::Extends,
        ] {
            if let Ok(rels) = self.document_index.get_relationships_to(symbol_id, *kind) {
                all_rels.extend(rels);
            }
        }

        Ok(all_rels)
    }

    // =========================================================================
    // Complex Query Methods (facade-level orchestration)
    // =========================================================================

    /// Get symbol context with configurable relationship inclusion.
    pub fn get_symbol_context(
        &self,
        symbol_id: SymbolId,
        include: ContextIncludes,
    ) -> Option<SymbolContext> {
        let symbol = self.get_symbol(symbol_id)?;
        let file_path = self
            .document_index
            .get_file_path(symbol.file_id)
            .ok()
            .flatten()
            .unwrap_or_else(|| symbol.file_path.to_string());

        let mut relationships = SymbolRelationships::default();

        if include.contains(ContextIncludes::IMPLEMENTATIONS) {
            let impls = self.get_implementations(symbol_id);
            if !impls.is_empty() {
                relationships.implemented_by = Some(impls);
            }
            // Also get what this type implements
            let implemented = self.get_implemented_traits(symbol_id);
            if !implemented.is_empty() {
                relationships.implements = Some(implemented);
            }
        }

        if include.contains(ContextIncludes::DEFINITIONS) {
            if let Ok(rels) = self
                .document_index
                .get_relationships_from(symbol_id, RelationKind::Defines)
            {
                let defines: Vec<Symbol> = rels
                    .iter()
                    .filter_map(|(_, to_id, _)| self.get_symbol(*to_id))
                    .collect();
                if !defines.is_empty() {
                    relationships.defines = Some(defines);
                }
            }
        }

        if include.contains(ContextIncludes::CALLS) {
            let calls = self.get_called_functions_with_metadata(symbol_id);
            if !calls.is_empty() {
                relationships.calls = Some(calls);
            }
        }

        if include.contains(ContextIncludes::CALLERS) {
            let callers = self.get_calling_functions_with_metadata(symbol_id);
            if !callers.is_empty() {
                relationships.called_by = Some(callers);
            }
        }

        if include.contains(ContextIncludes::EXTENDS) {
            let extends = self.get_extends(symbol_id);
            if !extends.is_empty() {
                relationships.extends = Some(extends);
            }
            let extended_by = self.get_extended_by(symbol_id);
            if !extended_by.is_empty() {
                relationships.extended_by = Some(extended_by);
            }
        }

        if include.contains(ContextIncludes::USES) {
            let uses = self.get_uses(symbol_id);
            if !uses.is_empty() {
                relationships.uses = Some(uses);
            }
            let used_by = self.get_used_by(symbol_id);
            if !used_by.is_empty() {
                relationships.used_by = Some(used_by);
            }
        }

        Some(SymbolContext {
            symbol,
            file_path,
            relationships,
        })
    }

    /// Get dependencies (what a symbol depends on).
    pub fn get_dependencies(&self, symbol_id: SymbolId) -> HashMap<RelationKind, Vec<Symbol>> {
        let mut deps: HashMap<RelationKind, Vec<Symbol>> = HashMap::new();

        for kind in &[
            RelationKind::Calls,
            RelationKind::Uses,
            RelationKind::Implements,
            RelationKind::Defines,
        ] {
            let rels = self
                .document_index
                .get_relationships_from(symbol_id, *kind)
                .unwrap_or_default();
            let symbols: Vec<Symbol> = rels
                .iter()
                .filter_map(|(_, to_id, _)| self.get_symbol(*to_id))
                .collect();
            if !symbols.is_empty() {
                deps.insert(*kind, symbols);
            }
        }

        deps
    }

    /// Get dependents (what depends on a symbol).
    pub fn get_dependents(&self, symbol_id: SymbolId) -> HashMap<RelationKind, Vec<Symbol>> {
        let mut deps: HashMap<RelationKind, Vec<Symbol>> = HashMap::new();

        for kind in &[
            RelationKind::Calls,
            RelationKind::Uses,
            RelationKind::Implements,
        ] {
            let rels = self
                .document_index
                .get_relationships_to(symbol_id, *kind)
                .unwrap_or_default();
            let symbols: Vec<Symbol> = rels
                .iter()
                .filter_map(|(from_id, _, _)| self.get_symbol(*from_id))
                .collect();
            if !symbols.is_empty() {
                deps.insert(*kind, symbols);
            }
        }

        deps
    }

    /// Get impact radius (BFS traversal of dependents).
    pub fn get_impact_radius(
        &self,
        symbol_id: SymbolId,
        max_depth: Option<usize>,
    ) -> Vec<SymbolId> {
        let max_depth = max_depth.unwrap_or(2);
        let mut visited = HashSet::new();
        let mut queue = std::collections::VecDeque::new();

        queue.push_back((symbol_id, 0usize));
        visited.insert(symbol_id);

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            // Get dependents via Calls, Uses, Implements, Extends
            for kind in &[
                RelationKind::Calls,
                RelationKind::Uses,
                RelationKind::Implements,
                RelationKind::Extends,
            ] {
                if let Ok(rels) = self.document_index.get_relationships_to(current_id, *kind) {
                    for (from_id, _, _) in rels {
                        if visited.insert(from_id) {
                            queue.push_back((from_id, depth + 1));
                        }
                    }
                }
            }
        }

        // Remove the initial symbol from results
        visited.remove(&symbol_id);
        visited.into_iter().collect()
    }

    /// Get recursive call tree (downstream BFS).
    ///
    /// This is the inverse of `analyze_impact`:
    /// - `analyze_impact`: Upstream BFS (who depends on me?)
    /// - `get_call_tree`: Downstream BFS (what do I call?)
    ///
    /// Returns hierarchical tree showing complete execution flow.
    ///
    /// # Arguments
    /// * `symbol_id` - Root symbol to start tree from
    /// * `max_depth` - Maximum recursion depth (prevents huge trees)
    /// * `max_nodes` - Maximum total nodes (prevents memory issues)
    ///
    /// # Example
    /// ```ignore
    /// let tree = facade.get_call_tree(SymbolId(42), 4, 100);
    /// // Returns tree with up to 4 levels deep, max 100 total nodes
    /// ```
    pub fn get_call_tree(
        &self,
        symbol_id: SymbolId,
        max_depth: usize,
        max_nodes: usize,
    ) -> Vec<crate::relationship::CallTreeNode> {
        use std::collections::HashSet;

        // Track visited symbols to detect cycles
        let mut visited = HashSet::new();
        visited.insert(symbol_id);

        // Build tree recursively
        let mut total_nodes = 0;
        self.build_call_tree_recursive(
            symbol_id,
            0,
            max_depth,
            &mut visited,
            &mut total_nodes,
            max_nodes,
        )
    }

    /// Recursive helper to build call tree
    ///
    /// Uses DFS with backtracking for cycle detection.
    /// visited set is modified during recursion and restored on backtrack.
    fn build_call_tree_recursive(
        &self,
        current_id: SymbolId,
        current_depth: usize,
        max_depth: usize,
        visited: &mut std::collections::HashSet<SymbolId>,
        total_nodes: &mut usize,
        max_nodes: usize,
    ) -> Vec<crate::relationship::CallTreeNode> {
        use crate::relationship::{CallTreeNode, TruncationReason};

        // Check depth limit
        if current_depth >= max_depth {
            return Vec::new();
        }

        // Check total node limit
        if *total_nodes >= max_nodes {
            return Vec::new();
        }

        // Get functions called by current symbol (uses existing method)
        let called_funcs = self.get_called_functions_with_metadata(current_id);
        let mut nodes = Vec::new();

        for (symbol, metadata) in called_funcs {
            *total_nodes += 1;

            // Check if we hit max nodes after incrementing
            if *total_nodes > max_nodes {
                // Add truncation marker for last node
                let truncated_node = CallTreeNode {
                    symbol,
                    depth: current_depth + 1,
                    children: Vec::new(),
                    metadata,
                    is_external: false,
                    is_recursive: false,
                    truncated: true,
                    truncation_reason: Some(TruncationReason::MaxDepthReached),
                };
                nodes.push(truncated_node);
                break; // Stop processing more calls
            }

            // Detect cycles (symbol already in call chain)
            let is_cycle = visited.contains(&symbol.id);

            // Detect external calls (heuristic based on file path)
            let is_external = symbol.file_path.is_empty()
                || symbol.file_path.starts_with("std::")
                || symbol.file_path.starts_with("node_modules")
                || symbol.file_path.contains("/stdlib/");

            let mut node = CallTreeNode {
                symbol: symbol.clone(),
                depth: current_depth + 1,
                children: Vec::new(),
                metadata,
                is_external,
                is_recursive: is_cycle,
                truncated: is_cycle || is_external,
                truncation_reason: if is_cycle {
                    Some(TruncationReason::CycleDetected)
                } else if is_external {
                    Some(TruncationReason::ExternalCall)
                } else {
                    None
                },
            };

            // Only recurse if no cycle and not external
            if !is_cycle && !is_external {
                // Add to visited (backtracking)
                visited.insert(symbol.id);

                // Recurse
                node.children = self.build_call_tree_recursive(
                    symbol.id,
                    current_depth + 1,
                    max_depth,
                    visited,
                    total_nodes,
                    max_nodes,
                );

                // Remove from visited (backtracking for other branches)
                visited.remove(&symbol.id);
            }

            nodes.push(node);
        }

        nodes
    }

    // =========================================================================
    // Search Methods
    // =========================================================================

    /// Full-text search for symbols.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
        kind_filter: Option<SymbolKind>,
        module_filter: Option<&str>,
        language_filter: Option<&str>,
    ) -> FacadeResult<Vec<SearchResult>> {
        self.document_index
            .search(query, limit, kind_filter, module_filter, language_filter)
            .map_err(Into::into)
    }

    /// Semantic search using doc comment embeddings.
    pub fn semantic_search_docs(
        &self,
        query: &str,
        limit: usize,
    ) -> FacadeResult<Vec<(Symbol, f32)>> {
        self.semantic_search_docs_with_language(query, limit, None)
    }

    /// Semantic search with language filter.
    pub fn semantic_search_docs_with_language(
        &self,
        query: &str,
        limit: usize,
        language_filter: Option<&str>,
    ) -> FacadeResult<Vec<(Symbol, f32)>> {
        let sym_backend = self.symbol_vector_backend.get_or_init(|| {
            let semantic_path = self.index_base.join("semantic");
            SymbolVectorBackend::open(&semantic_path).ok()
        });
        let vb = sym_backend
            .as_ref()
            .ok_or(IndexError::SemanticSearchNotEnabled)?;

        let results = vb
            .search(query, limit, language_filter)
            .map_err(|e| IndexError::General(format!("symbol vector search failed: {e}")))?;

        let mut symbols = Vec::new();
        for (symbol_id, score) in results {
            if let Some(symbol) = self.get_symbol(symbol_id) {
                symbols.push((symbol, score));
            }
        }

        Ok(symbols)
    }

    /// Semantic search with score threshold.
    pub fn semantic_search_docs_with_threshold(
        &self,
        query: &str,
        limit: usize,
        threshold: f32,
    ) -> FacadeResult<Vec<(Symbol, f32)>> {
        self.semantic_search_docs_with_threshold_and_language(query, limit, threshold, None)
    }

    /// Semantic search with threshold and language filter.
    pub fn semantic_search_docs_with_threshold_and_language(
        &self,
        query: &str,
        limit: usize,
        threshold: f32,
        language_filter: Option<&str>,
    ) -> FacadeResult<Vec<(Symbol, f32)>> {
        let results = self.semantic_search_docs_with_language(query, limit, language_filter)?;

        Ok(results
            .into_iter()
            .filter(|(_, score)| *score >= threshold)
            .collect())
    }

    fn run_rerank_with_timeout(
        &self,
        reranker: Arc<crate::reranking::Reranker>,
        query: &str,
        docs: Vec<String>,
        limit: usize,
        timeout_ms: u64,
        label: &'static str,
    ) -> RerankExecution {
        if self
            .rerank_inflight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            tracing::warn!(
                target: "rerank",
                "{} rerank skipped: previous rerank still running",
                label
            );
            return RerankExecution::SkippedBusy;
        }

        let inflight = Arc::clone(&self.rerank_inflight);
        let query_owned = query.to_string();
        let timeout = std::time::Duration::from_millis(timeout_ms.max(1));
        let (tx, rx) = std::sync::mpsc::sync_channel(1);

        std::thread::spawn(move || {
            struct InflightReset(Arc<AtomicBool>);
            impl Drop for InflightReset {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::Release);
                }
            }

            let _reset = InflightReset(inflight);
            let result = reranker.rerank(&query_owned, &docs, limit);
            let _ = tx.send(result);
        });

        match rx.recv_timeout(timeout) {
            Ok(Ok(reranked)) => RerankExecution::Completed(reranked),
            Ok(Err(e)) => {
                tracing::warn!(target: "rerank", "{} rerank failed: {}", label, e);
                RerankExecution::Failed
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                tracing::warn!(
                    target: "rerank",
                    "{} rerank timed out after {}ms; using fusion results",
                    label,
                    timeout_ms.max(1)
                );
                RerankExecution::TimedOut
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                tracing::warn!(
                    target: "rerank",
                    "{} rerank worker disconnected; using fusion results",
                    label
                );
                RerankExecution::Failed
            }
        }
    }

    /// Hybrid search combining BM25 (Tantivy) and vector (semantic) search with RRF merge.
    ///
    /// Retrieves candidates from both retrieval systems and merges results using
    /// Reciprocal Rank Fusion (RRF). Falls back gracefully:
    /// - Both available: RRF merge
    /// - Semantic only: vector results
    /// - BM25 only: text search results
    /// - Neither: empty results + error
    pub fn hybrid_search(
        &self,
        query: &str,
        limit: usize,
        language_filter: Option<&str>,
    ) -> FacadeResult<Vec<(Symbol, f32)>> {
        use std::time::Instant;
        let start = Instant::now();

        // Fetch more candidates than final limit for better RRF merge quality
        let candidate_limit = (limit * 3).max(20);

        // 1. BM25 search (Tantivy) — always available
        let bm25_start = Instant::now();
        let bm25_results = self
            .document_index
            .search(query, candidate_limit, None, None, language_filter)
            .unwrap_or_default();
        let bm25_ms = bm25_start.elapsed().as_millis();

        // 2. Vector search (semantic) — lazy-init lightweight backend
        let vector_start = Instant::now();
        let sym_backend = self.symbol_vector_backend.get_or_init(|| {
            let semantic_path = self.index_base.join("semantic");
            SymbolVectorBackend::open(&semantic_path).ok()
        });
        let vector_results: Vec<(SymbolId, f32)> = match sym_backend.as_ref() {
            Some(vb) => vb
                .search(query, candidate_limit, language_filter)
                .unwrap_or_default(),
            None => Vec::new(),
        };
        let vector_ms = vector_start.elapsed().as_millis();

        // 3. RRF merge
        let rrf_start = Instant::now();
        let merged = rrf_merge(&bm25_results, &vector_results, 60);
        let rrf_ms = rrf_start.elapsed().as_millis();
        let mut bm25_by_symbol: HashMap<SymbolId, &SearchResult> =
            HashMap::with_capacity(bm25_results.len());
        for hit in &bm25_results {
            bm25_by_symbol.entry(hit.symbol_id).or_insert(hit);
        }
        let bm25_ids: HashSet<SymbolId> = bm25_results.iter().map(|r| r.symbol_id).collect();
        let vector_ids: HashSet<SymbolId> = vector_results.iter().map(|(id, _)| *id).collect();

        // 4. Reranker (optional, lazy-initialized)
        let rerank_start = Instant::now();
        let final_candidates: Vec<(SymbolId, f32)> = if let Some(reranker) = self
            .reranker
            .get_or_init(|| {
                if !self.settings.reranking.enabled {
                    return None;
                }
                let runtime =
                    crate::reranking::RerankerRuntimeOptions::from_config(&self.settings.reranking);
                match crate::reranking::Reranker::new(
                    &self.settings.reranking.model,
                    self.settings.reranking.max_length,
                    runtime,
                ) {
                    Ok(r) => Some(Arc::new(r)),
                    Err(e) => {
                        tracing::warn!("Reranker init failed: {e}");
                        None
                    }
                }
            })
            .as_ref()
        {
            let top_n = self.settings.reranking.top_n.min(merged.len());
            let base_candidates: Vec<_> = merged.iter().copied().take(top_n).collect();
            let candidates = apply_rerank_prefilter(
                &self.settings.reranking,
                &base_candidates,
                &bm25_ids,
                &vector_ids,
                limit,
            );

            // Build reranker payload from symbol metadata plus BM25 context snippet.
            let rerank_items: Vec<(SymbolId, String)> = candidates
                .iter()
                .filter_map(|(sid, _)| {
                    self.get_symbol(*sid).map(|symbol| {
                        let bm25_hit = bm25_by_symbol.get(sid).copied();
                        (*sid, build_rerank_text(&symbol, bm25_hit))
                    })
                })
                .collect();
            let docs: Vec<String> = rerank_items.iter().map(|(_, doc)| doc.clone()).collect();
            if docs.is_empty() {
                candidates.into_iter().take(limit).collect()
            } else {
                let rerank_timeout_ms = self.settings.reranking.timeout_ms.max(1);
                match self.run_rerank_with_timeout(
                    Arc::clone(reranker),
                    query,
                    docs,
                    limit,
                    rerank_timeout_ms,
                    "symbol",
                ) {
                    RerankExecution::Completed(reranked) => reranked
                        .iter()
                        .filter_map(|(idx, score)| {
                            rerank_items.get(*idx).map(|(sid, _)| (*sid, *score))
                        })
                        .collect(),
                    RerankExecution::Failed
                    | RerankExecution::TimedOut
                    | RerankExecution::SkippedBusy => candidates.into_iter().take(limit).collect(),
                }
            }
        } else {
            merged.iter().copied().take(limit).collect()
        };
        let rerank_ms = rerank_start.elapsed().as_millis();

        // 5. Confidence gate (optional): suppress low-confidence result lists.
        if let Some(metrics) = evaluate_confidence_gate(
            &self.settings.reranking,
            &final_candidates,
            &merged,
            &bm25_ids,
            &vector_ids,
        ) {
            tracing::info!(
                top1_symbol_id = metrics.top1_id.to_u32(),
                top1_score = metrics.top1_score,
                top1_probability = metrics.top1_prob,
                top1_rrf_score = metrics.top1_rrf,
                dual_source = metrics.dual_source,
                "confidence gate suppressed low-confidence retrieval results"
            );
            return Ok(Vec::new());
        }

        // 6. Lookup full symbols
        let mut results = Vec::new();
        for (symbol_id, score) in final_candidates {
            if let Some(symbol) = self.get_symbol(symbol_id) {
                results.push((symbol, score));
            }
        }

        tracing::info!(
            search_bm25_ms = bm25_ms,
            search_vector_ms = vector_ms,
            rrf_merge_ms = rrf_ms,
            rerank_ms = rerank_ms,
            total_ms = start.elapsed().as_millis(),
            bm25_candidates = bm25_results.len(),
            vector_candidates = vector_results.len(),
            final_results = results.len(),
            "hybrid search complete"
        );

        Ok(results)
    }

    /// Hybrid search with score threshold.
    pub fn hybrid_search_with_threshold(
        &self,
        query: &str,
        limit: usize,
        threshold: f32,
        language_filter: Option<&str>,
    ) -> FacadeResult<Vec<(Symbol, f32)>> {
        let results = self.hybrid_search(query, limit, language_filter)?;

        Ok(results
            .into_iter()
            .filter(|(_, score)| *score >= threshold)
            .collect())
    }

    /// Hybrid chunk search (vector + BM25 with RRF merge), followed by optional reranker and confidence gate.
    ///
    /// Returns chunk-level results with file line ranges and snippet content.
    pub fn hybrid_chunk_search_detailed(
        &self,
        query: &str,
        limit: usize,
        language_filter: Option<&str>,
    ) -> FacadeResult<ChunkSearchOutcome> {
        use std::time::Instant;
        let start = Instant::now();

        if !self.settings.chunk_search.enabled {
            return Ok(ChunkSearchOutcome::default());
        }

        let chunk_root = self.index_base.join("code_chunks");

        // Lazy-init cached chunk Tantivy backend (BM25 + stored-field lookup).
        let backend = self
            .chunk_backend
            .get_or_init(|| ChunkSearchBackend::open(&chunk_root).ok());
        let backend = match backend.as_ref() {
            Some(b) => b,
            None => return Ok(ChunkSearchOutcome::default()),
        };

        let bm25_start = Instant::now();
        let bm25_results = backend
            .bm25_search(
                query,
                self.settings.chunk_search.top_k_bm25,
                language_filter,
            )
            .unwrap_or_default();
        let bm25_ms = bm25_start.elapsed().as_millis();

        let vector_start = Instant::now();
        let mut bm25_only_fallback = false;

        // Lazy-init cached chunk vector backend (binary pre-filter + mmap random access).
        let vec_backend = self
            .chunk_vector_backend
            .get_or_init(|| ChunkVectorBackend::open(&chunk_root).ok());

        let vector_results = match vec_backend.as_ref() {
            Some(vb) => match vb.search(
                query,
                self.settings.chunk_search.top_k_vector,
                language_filter,
            ) {
                Ok(results) => results,
                Err(e) => {
                    tracing::warn!(
                        target: "chunk_search",
                        "chunk vector recall failed: {e}"
                    );
                    bm25_only_fallback = true;
                    Vec::new()
                }
            },
            None => {
                bm25_only_fallback = true;
                Vec::new()
            }
        };
        let vector_ms = vector_start.elapsed().as_millis();

        let rrf_start = Instant::now();
        // Convex combination of min-max normalised arm scores (TM2C2 family, Bruch & Gai
        // ECIR'23): RRF throws away score magnitude and weights both arms equally, which
        // costs precision when one arm is much stronger. Linear normalisations are
        // rank-equivalent up to a shift in alpha, so pool min-max is used instead of the
        // analytic BM25 bound and the shift is absorbed by the alpha sweep.
        let merged = match self.settings.chunk_search.fusion_alpha {
            Some(alpha) => {
                fn normalise(list: &[(u32, f32)]) -> HashMap<u32, f32> {
                    let mut lo = f32::MAX;
                    let mut hi = f32::MIN;
                    for (_, s) in list {
                        lo = lo.min(*s);
                        hi = hi.max(*s);
                    }
                    let span = (hi - lo).max(f32::EPSILON);
                    list.iter().map(|(id, s)| (*id, (s - lo) / span)).collect()
                }
                let b = normalise(&bm25_results);
                let v = normalise(&vector_results);
                let mut ids: HashSet<u32> = b.keys().copied().collect();
                ids.extend(v.keys().copied());
                let mut out: Vec<(u32, f32)> = ids
                    .into_iter()
                    .map(|id| {
                        // A chunk missing from an arm scores that arm's minimum, which is
                        // 0 after normalisation.
                        let bs = b.get(&id).copied().unwrap_or(0.0);
                        let vs = v.get(&id).copied().unwrap_or(0.0);
                        (id, alpha * vs + (1.0 - alpha) * bs)
                    })
                    .collect();
                out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                out
            }
            None => rrf_merge_chunk(
                &bm25_results,
                &vector_results,
                self.settings.chunk_search.rrf_k,
            ),
        };
        let rrf_ms = rrf_start.elapsed().as_millis();

        if merged.is_empty() {
            return Ok(ChunkSearchOutcome {
                bm25_only_fallback,
                timing: SearchTimingMs {
                    bm25: bm25_ms as u64,
                    vector: vector_ms as u64,
                    rrf: rrf_ms as u64,
                    total: start.elapsed().as_millis() as u64,
                    ..SearchTimingMs::default()
                },
                ..ChunkSearchOutcome::default()
            });
        }

        // Batch-lookup only the ~50 chunk records we actually need from Tantivy
        // stored fields, instead of parsing the full 469 MB chunks.json.
        let lookup_ids: Vec<u32> = merged.iter().map(|(id, _)| *id).collect();
        let chunk_map = backend.lookup_records(&lookup_ids)?;

        let bm25_ids: HashSet<SymbolId> = bm25_results
            .iter()
            .filter_map(|(id, _)| SymbolId::new(*id))
            .collect();
        let vector_ids: HashSet<SymbolId> = vector_results
            .iter()
            .filter_map(|(id, _)| SymbolId::new(*id))
            .collect();
        let bm25_chunk_ids: HashSet<u32> = bm25_results.iter().map(|(id, _)| *id).collect();
        let vector_chunk_ids: HashSet<u32> = vector_results.iter().map(|(id, _)| *id).collect();

        let ranking_cfg = &self.settings.chunk_search;
        let rerank_start = Instant::now();
        let (mut final_candidates, rerank_scores_used, rerank_timed_out): (
            Vec<(u32, f32)>,
            bool,
            bool,
        ) = if let Some(reranker) = self
            .reranker
            .get_or_init(|| {
                if !self.settings.reranking.enabled {
                    return None;
                }
                let runtime =
                    crate::reranking::RerankerRuntimeOptions::from_config(&self.settings.reranking);
                match crate::reranking::Reranker::new(
                    &self.settings.reranking.model,
                    self.settings.reranking.max_length,
                    runtime,
                ) {
                    Ok(r) => Some(Arc::new(r)),
                    Err(e) => {
                        tracing::warn!("Reranker init failed: {e}");
                        None
                    }
                }
            })
            .as_ref()
        {
            let top_n = self
                .settings
                .reranking
                .top_n
                .min(self.settings.chunk_search.top_k_fused)
                .min(merged.len());
            let base_candidates: Vec<_> = merged.iter().copied().take(top_n).collect();
            let candidates = apply_rerank_prefilter(
                &self.settings.reranking,
                &base_candidates,
                &bm25_chunk_ids,
                &vector_chunk_ids,
                limit,
            );

            let rerank_items: Vec<(u32, String)> = candidates
                .iter()
                .filter_map(|(id, _)| {
                    chunk_map
                        .get(id)
                        .map(|record| (*id, build_chunk_rerank_text(record)))
                })
                .collect();
            let docs: Vec<String> = rerank_items.iter().map(|(_, doc)| doc.clone()).collect();

            if docs.is_empty() {
                (candidates.into_iter().take(limit).collect(), false, false)
            } else {
                let rerank_timeout_ms = self.settings.reranking.timeout_ms.max(1);
                match self.run_rerank_with_timeout(
                    Arc::clone(reranker),
                    query,
                    docs,
                    limit,
                    rerank_timeout_ms,
                    "chunk",
                ) {
                    RerankExecution::Completed(reranked) => (
                        reranked
                            .iter()
                            .filter_map(|(idx, score)| {
                                rerank_items.get(*idx).map(|(id, _)| (*id, *score))
                            })
                            .collect(),
                        true,
                        false,
                    ),
                    RerankExecution::TimedOut => {
                        (candidates.into_iter().take(limit).collect(), false, true)
                    }
                    RerankExecution::Failed | RerankExecution::SkippedBusy => {
                        (candidates.into_iter().take(limit).collect(), false, false)
                    }
                }
            }
        } else {
            (
                // Keep the configured fusion depth regardless of how many results the
                // caller asked for. Truncating the candidate pool to `limit` made a
                // 10-result request produce only 10 candidates, so per-file diversity had
                // nothing to draw on and a 10-result response averaged 5.2 distinct files.
                // Truncation to `limit` happens after diversity, further down.
                merged
                    .iter()
                    .copied()
                    .take(self.settings.chunk_search.top_k_fused.max(limit.max(1)))
                    .collect(),
                false,
                false,
            )
        };
        let rerank_ms = rerank_start.elapsed().as_millis();

        if rerank_scores_used {
            match ranking_cfg
                .rerank_score_normalization
                .trim()
                .to_ascii_lowercase()
                .as_str()
            {
                "none" => {}
                "sigmoid" => {
                    for (_, score) in &mut final_candidates {
                        *score = sigmoid_score(*score);
                    }
                }
                unknown => {
                    tracing::warn!(
                        target: "chunk_search",
                        "unknown rerank_score_normalization='{}' (expected none|sigmoid); using none",
                        unknown
                    );
                }
            }
        }

        let final_candidates_for_gate: Vec<(SymbolId, f32)> = final_candidates
            .iter()
            .filter_map(|(id, score)| SymbolId::new(*id).map(|sid| (sid, *score)))
            .collect();
        let merged_for_gate: Vec<(SymbolId, f32)> = merged
            .iter()
            .filter_map(|(id, score)| SymbolId::new(*id).map(|sid| (sid, *score)))
            .collect();
        if let Some(metrics) = evaluate_confidence_gate(
            &self.settings.reranking,
            &final_candidates_for_gate,
            &merged_for_gate,
            &bm25_ids,
            &vector_ids,
        ) {
            let mut pruned_by = vec!["confidence_gate".to_string()];
            if rerank_timed_out {
                pruned_by.push("rerank_timeout".to_string());
            }
            tracing::info!(
                top1_chunk_id = metrics.top1_id.to_u32(),
                top1_score = metrics.top1_score,
                top1_probability = metrics.top1_prob,
                top1_rrf_score = metrics.top1_rrf,
                dual_source = metrics.dual_source,
                "confidence gate suppressed low-confidence chunk retrieval results"
            );
            return Ok(ChunkSearchOutcome {
                weak_count: final_candidates.len(),
                pruned_by,
                bm25_only_fallback,
                timing: SearchTimingMs {
                    bm25: bm25_ms as u64,
                    vector: vector_ms as u64,
                    rrf: rrf_ms as u64,
                    rerank: rerank_ms as u64,
                    total: start.elapsed().as_millis() as u64,
                },
                ..ChunkSearchOutcome::default()
            });
        }

        let trivial_names: HashSet<String> = ranking_cfg
            .symbol_trivial_names
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect();
        let query_terms = extract_query_terms(query);
        let hard_ignore_patterns = compile_glob_patterns(&ranking_cfg.hard_ignore_path_patterns);

        if !ranking_cfg.post_rerank_heuristics_enabled {
            let mut pruned_by = Vec::new();
            let mut dropped_hard_ignore = 0usize;
            let mut direct_candidates = Vec::with_capacity(final_candidates.len());
            for (chunk_id, score) in final_candidates {
                let Some(record) = chunk_map.get(&chunk_id) else {
                    continue;
                };
                if path_matches_any_pattern(&record.file_path, &hard_ignore_patterns) {
                    dropped_hard_ignore += 1;
                    continue;
                }
                direct_candidates.push((chunk_id, score));
            }
            if dropped_hard_ignore > 0 {
                pruned_by.push("hard_ignore".to_string());
            }
            if rerank_timed_out {
                pruned_by.push("rerank_timeout".to_string());
            }
            direct_candidates
                .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            // Retrieval is chunk-level but callers reason in files: several chunks of the
            // same file used to consume the whole response, so a limit of 10 returned far
            // fewer than 10 distinct files. Capping per file and backfilling from the
            // overflow keeps the response size identical while widening its file coverage
            // (measured file-level R@10 gap between limit=10 and limit=30 was +0.06..+0.12
            // across django/tokio/vite/hugo).
            let limit = limit.max(1);
            let max_per_file = ranking_cfg.diversity_max_per_file.max(1);

            // Chunk-level ranking cannot express that a file matched by several fair chunks
            // is stronger evidence than a file matched by one good chunk, even though
            // callers reason about files. Score files by their best chunk plus a decaying
            // tail so long files cannot win on volume alone, then emit chunks in file order.
            let alpha = ranking_cfg.file_evidence_alpha;
            let mut by_file: HashMap<&str, Vec<(u32, f32)>> = HashMap::new();
            for (chunk_id, score) in direct_candidates.iter() {
                if let Some(record) = chunk_map.get(chunk_id) {
                    by_file
                        .entry(record.file_path.as_str())
                        .or_default()
                        .push((*chunk_id, *score));
                }
            }
            let mut ranked: Vec<(&str, f32)> = by_file
                .iter()
                .map(|(path, chunks)| {
                    let head = chunks.first().map(|(_, s)| *s).unwrap_or(0.0);
                    let tail: f32 = chunks
                        .iter()
                        .skip(1)
                        .take(5)
                        .enumerate()
                        .map(|(i, (_, s))| *s / (i as f32 + 2.0))
                        .sum();
                    (*path, head + alpha * tail)
                })
                .collect();
            ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            let mut results = Vec::with_capacity(limit);
            let mut used: HashSet<u32> = HashSet::new();
            for (path, _) in ranked {
                if results.len() >= limit {
                    break;
                }
                let Some(chunks) = by_file.get(path) else {
                    continue;
                };
                for (chunk_id, score) in chunks.iter().take(max_per_file) {
                    if results.len() >= limit {
                        break;
                    }
                    if let Some(record) = chunk_map.get(chunk_id) {
                        used.insert(*chunk_id);
                        results.push(ChunkSearchResult::from_record(record, *score));
                    }
                }
            }
            // Never return fewer results than the caller asked for just because the
            // per-file cap ran out of distinct files.
            for (chunk_id, score) in direct_candidates.iter() {
                if results.len() >= limit {
                    break;
                }
                if used.contains(chunk_id) {
                    continue;
                }
                if let Some(record) = chunk_map.get(chunk_id) {
                    results.push(ChunkSearchResult::from_record(record, *score));
                }
            }
            let weak_count =
                direct_candidates.len().saturating_sub(results.len()) + dropped_hard_ignore;
            return Ok(ChunkSearchOutcome {
                results,
                weak_count,
                pruned_by,
                bm25_only_fallback,
                timing: SearchTimingMs {
                    bm25: bm25_ms as u64,
                    vector: vector_ms as u64,
                    rrf: rrf_ms as u64,
                    rerank: rerank_ms as u64,
                    total: start.elapsed().as_millis() as u64,
                },
            });
        }

        let mut rescored: Vec<(u32, f32)> = final_candidates
            .into_iter()
            .filter_map(|(chunk_id, mut score)| {
                let record = chunk_map.get(&chunk_id)?;

                if path_matches_any_pattern(&record.file_path, &hard_ignore_patterns) {
                    return None;
                }

                // Source-type weighting: implementation > types > tests > generated.
                let source_weight = source_weight_for_path(ranking_cfg, &record.file_path);
                score = downrank_score(score, source_weight);
                // Keep mixed text chunks available but behind code implementation by default.
                score = downrank_score(score, chunk_type_weight(record));

                if ranking_cfg.single_line_penalty_enabled && is_single_line_chunk(record) {
                    score = downrank_score(score, ranking_cfg.single_line_penalty_factor);
                }
                if ranking_cfg.block_chunk_boost_enabled
                    && is_block_chunk(record, ranking_cfg.block_chunk_min_lines)
                {
                    score = uprank_score(score, ranking_cfg.block_chunk_boost_factor);
                }

                // Query-result coherence check: downrank partial term matches.
                if ranking_cfg.coherence_penalty_enabled && query_terms.len() >= 2 {
                    let coverage = query_term_coverage(record, &query_terms);
                    if coverage < 0.5 {
                        score = downrank_score(score, ranking_cfg.coherence_penalty_factor);
                    }
                }

                // Symbol-aware bounded reranking (tie-break, not override).
                if ranking_cfg.symbol_aware_enabled {
                    let signal = self.symbol_signal_for_chunk(record, &trivial_names);
                    let bounded_weight = ranking_cfg.symbol_aware_weight.clamp(0.0, 0.25);
                    let max_adjust = score.abs().max(1.0) * bounded_weight;
                    score += (signal - 0.5) * 2.0 * max_adjust;
                }

                Some((chunk_id, score))
            })
            .collect();

        rescored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Deduplicate by exact chunk identity; then enforce diversity by file path.
        let mut seen = HashSet::new();
        let mut per_file: HashMap<&str, usize> = HashMap::new();
        let mut dedup_diversity_filtered = Vec::with_capacity(rescored.len());
        let mut dropped_dedup_diversity = 0usize;
        for (chunk_id, score) in rescored {
            let Some(record) = chunk_map.get(&chunk_id) else {
                continue;
            };
            let key = (
                record.file_path.as_str(),
                record.line_start,
                record.line_end,
            );
            if !seen.insert(key) {
                dropped_dedup_diversity += 1;
                continue;
            }
            let used = per_file.entry(record.file_path.as_str()).or_insert(0);
            if *used >= ranking_cfg.diversity_max_per_file.max(1) {
                dropped_dedup_diversity += 1;
                continue;
            }
            *used += 1;
            dedup_diversity_filtered.push((chunk_id, score));
        }

        let mut pruned_by = Vec::new();
        if dropped_dedup_diversity > 0 {
            pruned_by.push("dedup_diversity".to_string());
        }
        if rerank_timed_out {
            pruned_by.push("rerank_timeout".to_string());
        }

        let mut strong_count = dedup_diversity_filtered.len();
        if ranking_cfg.result_filter_enabled && !dedup_diversity_filtered.is_empty() {
            let min_keep = ranking_cfg.min_strong_keep.max(1).min(strong_count);
            let top_score = dedup_diversity_filtered[0].1;
            let relative_floor = top_score - ranking_cfg.relative_score_delta.max(0.0);
            strong_count = dedup_diversity_filtered
                .iter()
                .take_while(|(_, score)| *score >= relative_floor)
                .count()
                .max(min_keep);
            if strong_count < dedup_diversity_filtered.len() {
                pruned_by.push("relative_threshold".to_string());
            }

            for i in min_keep..dedup_diversity_filtered.len() {
                let prev = dedup_diversity_filtered[i - 1].1;
                let curr = dedup_diversity_filtered[i].1;
                if (prev - curr) >= ranking_cfg.cliff_min_drop.max(0.0) {
                    if i < strong_count {
                        strong_count = i;
                    }
                    pruned_by.push("cliff".to_string());
                    break;
                }
            }
            strong_count = strong_count.max(min_keep);
        }

        let capped = strong_count.min(limit.max(1));
        let mut results = Vec::new();
        for (chunk_id, score) in dedup_diversity_filtered.iter().take(capped) {
            if let Some(record) = chunk_map.get(chunk_id) {
                results.push(ChunkSearchResult::from_record(record, *score));
            }
        }
        let weak_count =
            dedup_diversity_filtered.len().saturating_sub(capped) + dropped_dedup_diversity;

        tracing::info!(
            search_bm25_ms = bm25_ms,
            search_vector_ms = vector_ms,
            rrf_merge_ms = rrf_ms,
            rerank_ms = rerank_ms,
            total_ms = start.elapsed().as_millis(),
            bm25_candidates = bm25_results.len(),
            vector_candidates = vector_results.len(),
            weak_results = weak_count,
            final_results = results.len(),
            "hybrid chunk search complete"
        );

        Ok(ChunkSearchOutcome {
            results,
            weak_count,
            pruned_by,
            bm25_only_fallback,
            timing: SearchTimingMs {
                bm25: bm25_ms as u64,
                vector: vector_ms as u64,
                rrf: rrf_ms as u64,
                rerank: rerank_ms as u64,
                total: start.elapsed().as_millis() as u64,
            },
        })
    }

    pub fn hybrid_chunk_search(
        &self,
        query: &str,
        limit: usize,
        language_filter: Option<&str>,
    ) -> FacadeResult<Vec<ChunkSearchResult>> {
        Ok(self
            .hybrid_chunk_search_detailed(query, limit, language_filter)?
            .results)
    }

    /// Hybrid chunk search with score threshold.
    pub fn hybrid_chunk_search_with_threshold_detailed(
        &self,
        query: &str,
        limit: usize,
        threshold: f32,
        language_filter: Option<&str>,
    ) -> FacadeResult<ChunkSearchOutcome> {
        let mut out = self.hybrid_chunk_search_detailed(query, limit, language_filter)?;
        let before = out.results.len();
        out.results.retain(|r| r.score >= threshold);
        let dropped = before.saturating_sub(out.results.len());
        if dropped > 0 {
            out.weak_count += dropped;
            out.pruned_by.push("score_threshold".to_string());
        }
        Ok(out)
    }

    /// Hybrid chunk search with score threshold.
    pub fn hybrid_chunk_search_with_threshold(
        &self,
        query: &str,
        limit: usize,
        threshold: f32,
        language_filter: Option<&str>,
    ) -> FacadeResult<Vec<ChunkSearchResult>> {
        Ok(self
            .hybrid_chunk_search_with_threshold_detailed(query, limit, threshold, language_filter)?
            .results)
    }

    fn symbol_signal_for_chunk(
        &self,
        record: &CodeChunkRecord,
        trivial_names: &HashSet<String>,
    ) -> f32 {
        let Some(symbol_id) = SymbolId::new(record.symbol_id) else {
            return 0.5;
        };
        let Some(symbol) = self.get_symbol(symbol_id) else {
            return 0.5;
        };

        let name = symbol.name.to_ascii_lowercase();
        let is_trivial = trivial_names.contains(&name);
        let kind_signal = match symbol.kind {
            SymbolKind::Function | SymbolKind::Method => 1.0,
            SymbolKind::Class | SymbolKind::Struct | SymbolKind::Enum | SymbolKind::Module => 0.8,
            SymbolKind::Interface | SymbolKind::Trait | SymbolKind::TypeAlias => 0.6,
            SymbolKind::Constant | SymbolKind::Field | SymbolKind::Variable => 0.45,
            SymbolKind::Parameter | SymbolKind::Macro => 0.35,
        };

        let fan_signal = if is_trivial {
            0.0
        } else {
            let fan_in = self
                .document_index
                .get_relationships_to(symbol_id, RelationKind::Calls)
                .map(|v| v.len())
                .unwrap_or(0);
            let fan_out = self
                .document_index
                .get_relationships_from(symbol_id, RelationKind::Calls)
                .map(|v| v.len())
                .unwrap_or(0);
            ((fan_in + fan_out + 1) as f32).ln().min(4.0) / 4.0
        };

        (0.65 * kind_signal + 0.35 * fan_signal).clamp(0.0, 1.0)
    }

    // =========================================================================
    // File Operations
    // =========================================================================

    /// Get file ID for a path.
    pub fn get_file_id_for_path(&self, path: &str) -> Option<FileId> {
        self.document_index
            .get_file_info(path)
            .ok()
            .flatten()
            .map(|(id, _, _)| id)
    }

    /// Get file path for a FileId.
    ///
    /// Returns None on error for SimpleIndexer API compatibility.
    pub fn get_file_path(&self, file_id: FileId) -> Option<String> {
        self.document_index.get_file_path(file_id).ok().flatten()
    }

    /// Get all indexed file paths.
    pub fn get_all_indexed_paths(&self) -> Vec<PathBuf> {
        self.document_index
            .get_all_indexed_paths()
            .unwrap_or_default()
    }

    // =========================================================================
    // Statistics Methods
    // =========================================================================

    /// Get the number of indexed symbols.
    pub fn symbol_count(&self) -> usize {
        self.document_index.count_symbols().unwrap_or(0)
    }

    /// Get the number of indexed files.
    pub fn file_count(&self) -> u32 {
        self.document_index.count_files().unwrap_or(0) as u32
    }

    /// Get the number of relationships.
    pub fn relationship_count(&self) -> usize {
        self.document_index.count_relationships().unwrap_or(0)
    }

    /// Get total Tantivy document count.
    pub fn document_count(&self) -> FacadeResult<u64> {
        self.document_index.document_count().map_err(Into::into)
    }

    // =========================================================================
    // Directory Tracking
    // =========================================================================

    /// Add a directory to tracked indexed paths.
    pub fn add_indexed_path(&mut self, dir_path: &Path) {
        if let Ok(canonical) = dir_path.canonicalize() {
            // Skip if already covered by an existing parent directory
            let already_covered = self
                .indexed_paths
                .iter()
                .any(|p| canonical.starts_with(p) && canonical != *p);
            if already_covered {
                return;
            }

            // Remove any child paths that would be covered by this directory
            self.indexed_paths
                .retain(|p| !p.starts_with(&canonical) || *p == canonical);
            self.indexed_paths.insert(canonical);
        } else {
            self.indexed_paths.insert(dir_path.to_path_buf());
        }
    }

    /// Get tracked indexed paths.
    pub fn get_indexed_paths(&self) -> &HashSet<PathBuf> {
        &self.indexed_paths
    }

    /// Update indexed paths from a vector.
    pub fn set_indexed_paths(&mut self, paths: Vec<PathBuf>) {
        self.indexed_paths = paths.into_iter().collect();
    }

    // =========================================================================
    // Mutation Methods (delegate to Pipeline)
    // =========================================================================

    /// Index a single file using the parallel pipeline.
    ///
    /// Returns `IndexingResult::Indexed` with the file ID on success.
    pub fn index_file(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> crate::IndexResult<crate::IndexingResult> {
        let path = path.as_ref();
        if self.semantic_data_available {
            self.ensure_semantic_for_write()?;
            self.ensure_embedding_pool()?;
        }
        let stats = self.pipeline.index_file_single(
            path,
            Arc::clone(&self.document_index),
            self.semantic_search.clone(),
            self.embedding_pool.clone(),
        )?;

        let delta = ChunkRefreshDelta {
            changed_files: vec![path.to_path_buf()],
            deleted_files: Vec::new(),
            force_full: false,
        };
        if let Err(e) = self.refresh_code_chunk_index(Some(&delta)) {
            tracing::warn!(target: "chunk_search", "failed to refresh code chunk index: {e}");
        }

        Ok(crate::IndexingResult::Indexed(stats.file_id))
    }

    /// Index a single file with optional force re-indexing.
    ///
    /// When `force` is true, removes the file first to ensure a fresh re-index.
    pub fn index_file_with_force(
        &mut self,
        path: impl AsRef<std::path::Path>,
        force: bool,
    ) -> crate::IndexResult<crate::IndexingResult> {
        let path = path.as_ref();

        if force {
            // Remove first to force re-index
            let _ = self.remove_file(path);
        }

        self.index_file(path)
    }

    /// Remove a file from the index.
    ///
    /// Uses the Pipeline's cleanup stage to remove symbols and embeddings.
    pub fn remove_file(&mut self, path: impl AsRef<std::path::Path>) -> crate::IndexResult<()> {
        let path = path.as_ref();
        if self.semantic_data_available {
            self.ensure_semantic_for_write()?;
        }
        let semantic_path = self.index_base.join("semantic");

        use crate::indexing::pipeline::stages::CleanupStage;
        let cleanup_stage = if let Some(ref sem) = self.semantic_search {
            CleanupStage::new(Arc::clone(&self.document_index), &semantic_path)
                .with_semantic(Arc::clone(sem))
        } else {
            CleanupStage::new(Arc::clone(&self.document_index), &semantic_path)
        };

        cleanup_stage.cleanup_files(&[path.to_path_buf()])?;
        let delta = ChunkRefreshDelta {
            changed_files: Vec::new(),
            deleted_files: vec![path.to_path_buf()],
            force_full: false,
        };
        if let Err(e) = self.refresh_code_chunk_index(Some(&delta)) {
            tracing::warn!(target: "chunk_search", "failed to refresh code chunk index: {e}");
        }
        Ok(())
    }

    /// Index a directory using the parallel pipeline.
    ///
    /// This is the primary indexing entry point using Pipeline.
    pub fn index_directory(&mut self, path: &Path, force: bool) -> FacadeResult<IndexingStats> {
        if self.semantic_data_available {
            self.ensure_semantic_for_write()?;
            self.ensure_embedding_pool()?;
        }

        let chunk_config = self.build_chunk_config();

        let stats = self.pipeline.index_incremental(
            path,
            Arc::clone(&self.document_index),
            self.semantic_search.clone(),
            self.embedding_pool.clone(),
            force,
            chunk_config,
        )?;

        // Update tracked paths
        self.add_indexed_path(path);

        // If chunk rebuild was NOT done in pipeline (no pool/disabled), fall back to post-pipeline
        if !self.settings.chunk_search.enabled || self.embedding_pool.is_none() {
            let mut changed_files =
                Vec::with_capacity(stats.new_file_paths.len() + stats.modified_file_paths.len());
            changed_files.extend(stats.new_file_paths.iter().cloned());
            changed_files.extend(stats.modified_file_paths.iter().cloned());
            let delta = ChunkRefreshDelta {
                changed_files,
                deleted_files: stats.deleted_file_paths.clone(),
                force_full: force,
            };
            if let Err(e) = self.refresh_code_chunk_index(Some(&delta)) {
                tracing::warn!(target: "chunk_search", "failed to refresh code chunk index: {e}");
            }
        }

        Ok(IndexingStats {
            files_indexed: stats.new_files + stats.modified_files,
            symbols_found: stats.index_stats.symbols_found,
            relationships_resolved: stats.phase2_stats.defines_resolved
                + stats.phase2_stats.calls_resolved
                + stats.phase2_stats.other_resolved,
        })
    }

    /// Index a directory with advanced options.
    ///
    /// Provides options for progress reporting, dry-run mode, force re-indexing,
    /// and limiting the number of files.
    pub fn index_directory_with_options(
        &mut self,
        dir: impl AsRef<Path>,
        progress: bool,
        dry_run: bool,
        force: bool,
        max_files: Option<usize>,
    ) -> crate::IndexResult<crate::indexing::progress::IndexStats> {
        use crate::indexing::FileWalker;
        use crate::indexing::progress::IndexStats;

        if self.semantic_data_available {
            self.ensure_semantic_for_write()?;
            self.ensure_embedding_pool()?;
        }

        let dir = dir.as_ref();
        let walker = FileWalker::new(Arc::clone(&self.settings));
        let files: Vec<_> = walker.walk(dir).collect();

        // Apply max_files limit if specified
        let files = if let Some(max) = max_files {
            files.into_iter().take(max).collect()
        } else {
            files
        };

        let total_files = files.len();

        // Handle dry-run mode
        if dry_run {
            println!("Would index {total_files} files:");
            for (i, file_path) in files.iter().enumerate() {
                if i < 5 {
                    println!("  {}", file_path.display());
                } else if i == 5 && total_files > 5 {
                    println!("  ... and {} more files", total_files - 5);
                    break;
                }
            }

            let mut stats = IndexStats::new();
            stats.files_indexed = total_files;
            return Ok(stats);
        }

        // Auto-force mode for empty indexes (clean index behaves like --force)
        let force = force || self.document_count().unwrap_or(0) == 0;

        // Build chunk config for parallel rebuild inside pipeline
        let chunk_config = self.build_chunk_config();

        // Use Pipeline for indexing with progress flag
        // The pipeline manages progress bars internally for clean sequential display
        let pipeline_stats = self.pipeline.index_incremental_with_progress_flag(
            dir,
            Arc::clone(&self.document_index),
            self.semantic_search.clone(),
            self.embedding_pool.clone(),
            force,
            progress && total_files > 0,
            total_files,
            chunk_config,
        )?;

        // Update tracked paths
        self.add_indexed_path(dir);

        // If chunk rebuild was NOT done in pipeline (no pool/disabled), fall back to post-pipeline
        if !self.settings.chunk_search.enabled || self.embedding_pool.is_none() {
            let mut changed_files = Vec::with_capacity(
                pipeline_stats.new_file_paths.len() + pipeline_stats.modified_file_paths.len(),
            );
            changed_files.extend(pipeline_stats.new_file_paths.iter().cloned());
            changed_files.extend(pipeline_stats.modified_file_paths.iter().cloned());
            let delta = ChunkRefreshDelta {
                changed_files,
                deleted_files: pipeline_stats.deleted_file_paths.clone(),
                force_full: force,
            };
            if let Err(e) = self.refresh_code_chunk_index(Some(&delta)) {
                tracing::warn!(target: "chunk_search", "failed to refresh code chunk index: {e}");
            }
        }

        // Convert to IndexStats format using pipeline's actual timing
        let mut stats = IndexStats::default();
        stats.files_indexed = pipeline_stats.new_files + pipeline_stats.modified_files;
        stats.symbols_found = pipeline_stats.index_stats.symbols_found;
        stats.elapsed = pipeline_stats.elapsed;

        Ok(stats)
    }

    /// Sync with configuration (compare stored vs config paths).
    ///
    /// Returns (added_dirs, removed_dirs, files_indexed, symbols_found).
    pub fn sync_with_config(
        &mut self,
        stored_paths: Option<Vec<PathBuf>>,
        config_paths: &[PathBuf],
        progress: bool,
    ) -> FacadeResult<SyncStats> {
        if self.semantic_data_available {
            self.ensure_semantic_for_write()?;
            self.ensure_embedding_pool()?;
        }

        let stored = stored_paths.unwrap_or_default();
        let stored_set: HashSet<PathBuf> = stored.iter().cloned().collect();
        let config_set: HashSet<PathBuf> = config_paths.iter().cloned().collect();

        // Determine what to add and remove
        let to_add: Vec<&PathBuf> = config_set.difference(&stored_set).collect();
        let to_remove: Vec<&PathBuf> = stored_set.difference(&config_set).collect();

        let mut stats = SyncStats::default();
        let mut did_parallel_chunk = false;

        // Index new directories with progress if enabled
        // Use force=true since these are new directories being indexed for the first time
        for path in &to_add {
            // Visual separator and directory label (stderr syncs with progress bars)
            eprintln!();
            eprintln!("Indexing directory: {}", path.display());

            // Count files first for accurate progress bar
            let file_count = if progress {
                use crate::indexing::FileWalker;
                let walker = FileWalker::new(Arc::clone(&self.settings));
                walker.walk(path).count()
            } else {
                0
            };

            let chunk_config = self.build_chunk_config();
            if chunk_config.is_some() {
                did_parallel_chunk = true;
            }

            let result = self.pipeline.index_incremental_with_progress_flag(
                path,
                Arc::clone(&self.document_index),
                self.semantic_search.clone(),
                self.embedding_pool.clone(),
                true, // force: new directories should be fully indexed
                progress,
                file_count,
                chunk_config,
            )?;
            stats.files_indexed += result.new_files + result.modified_files;
            stats.symbols_found += result.index_stats.symbols_found;
        }
        stats.added_dirs = to_add.len();

        // Remove files from removed directories
        for path in &to_remove {
            self.remove_directory_files(path)?;
        }
        stats.removed_dirs = to_remove.len();

        // Update tracked paths
        self.indexed_paths = config_set;

        // Only run post-sync chunk rebuild if parallel didn't handle it
        if stats.has_changes() && !did_parallel_chunk {
            if let Err(e) = self.refresh_code_chunk_index(None) {
                tracing::warn!(target: "chunk_search", "failed to refresh code chunk index: {e}");
            }
        }

        Ok(stats)
    }

    /// Remove all files from a directory.
    fn remove_directory_files(&self, _dir: &Path) -> FacadeResult<()> {
        // TODO: Implement using CleanupStage
        // For now, this is a placeholder
        Ok(())
    }
}

/// Build a text representation of a symbol for cross-encoder reranking.
///
/// Includes structural fields and bounded text snippets for robust ranking.
fn build_rerank_text(symbol: &Symbol, bm25_hit: Option<&SearchResult>) -> String {
    const MAX_SIGNATURE_CHARS: usize = 256;
    const MAX_DOC_CHARS: usize = 384;
    const MAX_CONTEXT_CHARS: usize = 512;
    const MAX_TOTAL_CHARS: usize = 1400;

    let mut parts = Vec::with_capacity(7);
    parts.push(format!("kind: {:?}", symbol.kind));
    parts.push(format!("name: {}", symbol.name));
    parts.push(format!("file: {}", symbol.file_path));

    if let Some(module_path) = symbol.module_path.as_deref() {
        if !module_path.is_empty() {
            parts.push(format!("module: {module_path}"));
        }
    }
    if let Some(sig) = symbol.signature.as_deref() {
        parts.push(format!(
            "signature: {}",
            truncate_for_rerank(sig, MAX_SIGNATURE_CHARS)
        ));
    }
    if let Some(doc) = symbol.doc_comment.as_deref() {
        parts.push(format!("doc: {}", truncate_for_rerank(doc, MAX_DOC_CHARS)));
    }
    if let Some(ctx) = bm25_hit.and_then(|hit| hit.context.as_deref()) {
        parts.push(format!(
            "context: {}",
            truncate_for_rerank(ctx, MAX_CONTEXT_CHARS)
        ));
    }

    let mut text = parts.join("\n");
    if text.chars().count() > MAX_TOTAL_CHARS {
        text = truncate_for_rerank(&text, MAX_TOTAL_CHARS);
    }
    text
}

fn truncate_for_rerank(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect()
}

#[derive(Debug, Clone, Copy)]
struct ConfidenceGateMetrics {
    top1_id: SymbolId,
    top1_score: f32,
    top1_prob: f32,
    top1_rrf: f32,
    dual_source: bool,
}

fn evaluate_confidence_gate(
    cfg: &crate::config::RerankingConfig,
    final_candidates: &[(SymbolId, f32)],
    merged: &[(SymbolId, f32)],
    bm25_ids: &HashSet<SymbolId>,
    vector_ids: &HashSet<SymbolId>,
) -> Option<ConfidenceGateMetrics> {
    if !cfg.confidence_gate_enabled || final_candidates.is_empty() {
        return None;
    }

    let top1_id = final_candidates[0].0;
    let top1_score = final_candidates[0].1;
    let top_scores: Vec<f32> = final_candidates
        .iter()
        .take(5)
        .map(|(_, score)| *score)
        .collect();
    let top1_prob = softmax_top1_probability(&top_scores);
    let top1_rrf = merged
        .iter()
        .find(|(sid, _)| *sid == top1_id)
        .map(|(_, score)| *score)
        .unwrap_or(0.0);
    let dual_source = bm25_ids.contains(&top1_id) && vector_ids.contains(&top1_id);

    let weak_dual_source = if cfg.confidence_gate_require_dual_source {
        !dual_source
    } else {
        true
    };
    if top1_prob < cfg.confidence_gate_min_top1_prob
        && top1_rrf < cfg.confidence_gate_min_rrf
        && weak_dual_source
    {
        return Some(ConfidenceGateMetrics {
            top1_id,
            top1_score,
            top1_prob,
            top1_rrf,
            dual_source,
        });
    }

    None
}

fn apply_rerank_prefilter<T>(
    cfg: &crate::config::RerankingConfig,
    candidates: &[(T, f32)],
    bm25_ids: &HashSet<T>,
    vector_ids: &HashSet<T>,
    limit: usize,
) -> Vec<(T, f32)>
where
    T: Copy + Eq + std::hash::Hash,
{
    if !cfg.prefilter_enabled || candidates.is_empty() {
        return candidates.to_vec();
    }

    let min_target = limit.max(1).min(candidates.len());
    let configured_target = cfg.prefilter_target_top_n.max(1);
    let target = configured_target.clamp(min_target, candidates.len());

    if candidates.len() <= target {
        return candidates.to_vec();
    }

    if cfg.prefilter_fallback_on_small_gap {
        let top_score = candidates[0].1;
        let tail_score = candidates[target - 1].1;
        let score_gap = top_score - tail_score;
        if score_gap < cfg.prefilter_small_gap_threshold {
            return candidates.to_vec();
        }
    }

    let mut out = Vec::with_capacity(
        target.saturating_add(
            cfg.prefilter_dual_source_tail_keep
                .min(candidates.len().saturating_sub(target)),
        ),
    );
    let mut seen = HashSet::with_capacity(out.capacity());

    for (id, score) in candidates.iter().take(target) {
        out.push((*id, *score));
        seen.insert(*id);
    }

    if cfg.prefilter_dual_source_tail_keep == 0 {
        return out;
    }

    let mut tail_kept = 0usize;
    for (id, score) in candidates.iter().skip(target) {
        if tail_kept >= cfg.prefilter_dual_source_tail_keep {
            break;
        }
        if seen.contains(id) {
            continue;
        }
        if bm25_ids.contains(id) && vector_ids.contains(id) {
            out.push((*id, *score));
            seen.insert(*id);
            tail_kept += 1;
        }
    }

    out
}

fn softmax_top1_probability(scores: &[f32]) -> f32 {
    if scores.is_empty() {
        return 0.0;
    }
    let max_score = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut denom = 0.0f32;
    let mut first = 0.0f32;
    for (idx, score) in scores.iter().enumerate() {
        let exp_score = (*score - max_score).exp();
        denom += exp_score;
        if idx == 0 {
            first = exp_score;
        }
    }
    if denom == 0.0 { 0.0 } else { first / denom }
}

fn downrank_score(score: f32, weight: f32) -> f32 {
    let w = weight.clamp(0.05, 1.0);
    if score >= 0.0 { score * w } else { score / w }
}

fn sigmoid_score(score: f32) -> f32 {
    1.0 / (1.0 + (-score).exp())
}

fn uprank_score(score: f32, factor: f32) -> f32 {
    let f = factor.clamp(1.0, 3.0);
    if score >= 0.0 { score * f } else { score / f }
}

fn chunk_line_count(record: &CodeChunkRecord) -> usize {
    (record.line_end.saturating_sub(record.line_start) as usize).saturating_add(1)
}

fn is_single_line_chunk(record: &CodeChunkRecord) -> bool {
    chunk_line_count(record) <= 1
}

fn is_block_chunk(record: &CodeChunkRecord, min_lines: usize) -> bool {
    if chunk_line_count(record) < min_lines.max(2) {
        return false;
    }
    matches!(
        record.chunk_type.as_str(),
        "function"
            | "method"
            | "class"
            | "struct"
            | "interface"
            | "enum"
            | "module"
            | "flow_if_else"
            | "flow_try_catch"
            | "flow_switch"
            | "flow_loop"
            | "flow_call_chain"
            | "flow_error_path"
    )
}

fn compile_glob_patterns(patterns: &[String]) -> Vec<Pattern> {
    patterns
        .iter()
        .filter_map(|p| Pattern::new(p).ok())
        .collect()
}

fn path_matches_any_pattern(path: &str, patterns: &[Pattern]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    let normalized = path.replace('\\', "/");
    let trimmed = normalized.trim_start_matches("./");
    patterns
        .iter()
        .any(|pattern| pattern.matches(&normalized) || pattern.matches(trimmed))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChunkSourceType {
    Implementation,
    Types,
    Tests,
    Generated,
}

fn classify_chunk_source(path: &str) -> ChunkSourceType {
    let p = path.to_ascii_lowercase();
    if p.contains("/generated/")
        || p.contains("__generated__")
        || p.contains(".generated.")
        || p.ends_with(".gen.ts")
    {
        return ChunkSourceType::Generated;
    }
    if p.contains("/__tests__/")
        || p.contains("/tests/")
        || p.contains(".test.")
        || p.contains(".spec.")
    {
        return ChunkSourceType::Tests;
    }
    if p.ends_with(".d.ts")
        || p.ends_with("types.ts")
        || p.contains("/types/")
        || p.ends_with("type.ts")
    {
        return ChunkSourceType::Types;
    }
    ChunkSourceType::Implementation
}

fn source_weight_for_path(cfg: &crate::config::ChunkSearchConfig, path: &str) -> f32 {
    match classify_chunk_source(path) {
        ChunkSourceType::Implementation => cfg.source_weight_impl,
        ChunkSourceType::Types => cfg.source_weight_types,
        ChunkSourceType::Tests => cfg.source_weight_tests,
        ChunkSourceType::Generated => cfg.source_weight_generated,
    }
}

fn chunk_type_weight(record: &CodeChunkRecord) -> f32 {
    match record.chunk_type.as_str() {
        "function" | "method" | "class" | "struct" | "module" | "interface" | "enum"
        | "flow_if_else" | "flow_try_catch" | "flow_switch" | "flow_loop" | "flow_call_chain"
        | "flow_error_path" => 1.0,
        "module_comment" | "inter_symbol_gap" | "imports_gap" => 0.92,
        "doc_chunk" => 0.65,
        "config_chunk" => 0.6,
        "constant" | "variable" | "field" | "type_alias" => 0.9,
        _ => 0.85,
    }
}

fn extract_query_terms(query: &str) -> Vec<String> {
    const STOPWORDS: &[&str] = &[
        "the", "a", "an", "for", "to", "of", "in", "on", "and", "or", "with", "by", "is", "are",
        "this", "that", "how", "what", "where", "when",
    ];
    let stop: HashSet<&str> = STOPWORDS.iter().copied().collect();
    query
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| s.len() >= 3 && !stop.contains(s.as_str()))
        .collect()
}

fn query_term_coverage(record: &CodeChunkRecord, query_terms: &[String]) -> f32 {
    if query_terms.is_empty() {
        return 1.0;
    }
    let mut haystack = String::new();
    haystack.push_str(&record.file_path);
    haystack.push(' ');
    haystack.push_str(&record.chunk_type);
    if let Some(scope) = record.parent_scope.as_deref() {
        haystack.push(' ');
        haystack.push_str(scope);
    }
    if let Some(sig) = record.signature.as_deref() {
        haystack.push(' ');
        haystack.push_str(sig);
    }
    if let Some(doc) = record.doc_comment.as_deref() {
        haystack.push(' ');
        haystack.push_str(doc);
    }
    haystack.push(' ');
    haystack.push_str(&record.snippet);
    let haystack = haystack.to_ascii_lowercase();

    let matched = query_terms
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .count();
    matched as f32 / query_terms.len() as f32
}

/// Reciprocal Rank Fusion (RRF) merge of BM25 and vector search results.
///
/// Combines ranked lists using the formula: score(d) = Σ 1/(k + rank_i(d))
/// where k is a smoothing constant (typically 60) that prevents high-ranked
/// items from dominating. Results present in both lists get boosted.
fn rrf_merge(bm25: &[SearchResult], vector: &[(SymbolId, f32)], k: u32) -> Vec<(SymbolId, f32)> {
    let mut scores: HashMap<SymbolId, f32> = HashMap::new();
    let k_f = k as f32;

    for (rank, result) in bm25.iter().enumerate() {
        *scores.entry(result.symbol_id).or_default() += 1.0 / (k_f + rank as f32 + 1.0);
    }
    for (rank, (symbol_id, _)) in vector.iter().enumerate() {
        *scores.entry(*symbol_id).or_default() += 1.0 / (k_f + rank as f32 + 1.0);
    }

    let mut sorted: Vec<_> = scores.into_iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    sorted
}

/// Reciprocal Rank Fusion merge for chunk IDs.
fn rrf_merge_chunk(bm25: &[(u32, f32)], vector: &[(u32, f32)], k: u32) -> Vec<(u32, f32)> {
    let mut scores: HashMap<u32, f32> = HashMap::new();
    let k_f = k as f32;

    for (rank, (chunk_id, _)) in bm25.iter().enumerate() {
        *scores.entry(*chunk_id).or_default() += 1.0 / (k_f + rank as f32 + 1.0);
    }
    for (rank, (chunk_id, _)) in vector.iter().enumerate() {
        *scores.entry(*chunk_id).or_default() += 1.0 / (k_f + rank as f32 + 1.0);
    }

    let mut sorted: Vec<_> = scores.into_iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    sorted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RerankingConfig;
    use crate::{FileId, Range, Symbol, SymbolId, SymbolKind};

    #[test]
    fn test_build_rerank_text_includes_structural_fields_and_context() {
        let symbol = Symbol::new(
            SymbolId::new(7).unwrap(),
            "authenticate",
            SymbolKind::Function,
            FileId::new(1).unwrap(),
            Range::new(1, 0, 10, 0),
        )
        .with_file_path("src/auth.ts")
        .with_module_path("auth.core")
        .with_signature("function authenticate(token: string): Promise<Status>")
        .with_doc("Completes OAuth flow for MCP providers.");

        let hit = SearchResult {
            symbol_id: symbol.id,
            name: "authenticate".to_string(),
            kind: SymbolKind::Function,
            file_path: "src/auth.ts".to_string(),
            line: 1,
            column: 0,
            doc_comment: Some("Doc".to_string()),
            signature: Some("sig".to_string()),
            module_path: "auth.core".to_string(),
            score: 1.0,
            highlights: Vec::new(),
            context: Some("Reads callback params and exchanges auth code for tokens".to_string()),
        };

        let text = build_rerank_text(&symbol, Some(&hit));
        assert!(text.contains("kind: Function"));
        assert!(text.contains("name: authenticate"));
        assert!(text.contains("file: src/auth.ts"));
        assert!(text.contains("module: auth.core"));
        assert!(text.contains("signature: function authenticate"));
        assert!(text.contains("doc: Completes OAuth flow"));
        assert!(text.contains("context: Reads callback params"));
    }

    #[test]
    fn test_softmax_top1_probability_is_well_formed() {
        let p = softmax_top1_probability(&[2.0, 1.0, 0.0]);
        assert!(p > 0.0);
        assert!(p < 1.0);
    }

    #[test]
    fn test_confidence_gate_suppresses_low_confidence_single_source_results() {
        let cfg = RerankingConfig {
            confidence_gate_enabled: true,
            confidence_gate_min_top1_prob: 0.34,
            confidence_gate_min_rrf: 0.018,
            confidence_gate_require_dual_source: true,
            ..RerankingConfig::default()
        };
        let final_candidates = vec![
            (SymbolId::new(10).unwrap(), -2.5000),
            (SymbolId::new(11).unwrap(), -2.5001),
            (SymbolId::new(12).unwrap(), -2.5002),
        ];
        let merged = vec![
            (SymbolId::new(10).unwrap(), 0.010),
            (SymbolId::new(11).unwrap(), 0.009),
        ];
        let bm25_ids: HashSet<SymbolId> = [SymbolId::new(10).unwrap()].into_iter().collect();
        let vector_ids: HashSet<SymbolId> = [SymbolId::new(11).unwrap()].into_iter().collect();

        let metrics =
            evaluate_confidence_gate(&cfg, &final_candidates, &merged, &bm25_ids, &vector_ids);
        assert!(metrics.is_some(), "expected confidence gate to suppress");
    }

    #[test]
    fn test_confidence_gate_keeps_dual_source_high_rrf_results() {
        let cfg = RerankingConfig {
            confidence_gate_enabled: true,
            confidence_gate_min_top1_prob: 0.34,
            confidence_gate_min_rrf: 0.018,
            confidence_gate_require_dual_source: true,
            ..RerankingConfig::default()
        };
        let final_candidates = vec![
            (SymbolId::new(20).unwrap(), -1.5),
            (SymbolId::new(21).unwrap(), -2.0),
        ];
        let merged = vec![
            (SymbolId::new(20).unwrap(), 0.031),
            (SymbolId::new(21).unwrap(), 0.020),
        ];
        let bm25_ids: HashSet<SymbolId> = [SymbolId::new(20).unwrap()].into_iter().collect();
        let vector_ids: HashSet<SymbolId> = [SymbolId::new(20).unwrap()].into_iter().collect();

        let metrics =
            evaluate_confidence_gate(&cfg, &final_candidates, &merged, &bm25_ids, &vector_ids);
        assert!(
            metrics.is_none(),
            "expected confidence gate to keep results"
        );
    }

    #[test]
    fn test_apply_rerank_prefilter_disabled_is_noop() {
        let cfg = RerankingConfig {
            prefilter_enabled: false,
            ..RerankingConfig::default()
        };
        let candidates = vec![
            (SymbolId::new(1).unwrap(), 0.20),
            (SymbolId::new(2).unwrap(), 0.19),
            (SymbolId::new(3).unwrap(), 0.18),
        ];
        let bm25_ids: HashSet<SymbolId> = candidates.iter().map(|(id, _)| *id).collect();
        let vector_ids = bm25_ids.clone();

        let filtered = apply_rerank_prefilter(&cfg, &candidates, &bm25_ids, &vector_ids, 2);
        assert_eq!(filtered, candidates);
    }

    #[test]
    fn test_apply_rerank_prefilter_small_gap_falls_back_to_full_set() {
        let cfg = RerankingConfig {
            prefilter_enabled: true,
            prefilter_target_top_n: 2,
            prefilter_fallback_on_small_gap: true,
            prefilter_small_gap_threshold: 0.05,
            prefilter_dual_source_tail_keep: 1,
            ..RerankingConfig::default()
        };
        let candidates = vec![
            (SymbolId::new(10).unwrap(), 0.200),
            (SymbolId::new(11).unwrap(), 0.190),
            (SymbolId::new(12).unwrap(), 0.180),
        ];
        let bm25_ids: HashSet<SymbolId> = candidates.iter().map(|(id, _)| *id).collect();
        let vector_ids = bm25_ids.clone();

        let filtered = apply_rerank_prefilter(&cfg, &candidates, &bm25_ids, &vector_ids, 1);
        assert_eq!(filtered, candidates);
    }

    #[test]
    fn test_apply_rerank_prefilter_trims_and_keeps_dual_source_tail() {
        let cfg = RerankingConfig {
            prefilter_enabled: true,
            prefilter_target_top_n: 2,
            prefilter_fallback_on_small_gap: true,
            prefilter_small_gap_threshold: 0.01,
            prefilter_dual_source_tail_keep: 1,
            ..RerankingConfig::default()
        };
        let candidates = vec![
            (SymbolId::new(1).unwrap(), 0.30),
            (SymbolId::new(2).unwrap(), 0.10),
            (SymbolId::new(3).unwrap(), 0.09),
            (SymbolId::new(4).unwrap(), 0.08),
        ];
        let bm25_ids: HashSet<SymbolId> = [1u32, 2, 3]
            .into_iter()
            .map(|id| SymbolId::new(id).unwrap())
            .collect();
        let vector_ids: HashSet<SymbolId> = [1u32, 3, 4]
            .into_iter()
            .map(|id| SymbolId::new(id).unwrap())
            .collect();

        let filtered = apply_rerank_prefilter(&cfg, &candidates, &bm25_ids, &vector_ids, 1);
        assert_eq!(
            filtered,
            vec![
                (SymbolId::new(1).unwrap(), 0.30),
                (SymbolId::new(2).unwrap(), 0.10),
                (SymbolId::new(3).unwrap(), 0.09),
            ]
        );
    }

    #[test]
    fn test_downrank_score_is_sign_aware() {
        let pos = downrank_score(1.0, 0.5);
        let neg = downrank_score(-1.0, 0.5);
        assert!((pos - 0.5).abs() < f32::EPSILON);
        assert!((neg + 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_uprank_score_is_sign_aware() {
        let pos = uprank_score(1.0, 1.5);
        let neg = uprank_score(-1.0, 1.5);
        assert!((pos - 1.5).abs() < f32::EPSILON);
        assert!((neg + (1.0 / 1.5)).abs() < 1e-6);
    }

    #[test]
    fn test_sigmoid_score_maps_logits_to_zero_one() {
        let low = sigmoid_score(-2.0);
        let mid = sigmoid_score(0.0);
        let high = sigmoid_score(2.0);
        assert!(low > 0.0 && low < 0.5);
        assert!((mid - 0.5).abs() < 1e-6);
        assert!(high > 0.5 && high < 1.0);
    }

    #[test]
    fn test_path_matches_any_pattern_for_eval_paths() {
        let patterns = compile_glob_patterns(&["evals/**".to_string(), "**/*.eval.*".to_string()]);
        assert!(path_matches_any_pattern(
            "./evals/answer-vs-act.eval.ts",
            &patterns
        ));
        assert!(path_matches_any_pattern("evals/foo/bar.ts", &patterns));
        assert!(!path_matches_any_pattern(
            "./packages/core/src/policy.ts",
            &patterns
        ));
    }

    #[test]
    fn test_classify_chunk_source_prefers_generated_and_tests() {
        assert_eq!(
            classify_chunk_source("src/__generated__/types.ts"),
            ChunkSourceType::Generated
        );
        assert_eq!(
            classify_chunk_source("src/foo/bar.spec.ts"),
            ChunkSourceType::Tests
        );
        assert_eq!(
            classify_chunk_source("src/shared/types.ts"),
            ChunkSourceType::Types
        );
        assert_eq!(
            classify_chunk_source("src/core/edit.ts"),
            ChunkSourceType::Implementation
        );
    }

    #[test]
    fn test_extract_query_terms_filters_stopwords() {
        let terms = extract_query_terms("how does edit tool work in the codebase");
        assert!(terms.contains(&"edit".to_string()));
        assert!(terms.contains(&"tool".to_string()));
        assert!(!terms.contains(&"how".to_string()));
        assert!(!terms.contains(&"the".to_string()));
    }

    #[test]
    fn test_chunk_type_weight_downranks_doc_and_config_chunks() {
        let mut rec = crate::chunks::CodeChunkRecord {
            chunk_id: 1,
            symbol_id: 0,
            file_path: "./README.md".to_string(),
            language: Some("markdown".to_string()),
            chunk_type: "doc_chunk".to_string(),
            parent_scope: None,
            line_start: 0,
            line_end: 1,
            signature: None,
            doc_comment: None,
            snippet: "edit tool docs".to_string(),
            embedding_text: "x".to_string(),
        };
        assert!(chunk_type_weight(&rec) < 1.0);
        rec.chunk_type = "config_chunk".to_string();
        assert!(chunk_type_weight(&rec) < 1.0);
        rec.chunk_type = "function".to_string();
        assert!((chunk_type_weight(&rec) - 1.0).abs() < f32::EPSILON);
        rec.chunk_type = "flow_if_else".to_string();
        assert!((chunk_type_weight(&rec) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_line_shape_helpers_and_block_detection() {
        let mut rec = crate::chunks::CodeChunkRecord {
            chunk_id: 1,
            symbol_id: 1,
            file_path: "./src/core/edit.ts".to_string(),
            language: Some("typescript".to_string()),
            chunk_type: "function".to_string(),
            parent_scope: Some("edit".to_string()),
            line_start: 42,
            line_end: 42,
            signature: Some("function edit()".to_string()),
            doc_comment: None,
            snippet: "EDIT_TOOL_NAMES.has(tool.name)".to_string(),
            embedding_text: "x".to_string(),
        };
        assert!(is_single_line_chunk(&rec));
        assert!(!is_block_chunk(&rec, 4));

        rec.line_end = 50;
        assert!(!is_single_line_chunk(&rec));
        assert!(is_block_chunk(&rec, 4));
    }
}
