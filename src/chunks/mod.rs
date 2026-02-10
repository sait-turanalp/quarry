//! Code chunk indexing and retrieval for symbol + mixed-file RAG.
//!
//! Phase 1+2 implementation:
//! - Builds chunk records from indexed symbols
//! - Adds mixed chunks (module comments, inter-symbol gaps, config/doc text chunks)
//! - Persists chunk metadata + semantic vectors under `.codanna/index/code_chunks/`
//! - Maintains dedicated chunk Tantivy index for BM25 recall (`doc_type=chunk`)
//! - Provides vector/BM25 recall helpers for hybrid retrieval

use crate::config::SemanticBackend;
use crate::parsing::{get_registry, FlowKind};
use crate::semantic::{
    EmbeddingPool, SemanticMetadata, SemanticVectorStorage, SimpleSemanticSearch,
};
use crate::symbol::ScopeContext;
use crate::vector::{
    create_text_embedding_with_max_length, create_text_embedding_with_runtime,
    cosine_similarity, BinaryVector, EmbeddingRuntimeConfig, MmapVectorStorage, SegmentOrdinal,
};
use crate::{IndexError, Settings, Symbol, SymbolId, SymbolKind};
use crate::semantic::OptimizedStaticModel;
use fastembed::TextEmbedding;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::{BooleanQuery, BoostQuery, Occur, PhraseQuery, Query, QueryParser, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, NumericOptions, Schema, SchemaBuilder, TextFieldIndexing,
    TextOptions, Value, FAST, STORED, STRING,
};
use tantivy::{Index, IndexReader, IndexSettings, ReloadPolicy, TantivyDocument as Document, Term};

/// Backend abstraction for chunk embedding generation.
///
/// Chunk indexing depends only on this trait so embedding backend migrations
/// (Granite <-> model2vec) do not require chunk-pipeline changes.
pub trait RecallBackend: Send + Sync {
    fn model_name(&self) -> &str;
    fn dimensions(&self) -> usize;
    fn encode(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, IndexError>;
}

enum ActiveModel {
    Fastembed(TextEmbedding),
    Optimized(OptimizedStaticModel),
}

/// Active semantic embedding backend for code chunks.
pub struct ActiveRecallBackend {
    model_name: String,
    dimensions: usize,
    model: Mutex<ActiveModel>,
}

impl ActiveRecallBackend {
    pub fn new(
        model_name: &str,
        backend: SemanticBackend,
        runtime: EmbeddingRuntimeConfig,
        max_sequence_length: usize,
    ) -> Result<Self, IndexError> {
        match backend {
            SemanticBackend::Model2vec => {
                let source = model_name.strip_prefix("model2vec:").unwrap_or(model_name);
                let opt = OptimizedStaticModel::from_local(source).map_err(|e| {
                    IndexError::ConfigError {
                        reason: format!(
                            "failed to initialize optimized static model '{source}': {e}"
                        ),
                    }
                })?;
                let dims = opt.dim();
                Ok(Self {
                    model_name: model_name.to_string(),
                    dimensions: dims,
                    model: Mutex::new(ActiveModel::Optimized(opt)),
                })
            }
            SemanticBackend::Fastembed => {
                let (mut model, resolved_name) = create_text_embedding_with_runtime(
                    model_name,
                    false,
                    Some(max_sequence_length),
                    Some(&runtime),
                )
                .map_err(|e| IndexError::ConfigError {
                    reason: format!("failed to initialize fastembed backend: {e}"),
                })?;
                let dims = model
                    .embed(vec!["test"], None)
                    .map_err(|e| {
                        IndexError::General(format!("failed to probe embedding dimensions: {e}"))
                    })?
                    .into_iter()
                    .next()
                    .map(|v| v.len())
                    .ok_or_else(|| {
                        IndexError::General("embedding probe returned no vectors".to_string())
                    })?;

                Ok(Self {
                    model_name: resolved_name,
                    dimensions: dims,
                    model: Mutex::new(ActiveModel::Fastembed(model)),
                })
            }
        }
    }
}

impl RecallBackend for ActiveRecallBackend {
    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn encode(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, IndexError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut guard = self.model.lock().map_err(|_| IndexError::lock_error())?;
        match &mut *guard {
            ActiveModel::Fastembed(model) => model
                .embed(texts.to_vec(), None)
                .map_err(|e| IndexError::General(format!("fastembed encode failed: {e}"))),
            ActiveModel::Optimized(model) => {
                Ok(model.encode_batch_parallel(texts, Some(512)))
            }
        }
    }
}

/// Adapter wrapping an existing EmbeddingPool as a RecallBackend.
/// Avoids creating a separate model instance for chunk embedding.
pub struct PoolRecallAdapter {
    pool: Arc<EmbeddingPool>,
}

impl PoolRecallAdapter {
    pub fn new(pool: Arc<EmbeddingPool>) -> Self {
        Self { pool }
    }
}

impl RecallBackend for PoolRecallAdapter {
    fn model_name(&self) -> &str {
        self.pool.model_name()
    }
    fn dimensions(&self) -> usize {
        self.pool.dimensions()
    }
    fn encode(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, IndexError> {
        Ok(self.pool.encode_texts(texts))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunkRecord {
    pub chunk_id: u32,
    pub symbol_id: u32,
    pub file_path: String,
    pub language: Option<String>,
    pub chunk_type: String,
    pub parent_scope: Option<String>,
    pub line_start: u32,
    pub line_end: u32,
    pub signature: Option<String>,
    pub doc_comment: Option<String>,
    pub snippet: String,
    pub embedding_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSearchResult {
    pub chunk_id: u32,
    pub filepath: String,
    pub line_start: u32,
    pub line_end: u32,
    pub snippet: String,
    pub parent_scope: Option<String>,
    pub language: Option<String>,
    pub score: f32,
}

impl ChunkSearchResult {
    pub fn from_record(record: &CodeChunkRecord, score: f32) -> Self {
        Self {
            chunk_id: record.chunk_id,
            filepath: record.file_path.clone(),
            line_start: record.line_start,
            line_end: record.line_end,
            snippet: record.snippet.clone(),
            parent_scope: record.parent_scope.clone(),
            language: record.language.clone(),
            score,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChunkIndexManifest {
    model_name: String,
    dimension: usize,
    chunk_count: usize,
    generated_at_utc_secs: u64,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ChunkIndexStats {
    pub chunks_indexed: usize,
    pub embeddings_indexed: usize,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ChunkIncrementalStats {
    pub retained_chunks: usize,
    pub removed_chunks: usize,
    pub encoded_chunks: usize,
    pub reused_embeddings: usize,
}

#[derive(Debug, Clone)]
struct ChunkTantivySchema {
    doc_type: Field,
    chunk_id: Field,
    symbol_id: Field,
    file_path: Field,
    language: Field,
    chunk_type: Field,
    parent_scope: Field,
    line_start: Field,
    line_end: Field,
    signature: Field,
    doc_comment: Field,
    snippet: Field,
}

impl ChunkTantivySchema {
    fn build() -> (Schema, Self) {
        let mut builder = SchemaBuilder::default();

        let doc_type = builder.add_text_field("doc_type", STRING | STORED | FAST);

        let indexed_u64 = NumericOptions::default()
            .set_indexed()
            .set_stored()
            .set_fast();

        let text_options = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("default")
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            )
            .set_stored();

        let chunk_id = builder.add_u64_field("chunk_id", indexed_u64.clone());
        let symbol_id = builder.add_u64_field("symbol_id", indexed_u64.clone());
        let file_path = builder.add_text_field("file_path", STRING | STORED | FAST);
        let language = builder.add_text_field("language", STRING | STORED | FAST);
        let chunk_type = builder.add_text_field("chunk_type", STRING | STORED | FAST);
        let parent_scope = builder.add_text_field("parent_scope", text_options.clone());
        let line_start = builder.add_u64_field("line_start", indexed_u64.clone());
        let line_end = builder.add_u64_field("line_end", indexed_u64);
        let signature = builder.add_text_field("signature", text_options.clone());
        let doc_comment = builder.add_text_field("doc_comment", text_options.clone());
        let snippet = builder.add_text_field("snippet", text_options);

        let schema = builder.build();
        (
            schema,
            Self {
                doc_type,
                chunk_id,
                symbol_id,
                file_path,
                language,
                chunk_type,
                parent_scope,
                line_start,
                line_end,
                signature,
                doc_comment,
                snippet,
            },
        )
    }

    fn from_schema(schema: &Schema) -> Result<Self, IndexError> {
        let field = |name: &str| {
            schema.get_field(name).map_err(|e| {
                IndexError::General(format!("chunk schema missing field '{name}': {e}"))
            })
        };

        Ok(Self {
            doc_type: field("doc_type")?,
            chunk_id: field("chunk_id")?,
            symbol_id: field("symbol_id")?,
            file_path: field("file_path")?,
            language: field("language")?,
            chunk_type: field("chunk_type")?,
            parent_scope: field("parent_scope")?,
            line_start: field("line_start")?,
            line_end: field("line_end")?,
            signature: field("signature")?,
            doc_comment: field("doc_comment")?,
            snippet: field("snippet")?,
        })
    }
}

pub struct CodeChunkIndexer {
    root: PathBuf,
}

impl CodeChunkIndexer {
    pub fn new(index_base: &Path) -> Self {
        Self {
            root: index_base.join("code_chunks"),
        }
    }

    pub fn rebuild_from_symbols(
        &self,
        symbols: &[Symbol],
        backend: &dyn RecallBackend,
        workspace_root: Option<&Path>,
        settings: Option<&Settings>,
        max_snippet_chars: usize,
        snippet_context_lines: usize,
        snippet_min_lines: usize,
        indexed_paths: Option<&[PathBuf]>,
        flow_chunk_enabled: bool,
        flow_chunk_languages: &[String],
        flow_chunk_max_per_symbol: usize,
        chunk_token_target: usize,
        chunk_token_max: usize,
        chunk_token_overlap: usize,
        configured_dimension: Option<usize>,
    ) -> Result<ChunkIndexStats, IndexError> {
        if let Some(expected_dim) = configured_dimension {
            if expected_dim != backend.dimensions() {
                return Err(IndexError::ConfigError {
                    reason: format!(
                        "chunk_search.embedding_dimension={} does not match active backend dimension={}",
                        expected_dim,
                        backend.dimensions()
                    ),
                });
            }
        }

        std::fs::create_dir_all(&self.root)?;
        let semantic_path = self.root.join("semantic");
        let manifest_path = self.root.join("manifest.json");

        let mut source_cache: HashMap<PathBuf, Vec<String>> = HashMap::new();
        let mut chunks = Vec::with_capacity(symbols.len());
        let stage_t = Instant::now();
        for symbol in symbols {
            if let Some(chunk) = build_chunk_record(
                symbol,
                workspace_root,
                max_snippet_chars,
                snippet_context_lines,
                snippet_min_lines,
                &mut source_cache,
            ) {
                chunks.push(chunk);
            }
        }
        tracing::info!(
            target: "pipeline",
            "CHUNK STAGE build_chunk_record: {} chunks from {} symbols, {} files cached in {:.1}s",
            chunks.len(), symbols.len(), source_cache.len(), stage_t.elapsed().as_secs_f64()
        );
        let mut next_chunk_id = symbols
            .iter()
            .map(|s| s.id.to_u32())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let stage_t = Instant::now();
        append_mixed_chunks(
            symbols,
            indexed_paths,
            workspace_root,
            max_snippet_chars,
            &mut source_cache,
            &mut chunks,
            &mut next_chunk_id,
        );
        tracing::info!(
            target: "pipeline",
            "CHUNK STAGE append_mixed_chunks: {} total chunks in {:.1}s",
            chunks.len(), stage_t.elapsed().as_secs_f64()
        );
        if flow_chunk_enabled {
            let stage_t = Instant::now();
            append_flow_chunks(
                symbols,
                settings,
                workspace_root,
                max_snippet_chars,
                flow_chunk_languages,
                flow_chunk_max_per_symbol,
                &mut source_cache,
                &mut chunks,
                &mut next_chunk_id,
            );
            tracing::info!(
                target: "pipeline",
                "CHUNK STAGE append_flow_chunks: {} total chunks in {:.1}s",
                chunks.len(), stage_t.elapsed().as_secs_f64()
            );
        }
        let stage_t = Instant::now();
        apply_token_budget_to_chunks(
            &mut chunks,
            &mut next_chunk_id,
            max_snippet_chars,
            chunk_token_target,
            chunk_token_max,
            chunk_token_overlap,
        );
        dedup_chunk_records(&mut chunks);
        tracing::info!(
            target: "pipeline",
            "CHUNK STAGE token_budget+dedup: {} final chunks in {:.1}s",
            chunks.len(), stage_t.elapsed().as_secs_f64()
        );

        let t0 = std::time::Instant::now();
        let texts: Vec<String> = chunks.iter().map(|c| c.embedding_text.clone()).collect();
        let chunk_count = texts.len();
        let embeddings = backend.encode(&texts)?;
        let embed_dur = t0.elapsed();

        if semantic_path.exists() {
            std::fs::remove_dir_all(&semantic_path)?;
        }
        std::fs::create_dir_all(&semantic_path)?;

        let mut semantic = SimpleSemanticSearch::from_model_name(
            backend.model_name(),
        )?;
        let items: Vec<(SymbolId, Vec<f32>, String)> = chunks
            .iter()
            .zip(embeddings)
            .filter_map(|(chunk, embedding)| {
                SymbolId::new(chunk.chunk_id).map(|sid| {
                    (
                        sid,
                        embedding,
                        chunk
                            .language
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string()),
                    )
                })
            })
            .collect();
        let embeddings_indexed = semantic.store_embeddings(items);
        let t1 = std::time::Instant::now();
        semantic.save(&semantic_path)?;
        let save_dur = t1.elapsed();

        let t2 = std::time::Instant::now();
        write_tantivy_chunk_index(&self.root, &chunks)?;
        let write_dur = t2.elapsed();

        tracing::info!(
            target: "pipeline",
            "CHUNK REBUILD detail: {} chunks | embed={:.1}s ({}/s) | save={:.1}s | write={:.1}s",
            chunk_count,
            embed_dur.as_secs_f64(),
            if embed_dur.as_secs_f64() > 0.0 { (chunk_count as f64 / embed_dur.as_secs_f64()) as u64 } else { 0 },
            save_dur.as_secs_f64(),
            write_dur.as_secs_f64(),
        );

        let manifest = ChunkIndexManifest {
            model_name: backend.model_name().to_string(),
            dimension: backend.dimensions(),
            chunk_count: chunks.len(),
            generated_at_utc_secs: current_unix_time_secs(),
        };
        let manifest_json = serde_json::to_string(&manifest)
            .map_err(|e| IndexError::General(format!("failed to serialize chunk manifest: {e}")))?;
        std::fs::write(&manifest_path, manifest_json)?;

        Ok(ChunkIndexStats {
            chunks_indexed: chunks.len(),
            embeddings_indexed,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rebuild_incremental_from_symbols(
        &self,
        symbols: &[Symbol],
        backend: &dyn RecallBackend,
        workspace_root: Option<&Path>,
        settings: Option<&Settings>,
        max_snippet_chars: usize,
        snippet_context_lines: usize,
        snippet_min_lines: usize,
        flow_chunk_enabled: bool,
        flow_chunk_languages: &[String],
        flow_chunk_max_per_symbol: usize,
        chunk_token_target: usize,
        chunk_token_max: usize,
        chunk_token_overlap: usize,
        configured_dimension: Option<usize>,
        changed_files: &[PathBuf],
        deleted_files: &[PathBuf],
        verbose_logging: bool,
    ) -> Result<ChunkIndexStats, IndexError> {
        if let Some(expected_dim) = configured_dimension {
            if expected_dim != backend.dimensions() {
                return Err(IndexError::ConfigError {
                    reason: format!(
                        "chunk_search.embedding_dimension={} does not match active backend dimension={}",
                        expected_dim,
                        backend.dimensions()
                    ),
                });
            }
        }

        let matcher = ChunkPathMatcher::new(changed_files, deleted_files, workspace_root);
        if !matcher.has_any() {
            return Ok(ChunkIndexStats::default());
        }

        std::fs::create_dir_all(&self.root)?;
        let semantic_path = self.root.join("semantic");
        let manifest_path = self.root.join("manifest.json");

        let start = Instant::now();
        let existing_chunks = match ChunkSearchBackend::open(&self.root) {
            Ok(backend) => backend.load_all_records()?,
            Err(_) => Vec::new(),
        };
        let mut retained_chunks = Vec::with_capacity(existing_chunks.len());
        let mut removed_chunks = 0usize;
        for chunk in existing_chunks {
            if !matcher.matches_chunk_path(&chunk.file_path, workspace_root) {
                retained_chunks.push(chunk);
            } else {
                removed_chunks += 1;
            }
        }

        let mut changed_symbols: Vec<Symbol> = symbols
            .iter()
            .filter(|s| matcher.matches_symbol_path(s.file_path.as_ref(), workspace_root))
            .cloned()
            .collect();
        changed_symbols.sort_by_key(|s| {
            (
                s.file_path.to_string(),
                s.range.start_line,
                s.range.end_line,
                s.id.to_u32(),
            )
        });

        let mut source_cache: HashMap<PathBuf, Vec<String>> = HashMap::new();
        let mut new_chunks = Vec::with_capacity(changed_symbols.len());
        for symbol in &changed_symbols {
            if let Some(chunk) = build_chunk_record(
                symbol,
                workspace_root,
                max_snippet_chars,
                snippet_context_lines,
                snippet_min_lines,
                &mut source_cache,
            ) {
                new_chunks.push(chunk);
            }
        }

        let mut next_chunk_id = retained_chunks
            .iter()
            .map(|c| c.chunk_id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);

        append_symbol_file_mixed_chunks(
            &changed_symbols,
            workspace_root,
            max_snippet_chars,
            &mut source_cache,
            &mut new_chunks,
            &mut next_chunk_id,
        );
        append_config_doc_chunks_for_paths(
            changed_files,
            workspace_root,
            max_snippet_chars,
            &mut source_cache,
            &mut new_chunks,
            &mut next_chunk_id,
        );
        if flow_chunk_enabled {
            append_flow_chunks(
                &changed_symbols,
                settings,
                workspace_root,
                max_snippet_chars,
                flow_chunk_languages,
                flow_chunk_max_per_symbol,
                &mut source_cache,
                &mut new_chunks,
                &mut next_chunk_id,
            );
        }
        apply_token_budget_to_chunks(
            &mut new_chunks,
            &mut next_chunk_id,
            max_snippet_chars,
            chunk_token_target,
            chunk_token_max,
            chunk_token_overlap,
        );
        dedup_chunk_records(&mut new_chunks);

        let new_chunk_ids: HashSet<u32> = new_chunks.iter().map(|c| c.chunk_id).collect();
        let mut merged_chunks = retained_chunks;
        merged_chunks.extend(new_chunks);

        let existing_embeddings = load_existing_chunk_embeddings(&semantic_path);
        let mut items: Vec<(SymbolId, Vec<f32>, String)> = Vec::with_capacity(merged_chunks.len());
        let mut to_encode: Vec<(u32, String, String)> = Vec::new();
        let mut reused_embeddings = 0usize;

        for chunk in &merged_chunks {
            if !new_chunk_ids.contains(&chunk.chunk_id) {
                if let Some(embedding) = existing_embeddings.get(&chunk.chunk_id) {
                    if let Some(sid) = SymbolId::new(chunk.chunk_id) {
                        items.push((
                            sid,
                            embedding.clone(),
                            chunk
                                .language
                                .clone()
                                .unwrap_or_else(|| "unknown".to_string()),
                        ));
                        reused_embeddings += 1;
                        continue;
                    }
                }
            }
            to_encode.push((
                chunk.chunk_id,
                chunk.embedding_text.clone(),
                chunk
                    .language
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            ));
        }

        if !to_encode.is_empty() {
            let texts: Vec<String> = to_encode.iter().map(|(_, text, _)| text.clone()).collect();
            let embeddings = backend.encode(&texts)?;
            for ((chunk_id, _, language), embedding) in to_encode.into_iter().zip(embeddings) {
                if let Some(sid) = SymbolId::new(chunk_id) {
                    items.push((sid, embedding, language));
                }
            }
        }

        if semantic_path.exists() {
            std::fs::remove_dir_all(&semantic_path)?;
        }
        std::fs::create_dir_all(&semantic_path)?;

        let mut semantic = SimpleSemanticSearch::from_model_name(
            backend.model_name(),
        )?;
        let embeddings_indexed = semantic.store_embeddings(items);
        semantic.save(&semantic_path)?;

        write_tantivy_chunk_index(&self.root, &merged_chunks)?;

        let manifest = ChunkIndexManifest {
            model_name: backend.model_name().to_string(),
            dimension: backend.dimensions(),
            chunk_count: merged_chunks.len(),
            generated_at_utc_secs: current_unix_time_secs(),
        };
        let manifest_json = serde_json::to_string(&manifest)
            .map_err(|e| IndexError::General(format!("failed to serialize chunk manifest: {e}")))?;
        std::fs::write(&manifest_path, manifest_json)?;

        if verbose_logging {
            let incremental = ChunkIncrementalStats {
                retained_chunks: merged_chunks.len().saturating_sub(new_chunk_ids.len()),
                removed_chunks,
                encoded_chunks: merged_chunks.len().saturating_sub(reused_embeddings),
                reused_embeddings,
            };
            tracing::info!(
                target: "chunk_search",
                "chunk incremental rebuild: retained={}, removed={}, encoded={}, reused={}, total={} ({:?})",
                incremental.retained_chunks,
                incremental.removed_chunks,
                incremental.encoded_chunks,
                incremental.reused_embeddings,
                merged_chunks.len(),
                start.elapsed()
            );
        }

        Ok(ChunkIndexStats {
            chunks_indexed: merged_chunks.len(),
            embeddings_indexed,
        })
    }
}

#[derive(Debug)]
struct ChunkPathMatcher {
    changed: HashSet<PathBuf>,
    deleted: HashSet<PathBuf>,
}

impl ChunkPathMatcher {
    fn new(
        changed_files: &[PathBuf],
        deleted_files: &[PathBuf],
        workspace_root: Option<&Path>,
    ) -> Self {
        let changed: HashSet<PathBuf> = changed_files
            .iter()
            .map(|p| normalize_match_path(p, workspace_root))
            .collect();
        let deleted: HashSet<PathBuf> = deleted_files
            .iter()
            .map(|p| normalize_match_path(p, workspace_root))
            .collect();
        Self { changed, deleted }
    }

    fn has_any(&self) -> bool {
        !(self.changed.is_empty() && self.deleted.is_empty())
    }

    fn matches_symbol_path(&self, symbol_path: &str, workspace_root: Option<&Path>) -> bool {
        let normalized = normalize_match_path(Path::new(symbol_path), workspace_root);
        self.changed.contains(&normalized)
    }

    fn matches_chunk_path(&self, chunk_path: &str, workspace_root: Option<&Path>) -> bool {
        let normalized = normalize_match_path(Path::new(chunk_path), workspace_root);
        self.changed.contains(&normalized) || self.deleted.contains(&normalized)
    }
}

fn normalize_match_path(path: &Path, workspace_root: Option<&Path>) -> PathBuf {
    let base = if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(root) = workspace_root {
        root.join(path)
    } else {
        path.to_path_buf()
    };
    normalize_components(&base)
}

fn normalize_components(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn append_symbol_file_mixed_chunks(
    symbols: &[Symbol],
    workspace_root: Option<&Path>,
    max_snippet_chars: usize,
    source_cache: &mut HashMap<PathBuf, Vec<String>>,
    chunks: &mut Vec<CodeChunkRecord>,
    next_chunk_id: &mut u32,
) {
    let mut by_file: HashMap<&str, Vec<&Symbol>> = HashMap::new();
    for symbol in symbols {
        by_file
            .entry(symbol.file_path.as_ref())
            .or_default()
            .push(symbol);
    }

    for (file_path, mut file_symbols) in by_file {
        file_symbols.sort_by_key(|s| (s.range.start_line, s.range.end_line));
        append_module_comment_chunk(
            file_path,
            &file_symbols,
            workspace_root,
            max_snippet_chars,
            source_cache,
            chunks,
            next_chunk_id,
        );
        append_inter_symbol_gap_chunks(
            file_path,
            &file_symbols,
            workspace_root,
            max_snippet_chars,
            source_cache,
            chunks,
            next_chunk_id,
        );
    }
}

fn append_config_doc_chunks_for_paths(
    changed_files: &[PathBuf],
    workspace_root: Option<&Path>,
    max_snippet_chars: usize,
    source_cache: &mut HashMap<PathBuf, Vec<String>>,
    chunks: &mut Vec<CodeChunkRecord>,
    next_chunk_id: &mut u32,
) {
    const OVERLAP_LINES: usize = 2;
    const MIN_DOC_CHUNK_CHARS: usize = 80;
    const MIN_CONFIG_CHUNK_CHARS: usize = 20;
    const MAX_CHUNKS_PER_FILE: usize = 24;
    const MAX_TEXT_FILE_BYTES: u64 = 512 * 1024;

    let mut seen_resolved: HashSet<PathBuf> = HashSet::new();
    for raw_path in changed_files {
        let resolved = resolve_indexed_path(raw_path, workspace_root);
        let Some(kind) = classify_mixed_file_kind(&resolved) else {
            continue;
        };
        if !seen_resolved.insert(resolved.clone()) {
            continue;
        }
        if std::fs::metadata(&resolved).map(|m| m.len()).unwrap_or(0) > MAX_TEXT_FILE_BYTES {
            continue;
        }
        let Some(lines) = load_source_lines(&resolved, source_cache) else {
            continue;
        };
        if lines.is_empty() {
            continue;
        }

        let file_path = normalize_chunk_file_path(raw_path, workspace_root);
        let language = detect_text_language(raw_path);
        let chunks_for_file = split_text_file_chunks(
            lines,
            max_snippet_chars.max(256),
            OVERLAP_LINES,
            match kind {
                MixedFileKind::Doc => MIN_DOC_CHUNK_CHARS,
                MixedFileKind::Config => MIN_CONFIG_CHUNK_CHARS,
            },
            MAX_CHUNKS_PER_FILE,
        );
        let chunk_type = match kind {
            MixedFileKind::Doc => "doc_chunk",
            MixedFileKind::Config => "config_chunk",
        };

        for (line_start, line_end, snippet) in chunks_for_file {
            let title = snippet
                .lines()
                .find(|l| !l.trim().is_empty())
                .map(|l| truncate_chars(l.trim(), 96))
                .unwrap_or_else(|| "content".to_string());
            let embedding_text =
                build_embedding_text(&file_path, None, chunk_type, &title, None, &snippet);
            chunks.push(CodeChunkRecord {
                chunk_id: *next_chunk_id,
                symbol_id: 0,
                file_path: file_path.clone(),
                language: language.clone(),
                chunk_type: chunk_type.to_string(),
                parent_scope: None,
                line_start: line_start as u32,
                line_end: line_end as u32,
                signature: None,
                doc_comment: None,
                snippet,
                embedding_text,
            });
            *next_chunk_id = next_chunk_id.saturating_add(1);
        }
    }
}

fn load_existing_chunk_embeddings(path: &Path) -> HashMap<u32, Vec<f32>> {
    if !path.join("metadata.json").exists() {
        return HashMap::new();
    }

    let Ok(mut storage) = SemanticVectorStorage::open(path) else {
        tracing::warn!(
            target: "chunk_search",
            "failed to open existing chunk semantic store '{}'; re-encoding all chunks",
            path.display()
        );
        return HashMap::new();
    };
    match storage.load_all() {
        Ok(vectors) => vectors
            .into_iter()
            .map(|(sid, embedding)| (sid.to_u32(), embedding))
            .collect(),
        Err(err) => {
            tracing::warn!(
                target: "chunk_search",
                "failed to load existing chunk embeddings '{}': {}; re-encoding all chunks",
                path.display(),
                err
            );
            HashMap::new()
        }
    }
}


pub fn build_rerank_text(record: &CodeChunkRecord) -> String {
    const MAX_SIGNATURE_CHARS: usize = 256;
    const MAX_DOC_CHARS: usize = 384;
    const MAX_SNIPPET_CHARS: usize = 1200;

    let mut parts = Vec::with_capacity(8);
    parts.push(format!(
        "file: {}:{}-{}",
        record.file_path,
        record.line_start + 1,
        record.line_end + 1
    ));
    parts.push(format!("kind: {}", record.chunk_type));

    if let Some(scope) = record.parent_scope.as_deref() {
        if !scope.is_empty() {
            parts.push(format!("scope: {scope}"));
        }
    }

    if let Some(sig) = record.signature.as_deref() {
        parts.push(format!(
            "signature: {}",
            truncate_for_rerank(sig, MAX_SIGNATURE_CHARS)
        ));
    }

    if let Some(doc) = record.doc_comment.as_deref() {
        parts.push(format!("doc: {}", truncate_for_rerank(doc, MAX_DOC_CHARS)));
    }

    parts.push(format!(
        "snippet: {}",
        truncate_for_rerank(&record.snippet, MAX_SNIPPET_CHARS)
    ));

    parts.join("\n")
}

fn build_chunk_bm25_query(
    index: &Index,
    schema: &ChunkTantivySchema,
    query_str: &str,
    language_filter: Option<&str>,
) -> Result<Box<dyn Query>, IndexError> {
    const BOOST_SIGNATURE: f32 = 3.0;
    const BOOST_DOC_COMMENT: f32 = 2.2;
    const BOOST_SNIPPET: f32 = 1.2;
    const BOOST_FILE_PATH: f32 = 1.5;
    const BOOST_PARENT_SCOPE: f32 = 1.4;
    const BOOST_CHUNK_TYPE: f32 = 0.8;

    let mut parser = QueryParser::for_index(
        index,
        vec![
            schema.signature,
            schema.doc_comment,
            schema.snippet,
            schema.file_path,
            schema.parent_scope,
            schema.chunk_type,
        ],
    );
    parser.set_field_boost(schema.signature, BOOST_SIGNATURE);
    parser.set_field_boost(schema.doc_comment, BOOST_DOC_COMMENT);
    parser.set_field_boost(schema.snippet, BOOST_SNIPPET);
    parser.set_field_boost(schema.file_path, BOOST_FILE_PATH);
    parser.set_field_boost(schema.parent_scope, BOOST_PARENT_SCOPE);
    parser.set_field_boost(schema.chunk_type, BOOST_CHUNK_TYPE);

    let main_query = match parser.parse_query(query_str) {
        Ok(q) => q,
        Err(_) => {
            let signature_term = Term::from_field_text(schema.signature, query_str);
            let doc_term = Term::from_field_text(schema.doc_comment, query_str);
            let snippet_term = Term::from_field_text(schema.snippet, query_str);
            let path_term = Term::from_field_text(schema.file_path, query_str);
            let scope_term = Term::from_field_text(schema.parent_scope, query_str);

            Box::new(BooleanQuery::new(vec![
                (
                    Occur::Should,
                    Box::new(BoostQuery::new(
                        Box::new(TermQuery::new(signature_term, IndexRecordOption::Basic)),
                        BOOST_SIGNATURE,
                    )) as Box<dyn Query>,
                ),
                (
                    Occur::Should,
                    Box::new(BoostQuery::new(
                        Box::new(TermQuery::new(doc_term, IndexRecordOption::Basic)),
                        BOOST_DOC_COMMENT,
                    )) as Box<dyn Query>,
                ),
                (
                    Occur::Should,
                    Box::new(BoostQuery::new(
                        Box::new(TermQuery::new(snippet_term, IndexRecordOption::Basic)),
                        BOOST_SNIPPET,
                    )) as Box<dyn Query>,
                ),
                (
                    Occur::Should,
                    Box::new(BoostQuery::new(
                        Box::new(TermQuery::new(path_term, IndexRecordOption::Basic)),
                        BOOST_FILE_PATH,
                    )) as Box<dyn Query>,
                ),
                (
                    Occur::Should,
                    Box::new(BoostQuery::new(
                        Box::new(TermQuery::new(scope_term, IndexRecordOption::Basic)),
                        BOOST_PARENT_SCOPE,
                    )) as Box<dyn Query>,
                ),
            ]))
        }
    };

    // Mandatory filter: never mix non-chunk docs in BM25 results.
    let mut filters: Vec<(Occur, Box<dyn Query>)> = vec![
        (
            Occur::Must,
            Box::new(TermQuery::new(
                Term::from_field_text(schema.doc_type, "chunk"),
                IndexRecordOption::Basic,
            )) as Box<dyn Query>,
        ),
        (Occur::Must, main_query),
    ];
    if let Some(phrase_boost) = build_phrase_boost_query(schema, query_str) {
        filters.push((Occur::Should, phrase_boost));
    }

    if let Some(lang) = language_filter {
        filters.push((
            Occur::Must,
            Box::new(TermQuery::new(
                Term::from_field_text(schema.language, lang),
                IndexRecordOption::Basic,
            )) as Box<dyn Query>,
        ));
    }

    Ok(Box::new(BooleanQuery::new(filters)))
}

fn build_phrase_boost_query(
    schema: &ChunkTantivySchema,
    query_str: &str,
) -> Option<Box<dyn Query>> {
    const BOOST_PHRASE_SIGNATURE: f32 = 3.8;
    const BOOST_PHRASE_DOC: f32 = 3.0;
    const BOOST_PHRASE_SNIPPET: f32 = 2.2;

    let terms = query_str
        .split_whitespace()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if terms.len() < 2 {
        return None;
    }

    let to_field_terms = |field: Field| -> Vec<Term> {
        terms
            .iter()
            .map(|token| Term::from_field_text(field, token))
            .collect()
    };

    Some(Box::new(BooleanQuery::new(vec![
        (
            Occur::Should,
            Box::new(BoostQuery::new(
                Box::new(PhraseQuery::new(to_field_terms(schema.signature))),
                BOOST_PHRASE_SIGNATURE,
            )) as Box<dyn Query>,
        ),
        (
            Occur::Should,
            Box::new(BoostQuery::new(
                Box::new(PhraseQuery::new(to_field_terms(schema.doc_comment))),
                BOOST_PHRASE_DOC,
            )) as Box<dyn Query>,
        ),
        (
            Occur::Should,
            Box::new(BoostQuery::new(
                Box::new(PhraseQuery::new(to_field_terms(schema.snippet))),
                BOOST_PHRASE_SNIPPET,
            )) as Box<dyn Query>,
        ),
    ])))
}

fn write_tantivy_chunk_index(root: &Path, chunks: &[CodeChunkRecord]) -> Result<(), IndexError> {
    let tantivy_path = root.join("tantivy");
    let lock_path = root.join("tantivy.refresh.lock");
    let lock_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|e| {
            IndexError::General(format!(
                "failed to open chunk tantivy lock '{}': {e}",
                lock_path.display()
            ))
        })?;
    lock_file.lock_exclusive().map_err(|e| {
        IndexError::General(format!(
            "failed to acquire chunk tantivy lock '{}': {e}",
            lock_path.display()
        ))
    })?;

    let suffix = format!("{}-{}", std::process::id(), current_unix_time_secs());
    let tmp_path = root.join(format!("tantivy.tmp-{suffix}"));
    let backup_path = root.join(format!("tantivy.prev-{suffix}"));
    if tmp_path.exists() {
        let _ = std::fs::remove_dir_all(&tmp_path);
    }
    std::fs::create_dir_all(&tmp_path)?;

    let build_result: Result<(), IndexError> = (|| {
        let (schema, fields) = ChunkTantivySchema::build();
        let dir = MmapDirectory::open(&tmp_path).map_err(|e| {
            IndexError::General(format!(
                "failed to create temporary chunk tantivy directory '{}': {e}",
                tmp_path.display()
            ))
        })?;
        let index = Index::create(dir, schema, IndexSettings::default()).map_err(|e| {
            IndexError::General(format!(
                "failed to create temporary chunk tantivy index: {e}"
            ))
        })?;

        let mut writer = index.writer(50_000_000).map_err(|e| {
            IndexError::General(format!("failed to create chunk tantivy writer: {e}"))
        })?;

        for chunk in chunks {
            let mut doc = Document::new();
            doc.add_text(fields.doc_type, "chunk");
            doc.add_u64(fields.chunk_id, chunk.chunk_id as u64);
            doc.add_u64(fields.symbol_id, chunk.symbol_id as u64);
            doc.add_text(fields.file_path, &chunk.file_path);
            doc.add_text(
                fields.language,
                chunk.language.as_deref().unwrap_or("unknown"),
            );
            doc.add_text(fields.chunk_type, &chunk.chunk_type);
            if let Some(scope) = chunk.parent_scope.as_deref() {
                doc.add_text(fields.parent_scope, scope);
            }
            doc.add_u64(fields.line_start, chunk.line_start as u64);
            doc.add_u64(fields.line_end, chunk.line_end as u64);
            if let Some(signature) = chunk.signature.as_deref() {
                doc.add_text(fields.signature, signature);
            }
            if let Some(doc_comment) = chunk.doc_comment.as_deref() {
                doc.add_text(fields.doc_comment, doc_comment);
            }
            doc.add_text(fields.snippet, &chunk.snippet);

            writer.add_document(doc).map_err(|e| {
                IndexError::General(format!("failed to add chunk doc to tantivy: {e}"))
            })?;
        }

        writer.commit().map_err(|e| {
            IndexError::General(format!("failed to commit chunk tantivy index: {e}"))
        })?;

        // Wait for background merge threads to finish before renaming the directory.
        // Without this, merge threads may still reference temp files when rename() moves
        // the directory, causing "FileDoesNotExist" warnings.
        writer.wait_merging_threads().map_err(|e| {
            IndexError::General(format!("failed to wait for chunk tantivy merge threads: {e}"))
        })?;

        Ok(())
    })();

    if let Err(e) = build_result {
        let _ = std::fs::remove_dir_all(&tmp_path);
        return Err(e);
    }

    if tantivy_path.exists() {
        std::fs::rename(&tantivy_path, &backup_path).map_err(|e| {
            let _ = std::fs::remove_dir_all(&tmp_path);
            IndexError::General(format!(
                "failed to rotate old chunk tantivy index '{}': {e}",
                tantivy_path.display()
            ))
        })?;
    }

    if let Err(e) = std::fs::rename(&tmp_path, &tantivy_path) {
        let _ = std::fs::remove_dir_all(&tmp_path);
        if backup_path.exists() {
            let _ = std::fs::rename(&backup_path, &tantivy_path);
        }
        return Err(IndexError::General(format!(
            "failed to publish chunk tantivy index '{}': {e}",
            tantivy_path.display()
        )));
    }

    if backup_path.exists() {
        let _ = std::fs::remove_dir_all(&backup_path);
    }

    Ok(())
}

fn build_chunk_record(
    symbol: &Symbol,
    workspace_root: Option<&Path>,
    max_snippet_chars: usize,
    snippet_context_lines: usize,
    snippet_min_lines: usize,
    source_cache: &mut HashMap<PathBuf, Vec<String>>,
) -> Option<CodeChunkRecord> {
    let snippet = extract_symbol_snippet(
        symbol,
        workspace_root,
        max_snippet_chars,
        snippet_context_lines,
        snippet_min_lines,
        source_cache,
    )?;
    let chunk_type = kind_label(symbol.kind).to_string();
    let parent_scope = parent_scope_label(symbol.scope_context.as_ref());
    let signature = symbol.signature.as_ref().map(|s| s.to_string());
    let doc_comment = symbol.doc_comment.as_ref().map(|s| s.to_string());
    let embedding_text = build_embedding_text(
        symbol.file_path.as_ref(),
        parent_scope.as_deref(),
        &chunk_type,
        signature.as_deref().unwrap_or(symbol.name.as_ref()),
        doc_comment.as_deref(),
        &snippet,
    );

    Some(CodeChunkRecord {
        chunk_id: symbol.id.to_u32(),
        symbol_id: symbol.id.to_u32(),
        file_path: symbol.file_path.to_string(),
        language: symbol.language_id.as_ref().map(|l| l.as_str().to_string()),
        chunk_type,
        parent_scope,
        line_start: symbol.range.start_line,
        line_end: symbol.range.end_line,
        signature,
        doc_comment,
        snippet,
        embedding_text,
    })
}

fn append_flow_chunks(
    symbols: &[Symbol],
    settings: Option<&Settings>,
    workspace_root: Option<&Path>,
    max_snippet_chars: usize,
    flow_chunk_languages: &[String],
    flow_chunk_max_per_symbol: usize,
    source_cache: &mut HashMap<PathBuf, Vec<String>>,
    chunks: &mut Vec<CodeChunkRecord>,
    next_chunk_id: &mut u32,
) {
    let Some(settings) = settings else {
        return;
    };

    let mut by_file: HashMap<&str, Vec<&Symbol>> = HashMap::new();
    for symbol in symbols {
        by_file
            .entry(symbol.file_path.as_ref())
            .or_default()
            .push(symbol);
    }

    let allowed_langs: HashSet<String> = flow_chunk_languages
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    let allow_all = allowed_langs.is_empty();

    for (file_path, mut file_symbols) in by_file {
        if file_symbols.is_empty() {
            continue;
        }
        file_symbols.sort_by_key(|s| (s.range.start_line, s.range.end_line));
        let lang = file_symbols[0]
            .language_id
            .as_ref()
            .map(|id| id.as_str().to_ascii_lowercase());
        if !allow_all && !lang.as_ref().is_some_and(|l| allowed_langs.contains(l)) {
            continue;
        }

        let resolved = resolve_symbol_path(file_path, workspace_root);
        let Some(lines) = load_source_lines(&resolved, source_cache) else {
            continue;
        };
        if lines.is_empty() {
            continue;
        }
        let source = lines.join("\n");

        let mut parser = {
            let registry = get_registry();
            let Ok(registry) = registry.lock() else {
                continue;
            };
            let language_id = lang
                .as_deref()
                .and_then(|name| registry.find_language_id(name))
                .or_else(|| {
                    resolved
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .and_then(|ext| registry.get_by_extension(ext).map(|def| def.id()))
                });
            let Some(language_id) = language_id else {
                continue;
            };
            let Ok(parser) = registry.create_parser(language_id, settings) else {
                continue;
            };
            parser
        };

        let flow_blocks = parser.find_flow_blocks(&source);
        if flow_blocks.is_empty() {
            continue;
        }

        let mut per_symbol_count: HashMap<u32, usize> = HashMap::new();
        let max_per_symbol = flow_chunk_max_per_symbol.max(1);

        for block in flow_blocks {
            let Some((start, end)) =
                normalize_line_range(block.range.start_line, block.range.end_line, lines.len())
            else {
                continue;
            };
            let snippet = truncate_chars(&lines[start..=end].join("\n"), max_snippet_chars);
            if snippet.trim().is_empty() {
                continue;
            }

            let parent = find_parent_symbol_for_block(
                &file_symbols,
                start,
                end,
                block.parent_symbol_name.as_deref(),
            );
            let symbol_id = parent.map(|s| s.id.to_u32()).unwrap_or(0);
            if symbol_id != 0 {
                let used = per_symbol_count.entry(symbol_id).or_insert(0);
                if *used >= max_per_symbol {
                    continue;
                }
                *used += 1;
            }
            let parent_scope = parent
                .map(|s| s.name.as_ref().to_string())
                .or_else(|| block.parent_symbol_name.clone());
            let signature = parent.and_then(|s| s.signature.as_ref().map(|sig| sig.to_string()));
            let label = block
                .label
                .clone()
                .unwrap_or_else(|| flow_kind_label(block.kind).to_string());
            let chunk_type = flow_kind_chunk_type(block.kind).to_string();
            let embedding_text = build_embedding_text(
                file_path,
                parent_scope.as_deref(),
                &chunk_type,
                &label,
                None,
                &snippet,
            );
            chunks.push(CodeChunkRecord {
                chunk_id: *next_chunk_id,
                symbol_id,
                file_path: file_path.to_string(),
                language: lang.clone(),
                chunk_type,
                parent_scope,
                line_start: start as u32,
                line_end: end as u32,
                signature,
                doc_comment: None,
                snippet,
                embedding_text,
            });
            *next_chunk_id = next_chunk_id.saturating_add(1);
        }
    }
}

fn flow_kind_chunk_type(kind: FlowKind) -> &'static str {
    match kind {
        FlowKind::IfElse => "flow_if_else",
        FlowKind::TryCatch => "flow_try_catch",
        FlowKind::Switch => "flow_switch",
        FlowKind::Loop => "flow_loop",
        FlowKind::CallChain => "flow_call_chain",
        FlowKind::ErrorPath => "flow_error_path",
    }
}

fn flow_kind_label(kind: FlowKind) -> &'static str {
    match kind {
        FlowKind::IfElse => "if/else flow",
        FlowKind::TryCatch => "try/catch flow",
        FlowKind::Switch => "switch/match flow",
        FlowKind::Loop => "loop flow",
        FlowKind::CallChain => "call chain",
        FlowKind::ErrorPath => "error path",
    }
}

fn find_parent_symbol_for_block<'a>(
    symbols: &[&'a Symbol],
    start: usize,
    end: usize,
    parent_name_hint: Option<&str>,
) -> Option<&'a Symbol> {
    let contains = |s: &Symbol| {
        normalize_line_range(s.range.start_line, s.range.end_line, usize::MAX)
            .map(|(s0, e0)| s0 <= start && end <= e0)
            .unwrap_or(false)
    };
    if let Some(name_hint) = parent_name_hint {
        let by_name = symbols
            .iter()
            .copied()
            .filter(|s| s.name.as_ref() == name_hint && contains(s))
            .min_by_key(|s| s.range.end_line.saturating_sub(s.range.start_line));
        if by_name.is_some() {
            return by_name;
        }
    }
    symbols
        .iter()
        .copied()
        .filter(|s| contains(s))
        .min_by_key(|s| s.range.end_line.saturating_sub(s.range.start_line))
}

fn dedup_chunk_records(chunks: &mut Vec<CodeChunkRecord>) {
    if chunks.is_empty() {
        return;
    }
    let mut seen: HashSet<(String, u32, u32, String)> = HashSet::with_capacity(chunks.len());
    chunks.retain(|chunk| {
        seen.insert((
            chunk.file_path.clone(),
            chunk.line_start,
            chunk.line_end,
            chunk.chunk_type.clone(),
        ))
    });
}

fn apply_token_budget_to_chunks(
    chunks: &mut Vec<CodeChunkRecord>,
    next_chunk_id: &mut u32,
    max_snippet_chars: usize,
    target_tokens: usize,
    max_tokens: usize,
    overlap_tokens: usize,
) {
    if chunks.is_empty() {
        return;
    }
    let target = target_tokens.max(64);
    let max = max_tokens.max(target);
    let overlap = overlap_tokens.min(max / 2);

    let mut out = Vec::with_capacity(chunks.len());
    for chunk in chunks.iter() {
        let split = split_chunk_by_token_budget(
            chunk,
            next_chunk_id,
            max_snippet_chars,
            target,
            max,
            overlap,
        );
        out.extend(split);
    }
    *chunks = out;
}

fn split_chunk_by_token_budget(
    chunk: &CodeChunkRecord,
    next_chunk_id: &mut u32,
    max_snippet_chars: usize,
    target_tokens: usize,
    max_tokens: usize,
    overlap_tokens: usize,
) -> Vec<CodeChunkRecord> {
    let total_tokens = approx_token_count(&chunk.snippet);
    if total_tokens <= max_tokens {
        return vec![chunk.clone()];
    }

    let lines: Vec<&str> = chunk.snippet.lines().collect();
    if lines.is_empty() {
        return vec![chunk.clone()];
    }

    let mut spans: Vec<(usize, usize, String)> = Vec::new();
    let mut start = 0usize;
    while start < lines.len() {
        let mut end = start;
        let mut tokens = 0usize;
        while end < lines.len() {
            let line_tokens = approx_token_count(lines[end]);
            if end > start && tokens.saturating_add(line_tokens) > max_tokens {
                break;
            }
            tokens = tokens.saturating_add(line_tokens);
            end += 1;
        }
        if end == start {
            end = (start + 1).min(lines.len());
        }
        let end_inclusive = end.saturating_sub(1);
        let snippet = lines[start..end].join("\n");
        spans.push((start, end_inclusive, snippet));

        if end >= lines.len() {
            break;
        }
        let mut back = end;
        let mut overlap = 0usize;
        while back > start {
            let t = approx_token_count(lines[back - 1]);
            if overlap.saturating_add(t) > overlap_tokens {
                break;
            }
            overlap = overlap.saturating_add(t);
            back -= 1;
        }
        start = back.max(start + 1).min(end);
    }

    // Merge tiny adjacent segments when possible.
    let min_tokens = (target_tokens / 3).max(24);
    let mut merged: Vec<(usize, usize, String)> = Vec::new();
    let mut i = 0usize;
    while i < spans.len() {
        let (s, mut e, mut snippet) = spans[i].clone();
        let mut tok = approx_token_count(&snippet);
        while tok < min_tokens && i + 1 < spans.len() {
            let candidate = &spans[i + 1];
            let combined = format!("{snippet}\n{}", candidate.2);
            let combined_tokens = approx_token_count(&combined);
            if combined_tokens > max_tokens {
                break;
            }
            e = candidate.1;
            snippet = combined;
            tok = combined_tokens;
            i += 1;
        }
        merged.push((s, e, snippet));
        i += 1;
    }

    let base_line = chunk.line_start as usize;
    let mut out = Vec::with_capacity(merged.len());
    for (idx, (start_off, end_off, snippet_raw)) in merged.into_iter().enumerate() {
        let snippet = truncate_chars(&snippet_raw, max_snippet_chars);
        if snippet.trim().is_empty() {
            continue;
        }
        let signature_or_name = chunk
            .signature
            .as_deref()
            .or(chunk.parent_scope.as_deref())
            .unwrap_or("chunk");
        let embedding_text = build_embedding_text(
            &chunk.file_path,
            chunk.parent_scope.as_deref(),
            &chunk.chunk_type,
            signature_or_name,
            chunk.doc_comment.as_deref(),
            &snippet,
        );

        let line_start = base_line.saturating_add(start_off);
        let line_end = base_line
            .saturating_add(end_off)
            .min(chunk.line_end as usize);
        out.push(CodeChunkRecord {
            chunk_id: if idx == 0 {
                chunk.chunk_id
            } else {
                let id = *next_chunk_id;
                *next_chunk_id = next_chunk_id.saturating_add(1);
                id
            },
            symbol_id: chunk.symbol_id,
            file_path: chunk.file_path.clone(),
            language: chunk.language.clone(),
            chunk_type: chunk.chunk_type.clone(),
            parent_scope: chunk.parent_scope.clone(),
            line_start: line_start as u32,
            line_end: line_end as u32,
            signature: chunk.signature.clone(),
            doc_comment: chunk.doc_comment.clone(),
            snippet,
            embedding_text,
        });
    }
    if out.is_empty() {
        vec![chunk.clone()]
    } else {
        out
    }
}

fn approx_token_count(text: &str) -> usize {
    text.split_whitespace().count().max(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MixedFileKind {
    Doc,
    Config,
}

fn append_mixed_chunks(
    symbols: &[Symbol],
    indexed_paths: Option<&[PathBuf]>,
    workspace_root: Option<&Path>,
    max_snippet_chars: usize,
    source_cache: &mut HashMap<PathBuf, Vec<String>>,
    chunks: &mut Vec<CodeChunkRecord>,
    next_chunk_id: &mut u32,
) {
    let t = Instant::now();
    let mut by_file: HashMap<&str, Vec<&Symbol>> = HashMap::new();
    for symbol in symbols {
        by_file
            .entry(symbol.file_path.as_ref())
            .or_default()
            .push(symbol);
    }

    for (file_path, mut file_symbols) in by_file {
        file_symbols.sort_by_key(|s| (s.range.start_line, s.range.end_line));
        append_module_comment_chunk(
            file_path,
            &file_symbols,
            workspace_root,
            max_snippet_chars,
            source_cache,
            chunks,
            next_chunk_id,
        );
        append_inter_symbol_gap_chunks(
            file_path,
            &file_symbols,
            workspace_root,
            max_snippet_chars,
            source_cache,
            chunks,
            next_chunk_id,
        );
    }
    tracing::info!(
        target: "pipeline",
        "CHUNK MIXED sub: module_comment+gap done in {:.1}s, {} chunks so far",
        t.elapsed().as_secs_f64(), chunks.len()
    );

    let t2 = Instant::now();
    append_config_doc_chunks(
        indexed_paths,
        workspace_root,
        max_snippet_chars,
        source_cache,
        chunks,
        next_chunk_id,
    );
    tracing::info!(
        target: "pipeline",
        "CHUNK MIXED sub: config_doc_chunks done in {:.1}s, {} chunks total",
        t2.elapsed().as_secs_f64(), chunks.len()
    );
}

fn append_module_comment_chunk(
    file_path: &str,
    symbols_in_file: &[&Symbol],
    workspace_root: Option<&Path>,
    max_snippet_chars: usize,
    source_cache: &mut HashMap<PathBuf, Vec<String>>,
    chunks: &mut Vec<CodeChunkRecord>,
    next_chunk_id: &mut u32,
) {
    let resolved = resolve_symbol_path(file_path, workspace_root);
    let Some(lines) = load_source_lines(&resolved, source_cache) else {
        return;
    };
    let Some((start_line, end_line, snippet)) =
        extract_leading_module_comment(lines, symbols_in_file, max_snippet_chars)
    else {
        return;
    };
    if snippet.trim().is_empty() {
        return;
    }

    let embedding_text = build_embedding_text(
        file_path,
        None,
        "module_comment",
        "file_header",
        None,
        &snippet,
    );
    chunks.push(CodeChunkRecord {
        chunk_id: *next_chunk_id,
        symbol_id: 0,
        file_path: file_path.to_string(),
        language: symbols_in_file
            .first()
            .and_then(|s| s.language_id.as_ref().map(|id| id.as_str().to_string())),
        chunk_type: "module_comment".to_string(),
        parent_scope: None,
        line_start: start_line as u32,
        line_end: end_line as u32,
        signature: None,
        doc_comment: None,
        snippet,
        embedding_text,
    });
    *next_chunk_id = next_chunk_id.saturating_add(1);
}

fn append_inter_symbol_gap_chunks(
    file_path: &str,
    symbols_in_file: &[&Symbol],
    workspace_root: Option<&Path>,
    max_snippet_chars: usize,
    source_cache: &mut HashMap<PathBuf, Vec<String>>,
    chunks: &mut Vec<CodeChunkRecord>,
    next_chunk_id: &mut u32,
) {
    if symbols_in_file.len() < 2 {
        return;
    }
    let resolved = resolve_symbol_path(file_path, workspace_root);
    let Some(lines) = load_source_lines(&resolved, source_cache) else {
        return;
    };
    if lines.is_empty() {
        return;
    }

    let mut ranges = Vec::with_capacity(symbols_in_file.len());
    for symbol in symbols_in_file {
        if let Some((start, end)) = normalize_symbol_line_range(symbol, lines.len()) {
            ranges.push((start, end));
        }
    }
    if ranges.len() < 2 {
        return;
    }
    ranges.sort_unstable_by_key(|(s, e)| (*s, *e));

    for pair in ranges.windows(2) {
        let prev = pair[0];
        let next = pair[1];
        if next.0 <= prev.1.saturating_add(1) {
            continue;
        }
        let gap_start = prev.1.saturating_add(1);
        let gap_end = next.0.saturating_sub(1);
        if gap_end < gap_start || gap_end >= lines.len() {
            continue;
        }
        let snippet = truncate_chars(&lines[gap_start..=gap_end].join("\n"), max_snippet_chars);
        if snippet.trim().is_empty() {
            continue;
        }
        // Skip trivial separators that are mostly punctuation.
        let meaningful_chars = snippet.chars().filter(|c| c.is_alphanumeric()).count();
        if meaningful_chars < 6 {
            continue;
        }

        let chunk_type = if snippet
            .lines()
            .map(str::trim_start)
            .filter(|l| !l.is_empty())
            .all(|l| {
                l.starts_with("import ")
                    || l.starts_with("from ")
                    || l.starts_with("export ")
                    || l.starts_with("use ")
                    || l.starts_with("#include")
            }) {
            "imports_gap"
        } else {
            "inter_symbol_gap"
        };

        let embedding_text = build_embedding_text(
            file_path,
            None,
            chunk_type,
            &format!("lines {}-{}", gap_start + 1, gap_end + 1),
            None,
            &snippet,
        );
        chunks.push(CodeChunkRecord {
            chunk_id: *next_chunk_id,
            symbol_id: 0,
            file_path: file_path.to_string(),
            language: symbols_in_file
                .first()
                .and_then(|s| s.language_id.as_ref().map(|id| id.as_str().to_string())),
            chunk_type: chunk_type.to_string(),
            parent_scope: None,
            line_start: gap_start as u32,
            line_end: gap_end as u32,
            signature: None,
            doc_comment: None,
            snippet,
            embedding_text,
        });
        *next_chunk_id = next_chunk_id.saturating_add(1);
    }
}

fn append_config_doc_chunks(
    indexed_paths: Option<&[PathBuf]>,
    workspace_root: Option<&Path>,
    max_snippet_chars: usize,
    source_cache: &mut HashMap<PathBuf, Vec<String>>,
    chunks: &mut Vec<CodeChunkRecord>,
    next_chunk_id: &mut u32,
) {
    const MAX_CHUNKS_PER_FILE: usize = 24;
    const OVERLAP_LINES: usize = 2;
    const MIN_DOC_CHUNK_CHARS: usize = 80;
    const MIN_CONFIG_CHUNK_CHARS: usize = 20;
    const MAX_WORKSPACE_TEXT_FILES: usize = 2_000;
    const MAX_TEXT_FILE_BYTES: u64 = 512 * 1024;

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(indexed_paths) = indexed_paths {
        candidates.extend_from_slice(indexed_paths);
    }
    let indexed_count = candidates.len();

    // Also include non-symbol text/config files directly from workspace for mixed chunking.
    if let Some(root) = workspace_root {
        let walker = walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                !(name == ".git"
                    || name == ".codanna"
                    || name == "node_modules"
                    || name == "target"
                    || name == "dist"
                    || name == "build"
                    || name == ".next"
                    || name == ".turbo"
                    || name == "coverage")
            });
        for entry in walker.filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            if classify_mixed_file_kind(entry.path()).is_none() {
                continue;
            }
            if entry.metadata().map(|m| m.len()).unwrap_or(0) > MAX_TEXT_FILE_BYTES {
                continue;
            }
            candidates.push(entry.path().to_path_buf());
            if candidates.len() >= MAX_WORKSPACE_TEXT_FILES {
                break;
            }
        }
    }
    tracing::info!(
        target: "pipeline",
        "CHUNK config_doc: {} indexed + {} walkdir = {} candidates",
        indexed_count, candidates.len() - indexed_count, candidates.len()
    );

    let mut seen_resolved = HashSet::new();
    let mut matched = 0usize;
    let total_candidates = candidates.len();
    let loop_start = Instant::now();
    for raw_path in candidates {
        let Some(kind) = classify_mixed_file_kind(&raw_path) else {
            continue;
        };
        let resolved = resolve_indexed_path(&raw_path, workspace_root);
        if !seen_resolved.insert(resolved.clone()) {
            continue;
        }
        matched += 1;
        if matched > 85 && matched < 105 {
            tracing::info!(
                target: "pipeline",
                "CHUNK config_doc BEFORE load #{}: {:?} ({:.1}s)",
                matched, resolved.file_name().unwrap_or_default(), loop_start.elapsed().as_secs_f64()
            );
        }
        let Some(lines) = load_source_lines(&resolved, source_cache) else {
            continue;
        };
        if lines.is_empty() {
            continue;
        }
        if matched % 10 == 0 || matched <= 5 {
            tracing::info!(
                target: "pipeline",
                "CHUNK config_doc progress: {}/{} matched in {:.1}s — {:?} ({} lines)",
                matched, total_candidates, loop_start.elapsed().as_secs_f64(),
                resolved.file_name().unwrap_or_default(), lines.len()
            );
        }

        let file_path = normalize_chunk_file_path(&raw_path, workspace_root);
        let language = detect_text_language(&raw_path);
        let split_t = Instant::now();
        let chunks_for_file = split_text_file_chunks(
            lines,
            max_snippet_chars.max(256),
            OVERLAP_LINES,
            match kind {
                MixedFileKind::Doc => MIN_DOC_CHUNK_CHARS,
                MixedFileKind::Config => MIN_CONFIG_CHUNK_CHARS,
            },
            MAX_CHUNKS_PER_FILE,
        );
        let split_dur = split_t.elapsed();
        if split_dur.as_millis() > 100 {
            tracing::warn!(
                target: "pipeline",
                "CHUNK config_doc SLOW split: {:?} took {:.1}s ({} lines, {} produced)",
                resolved.file_name().unwrap_or_default(),
                split_dur.as_secs_f64(), lines.len(), chunks_for_file.len()
            );
        }
        let chunk_type = match kind {
            MixedFileKind::Doc => "doc_chunk",
            MixedFileKind::Config => "config_chunk",
        };

        for (line_start, line_end, snippet) in chunks_for_file {
            let title = snippet
                .lines()
                .find(|l| !l.trim().is_empty())
                .map(|l| truncate_chars(l.trim(), 96))
                .unwrap_or_else(|| "content".to_string());
            let embedding_text =
                build_embedding_text(&file_path, None, chunk_type, &title, None, &snippet);
            chunks.push(CodeChunkRecord {
                chunk_id: *next_chunk_id,
                symbol_id: 0,
                file_path: file_path.clone(),
                language: language.clone(),
                chunk_type: chunk_type.to_string(),
                parent_scope: None,
                line_start: line_start as u32,
                line_end: line_end as u32,
                signature: None,
                doc_comment: None,
                snippet,
                embedding_text,
            });
            *next_chunk_id = next_chunk_id.saturating_add(1);
        }
    }
    tracing::info!(
        target: "pipeline",
        "CHUNK config_doc: {} matched from {} candidates, {} chunks added",
        matched, total_candidates, chunks.len()
    );
}

fn resolve_indexed_path(path: &Path, workspace_root: Option<&Path>) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    if let Some(root) = workspace_root {
        let joined = root.join(path);
        if joined.exists() {
            return joined;
        }
    }
    path.to_path_buf()
}

fn normalize_chunk_file_path(path: &Path, workspace_root: Option<&Path>) -> String {
    if let Some(root) = workspace_root {
        let resolved = resolve_indexed_path(path, Some(root));
        if let Ok(rel) = resolved.strip_prefix(root) {
            let rel_str = rel.to_string_lossy();
            if rel_str.starts_with("./") {
                return rel_str.into_owned();
            }
            return format!("./{rel_str}");
        }
    }
    path.to_string_lossy().to_string()
}

fn load_source_lines<'a>(
    path: &Path,
    source_cache: &'a mut HashMap<PathBuf, Vec<String>>,
) -> Option<&'a Vec<String>> {
    if !source_cache.contains_key(path) {
        let source = std::fs::read_to_string(path).ok()?;
        source_cache.insert(
            path.to_path_buf(),
            source.lines().map(|s| s.to_string()).collect(),
        );
    }
    source_cache.get(path)
}

fn extract_leading_module_comment(
    lines: &[String],
    symbols_in_file: &[&Symbol],
    max_snippet_chars: usize,
) -> Option<(usize, usize, String)> {
    if lines.is_empty() {
        return None;
    }
    let first_symbol_start = symbols_in_file
        .iter()
        .map(|s| s.range.start_line as usize)
        .min()
        .unwrap_or(lines.len())
        .min(lines.len());
    if first_symbol_start == 0 {
        return None;
    }

    let mut start = 0usize;
    if lines
        .first()
        .is_some_and(|l| l.trim_start().starts_with("#!"))
        && first_symbol_start > 1
    {
        start = 1;
    }

    let mut i = start;
    let mut saw_comment = false;
    let mut in_block = false;
    let mut end = start;

    while i < first_symbol_start {
        let line = lines[i].trim();
        if line.is_empty() {
            if saw_comment {
                end = i;
                i += 1;
                continue;
            }
            i += 1;
            continue;
        }

        let (is_comment_like, starts_block, ends_block) = is_comment_like_line(line, in_block);
        if !is_comment_like {
            break;
        }
        saw_comment = true;
        end = i;
        if starts_block {
            in_block = true;
        }
        if ends_block {
            in_block = false;
        }
        i += 1;
    }

    if !saw_comment || end < start {
        return None;
    }
    let snippet = truncate_chars(&lines[start..=end].join("\n"), max_snippet_chars);
    Some((start, end, snippet))
}

fn is_comment_like_line(line: &str, in_block: bool) -> (bool, bool, bool) {
    if in_block {
        return (
            true,
            false,
            line.contains("*/") || line.contains("\"\"\"") || line.contains("'''"),
        );
    }
    if line.starts_with("//")
        || line.starts_with('#')
        || line.starts_with(';')
        || line.starts_with('*')
        || line.starts_with("<!--")
    {
        return (true, line.starts_with("<!--"), line.contains("-->"));
    }
    if line.starts_with("/*") {
        return (true, true, line.contains("*/"));
    }
    if line.starts_with("\"\"\"") || line.starts_with("'''") {
        let closed = line.matches("\"\"\"").count() >= 2 || line.matches("'''").count() >= 2;
        return (true, !closed, closed);
    }
    (false, false, false)
}

fn normalize_symbol_line_range(symbol: &Symbol, line_count: usize) -> Option<(usize, usize)> {
    normalize_line_range(symbol.range.start_line, symbol.range.end_line, line_count)
}

fn normalize_line_range(
    start_line: u32,
    end_line: u32,
    line_count: usize,
) -> Option<(usize, usize)> {
    if line_count == usize::MAX {
        let start = start_line as usize;
        let end = (end_line as usize).max(start);
        return Some((start, end));
    }
    if line_count == 0 {
        return None;
    }
    let mut start = start_line as usize;
    let mut end = end_line as usize;
    if start >= line_count && start > 0 {
        start = start.saturating_sub(1);
    }
    if end >= line_count && end > 0 {
        end = end.saturating_sub(1);
    }
    if start >= line_count {
        return None;
    }
    end = end.min(line_count.saturating_sub(1));
    if end < start {
        end = start;
    }
    Some((start, end))
}

fn split_text_file_chunks(
    lines: &[String],
    max_chunk_chars: usize,
    overlap_lines: usize,
    min_chunk_chars: usize,
    max_chunks: usize,
) -> Vec<(usize, usize, String)> {
    if lines.is_empty() || max_chunk_chars == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0usize;

    while start < lines.len() && out.len() < max_chunks {
        let mut end = start;
        let mut chars = 0usize;
        while end < lines.len() {
            let line_len = lines[end].chars().count().saturating_add(1);
            if end > start && chars.saturating_add(line_len) > max_chunk_chars {
                break;
            }
            chars = chars.saturating_add(line_len);
            end += 1;
        }
        if end == start {
            end = (start + 1).min(lines.len());
        }
        let end_inclusive = end.saturating_sub(1);
        let snippet = lines[start..end].join("\n");
        if snippet.trim().chars().count() >= min_chunk_chars {
            out.push((start, end_inclusive, snippet));
        }
        if end >= lines.len() {
            break;
        }
        let prev_start = start;
        start = end.saturating_sub(overlap_lines.min(end.saturating_sub(start)));
        // Guarantee forward progress: start must always advance by at least 1.
        if start <= prev_start {
            start = prev_start + 1;
        }
    }
    out
}

fn classify_mixed_file_kind(path: &Path) -> Option<MixedFileKind> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_ascii_lowercase())?;
    if file_name.starts_with(".env") {
        return Some(MixedFileKind::Config);
    }
    if file_name == "dockerfile" || file_name == "makefile" {
        return Some(MixedFileKind::Config);
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());
    match ext.as_deref() {
        Some("md" | "mdx" | "txt" | "rst" | "adoc") => Some(MixedFileKind::Doc),
        Some(
            "toml" | "yaml" | "yml" | "json" | "jsonc" | "ini" | "cfg" | "conf" | "properties",
        ) => Some(MixedFileKind::Config),
        _ => None,
    }
}

fn detect_text_language(path: &Path) -> Option<String> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let lower_name = file_name.to_ascii_lowercase();
    if lower_name.starts_with(".env") {
        return Some("env".to_string());
    }
    if lower_name == "dockerfile" {
        return Some("dockerfile".to_string());
    }
    if lower_name == "makefile" {
        return Some("makefile".to_string());
    }

    let ext = path.extension().and_then(|e| e.to_str())?;
    Some(
        match ext.to_ascii_lowercase().as_str() {
            "md" | "mdx" => "markdown",
            "rst" => "rst",
            "adoc" => "asciidoc",
            "txt" => "text",
            "toml" => "toml",
            "yaml" | "yml" => "yaml",
            "json" | "jsonc" => "json",
            "ini" | "cfg" | "conf" | "properties" => "config",
            other => other,
        }
        .to_string(),
    )
}

fn resolve_symbol_path(file_path: &str, workspace_root: Option<&Path>) -> PathBuf {
    let path = PathBuf::from(file_path);
    if path.is_absolute() {
        return path;
    }
    if let Some(root) = workspace_root {
        let joined = root.join(&path);
        if joined.exists() {
            return joined;
        }
    }
    path
}

fn extract_symbol_snippet(
    symbol: &Symbol,
    workspace_root: Option<&Path>,
    max_snippet_chars: usize,
    snippet_context_lines: usize,
    snippet_min_lines: usize,
    source_cache: &mut HashMap<PathBuf, Vec<String>>,
) -> Option<String> {
    let resolved = resolve_symbol_path(symbol.file_path.as_ref(), workspace_root);
    if !source_cache.contains_key(&resolved) {
        let source = std::fs::read_to_string(&resolved).ok()?;
        source_cache.insert(
            resolved.clone(),
            source.lines().map(|s| s.to_string()).collect(),
        );
    }
    let lines = source_cache.get(&resolved)?;
    if lines.is_empty() {
        return None;
    }

    let len = lines.len();
    let mut start = symbol.range.start_line as usize;
    let mut end = symbol.range.end_line as usize;

    // Handle both 0-based and 1-based ranges defensively.
    if start >= len && start > 0 {
        start = start.saturating_sub(1);
    }
    if end >= len && end > 0 {
        end = end.saturating_sub(1);
    }
    if start >= len {
        return None;
    }
    end = end.min(len.saturating_sub(1));
    if end < start {
        end = start;
    }

    if snippet_context_lines > 0 {
        start = start.saturating_sub(snippet_context_lines);
        end = end
            .saturating_add(snippet_context_lines)
            .min(len.saturating_sub(1));
    }

    let target_min_lines = snippet_min_lines.max(1);
    while end.saturating_sub(start).saturating_add(1) < target_min_lines {
        if start == 0 && end == len.saturating_sub(1) {
            break;
        }
        if start > 0 {
            start -= 1;
        }
        if end < len.saturating_sub(1)
            && end.saturating_sub(start).saturating_add(1) < target_min_lines
        {
            end += 1;
        }
    }

    let snippet = lines[start..=end].join("\n");
    if snippet.trim().is_empty() {
        return None;
    }
    Some(truncate_chars(&snippet, max_snippet_chars))
}

fn build_embedding_text(
    file_path: &str,
    parent_scope: Option<&str>,
    kind: &str,
    signature_or_name: &str,
    doc_comment: Option<&str>,
    snippet: &str,
) -> String {
    let mut out = String::with_capacity(snippet.len() + 256);
    out.push_str("# ");
    out.push_str(file_path);
    out.push('\n');
    if let Some(scope) = parent_scope {
        out.push_str("# Scope: ");
        out.push_str(scope);
        out.push('\n');
    }
    out.push_str("# ");
    out.push_str(kind);
    out.push(' ');
    out.push_str(signature_or_name);
    out.push('\n');
    if let Some(doc) = doc_comment {
        out.push_str(doc);
        out.push('\n');
    }
    out.push_str(snippet);
    out
}

fn parent_scope_label(scope: Option<&ScopeContext>) -> Option<String> {
    match scope {
        Some(ScopeContext::ClassMember { class_name }) => {
            class_name.as_ref().map(|s| s.to_string())
        }
        Some(ScopeContext::Local { parent_name, .. }) => {
            parent_name.as_ref().map(|s| s.to_string())
        }
        Some(ScopeContext::Parameter) => Some("parameter".to_string()),
        _ => None,
    }
}

fn kind_label(kind: SymbolKind) -> &'static str {
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
        SymbolKind::TypeAlias => "type_alias",
        SymbolKind::Macro => "macro",
    }
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect()
}

fn truncate_for_rerank(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect()
}

fn current_unix_time_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Cached Tantivy chunk reader for BM25 search and stored-field lookups.
///
/// Holds an open Tantivy `Index` + `IndexReader` + schema so that
/// `hybrid_chunk_search_detailed` does not re-open the index on every query.
/// Also provides `lookup_records()` to fetch `CodeChunkRecord` by chunk_id
/// directly from Tantivy stored fields — eliminating the 469 MB `chunks.json`
/// parse that `load_chunk_record_map()` currently performs every call.
pub struct ChunkSearchBackend {
    index: Index,
    reader: IndexReader,
    schema: ChunkTantivySchema,
}

impl ChunkSearchBackend {
    /// Open the chunk Tantivy index at `<root>/tantivy`.
    pub fn open(root: &Path) -> Result<Self, IndexError> {
        let tantivy_path = root.join("tantivy");
        if !tantivy_path.join("meta.json").exists() {
            return Err(IndexError::General(format!(
                "chunk tantivy index not found at '{}'",
                tantivy_path.display()
            )));
        }

        let index = Index::open_in_dir(&tantivy_path).map_err(|e| {
            IndexError::General(format!(
                "failed to open chunk tantivy index '{}': {e}",
                tantivy_path.display()
            ))
        })?;
        let schema = ChunkTantivySchema::from_schema(&index.schema())?;

        let reader: IndexReader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(|e| {
                IndexError::General(format!("failed to create chunk tantivy reader: {e}"))
            })?;
        reader.reload().map_err(|e| {
            IndexError::General(format!("failed to reload chunk tantivy reader: {e}"))
        })?;

        Ok(Self {
            index,
            reader,
            schema,
        })
    }

    /// BM25 search returning `(chunk_id, score)` pairs.
    pub fn bm25_search(
        &self,
        query: &str,
        limit: usize,
        language_filter: Option<&str>,
    ) -> Result<Vec<(u32, f32)>, IndexError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let searcher = self.reader.searcher();
        let query_obj =
            build_chunk_bm25_query(&self.index, &self.schema, query, language_filter)?;
        let top_docs = searcher
            .search(&query_obj, &TopDocs::with_limit(limit))
            .map_err(|e| IndexError::General(format!("chunk BM25 search failed: {e}")))?;

        let mut results = Vec::with_capacity(top_docs.len());
        for (score, addr) in top_docs {
            let doc = searcher
                .doc::<Document>(addr)
                .map_err(|e| IndexError::General(format!("chunk BM25 doc load failed: {e}")))?;
            if let Some(chunk_id) = doc.get_first(self.schema.chunk_id).and_then(|v| v.as_u64()) {
                results.push((chunk_id as u32, score));
            }
        }
        Ok(results)
    }

    /// Fetch `CodeChunkRecord`s for a batch of chunk IDs from Tantivy stored fields.
    ///
    /// Returns a `HashMap` keyed by chunk_id. Missing IDs are silently skipped.
    pub fn lookup_records(
        &self,
        chunk_ids: &[u32],
    ) -> Result<HashMap<u32, CodeChunkRecord>, IndexError> {
        if chunk_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let searcher = self.reader.searcher();
        let s = &self.schema;
        let mut out = HashMap::with_capacity(chunk_ids.len());

        for &cid in chunk_ids {
            let term = Term::from_field_u64(s.chunk_id, cid as u64);
            let query = TermQuery::new(term, tantivy::schema::IndexRecordOption::Basic);
            let top = searcher
                .search(&query, &TopDocs::with_limit(1))
                .map_err(|e| {
                    IndexError::General(format!("chunk lookup failed for id={cid}: {e}"))
                })?;

            if let Some((_score, addr)) = top.first() {
                let doc = searcher.doc::<Document>(*addr).map_err(|e| {
                    IndexError::General(format!("chunk doc load failed for id={cid}: {e}"))
                })?;
                let record = self.doc_to_record(&doc);
                out.insert(cid, record);
            }
        }
        Ok(out)
    }

    /// Load ALL chunk records from Tantivy stored fields.
    ///
    /// Replaces `load_chunk_records()` which parsed `chunks.json` (469 MB).
    /// Tantivy stored-field scan of ~90 MB is faster than JSON parse.
    pub fn load_all_records(&self) -> Result<Vec<CodeChunkRecord>, IndexError> {
        let searcher = self.reader.searcher();
        let mut records = Vec::new();

        for segment_reader in searcher.segment_readers() {
            let store_reader = segment_reader.get_store_reader(1024).map_err(|e| {
                IndexError::General(format!("chunk stored-field reader failed: {e}"))
            })?;
            for doc_id in 0..segment_reader.num_docs() {
                let doc = store_reader.get::<Document>(doc_id).map_err(|e| {
                    IndexError::General(format!("chunk stored-field read failed: {e}"))
                })?;
                records.push(self.doc_to_record(&doc));
            }
        }
        Ok(records)
    }

    /// Total number of documents (chunks) in the Tantivy index.
    pub fn doc_count(&self) -> usize {
        let searcher = self.reader.searcher();
        searcher.segment_readers().iter().map(|r| r.num_docs() as usize).sum()
    }

    fn doc_to_record(&self, doc: &Document) -> CodeChunkRecord {
        let s = &self.schema;
        let text = |f: Field| -> String {
            doc.get_first(f)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        let opt_text = |f: Field| -> Option<String> {
            doc.get_first(f)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        };
        let u64_val = |f: Field| -> u64 {
            doc.get_first(f).and_then(|v| v.as_u64()).unwrap_or(0)
        };

        CodeChunkRecord {
            chunk_id: u64_val(s.chunk_id) as u32,
            symbol_id: u64_val(s.symbol_id) as u32,
            file_path: text(s.file_path),
            language: opt_text(s.language),
            chunk_type: text(s.chunk_type),
            parent_scope: opt_text(s.parent_scope),
            line_start: u64_val(s.line_start) as u32,
            line_end: u64_val(s.line_end) as u32,
            signature: opt_text(s.signature),
            doc_comment: opt_text(s.doc_comment),
            snippet: text(s.snippet),
            embedding_text: String::new(),
        }
    }
}

/// Query-time embedding model for chunk vector search.
enum ChunkQueryModel {
    Optimized(OptimizedStaticModel),
    Fastembed(Mutex<TextEmbedding>),
}

/// Cached chunk vector search backend using random-access mmap.
///
/// Instead of loading all embeddings into a HashMap (~860 MB heap copy),
/// this struct holds only:
/// - `binary_index` (~30 MB) for Hamming pre-filter
/// - `id_offset_map` (~5 MB) for random mmap access
/// - `languages` (~8.7 MB) for language filtering
/// - query model (~31 MB for model2vec) for embedding queries
///
/// Search reads only ~500 candidate vectors from mmap pages.
pub struct ChunkVectorBackend {
    binary_index: HashMap<u32, BinaryVector>,
    id_offset_map: HashMap<u32, usize>,
    languages: HashMap<u32, String>,
    storage: MmapVectorStorage,
    model: ChunkQueryModel,
}

impl ChunkVectorBackend {
    /// Open the chunk semantic index at `<root>/semantic`.
    pub fn open(root: &Path) -> Result<Self, IndexError> {
        let semantic_path = root.join("semantic");
        if !semantic_path.join("metadata.json").exists() {
            return Err(IndexError::General(
                "chunk semantic metadata not found".to_string(),
            ));
        }

        // 1. Load metadata → model_name
        let metadata = SemanticMetadata::load(&semantic_path).map_err(|e| {
            IndexError::General(format!("failed to load chunk semantic metadata: {e}"))
        })?;

        // 2. Open mmap storage + build id→offset map (~5 MB)
        let mut storage =
            MmapVectorStorage::open(&semantic_path, SegmentOrdinal::new(0)).map_err(|e| {
                IndexError::General(format!("failed to open chunk vector storage: {e}"))
            })?;
        let id_offset_map = storage.build_id_offset_map().map_err(|e| {
            IndexError::General(format!("failed to build chunk id-offset map: {e}"))
        })?;

        // 3. Load binary index (~30 MB)
        let binary_index = Self::load_binary_index(&semantic_path)?;

        // 4. Load languages map (~8.7 MB)
        let languages = Self::load_languages(&semantic_path);

        // 5. Init query embedding model
        let model = Self::init_model(&metadata.model_name)?;

        Ok(Self {
            binary_index,
            id_offset_map,
            languages,
            storage,
            model,
        })
    }

    /// Vector search returning `(chunk_id, cosine_score)` pairs.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
        language_filter: Option<&str>,
    ) -> Result<Vec<(u32, f32)>, IndexError> {
        if limit == 0 || self.binary_index.is_empty() {
            return Ok(Vec::new());
        }

        // Embed query
        let query_embedding = self.embed_query(query)?;
        let query_binary = BinaryVector::from_embedding(&query_embedding);

        // Hamming pre-filter
        let prefilter_limit = (limit * 5).max(100);
        let mut hamming_scores: Vec<(u32, u32)> = self
            .binary_index
            .iter()
            .filter(|(id, _)| match language_filter {
                Some(lang) => self
                    .languages
                    .get(id)
                    .is_some_and(|l| l == lang),
                None => true,
            })
            .map(|(&id, bv)| (id, query_binary.hamming_distance(bv)))
            .collect();

        hamming_scores.sort_unstable_by_key(|(_, dist)| *dist);
        hamming_scores.truncate(prefilter_limit);

        // Random-access read only the candidate vectors from mmap
        let ids_offsets: Vec<(u32, usize)> = hamming_scores
            .iter()
            .filter_map(|(id, _)| self.id_offset_map.get(id).map(|&off| (*id, off)))
            .collect();

        let vectors = self.storage.read_vectors_by_offsets(&ids_offsets);

        // Cosine similarity refinement
        let mut results: Vec<(u32, f32)> = vectors
            .into_iter()
            .map(|(id, emb)| (id, cosine_similarity(&query_embedding, &emb)))
            .collect();

        results.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        Ok(results)
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>, IndexError> {
        match &self.model {
            ChunkQueryModel::Optimized(m) => Ok(m.encode_single(text)),
            ChunkQueryModel::Fastembed(m) => {
                let mut guard = m
                    .lock()
                    .map_err(|_| IndexError::General("chunk embed lock poisoned".to_string()))?;
                let embeddings = guard
                    .embed(vec![text], None)
                    .map_err(|e| IndexError::General(format!("chunk query embed failed: {e}")))?;
                embeddings
                    .into_iter()
                    .next()
                    .ok_or_else(|| IndexError::General("no embedding returned".to_string()))
            }
        }
    }

    fn init_model(model_name: &str) -> Result<ChunkQueryModel, IndexError> {
        if SimpleSemanticSearch::looks_like_model2vec_model(model_name) {
            let source = model_name.strip_prefix("model2vec:").unwrap_or(model_name);
            let opt = OptimizedStaticModel::from_local(source).map_err(|e| {
                IndexError::General(format!(
                    "failed to load chunk vector model '{model_name}': {e}"
                ))
            })?;
            return Ok(ChunkQueryModel::Optimized(opt));
        }
        let (te, _dims) =
            create_text_embedding_with_max_length(model_name, false, Some(1024)).map_err(|e| {
                IndexError::General(format!(
                    "failed to load chunk vector model '{model_name}': {e}"
                ))
            })?;
        Ok(ChunkQueryModel::Fastembed(Mutex::new(te)))
    }

    fn load_binary_index(semantic_path: &Path) -> Result<HashMap<u32, BinaryVector>, IndexError> {
        let bin_path = semantic_path.join("binary_index.bin");
        let data = std::fs::read(&bin_path).map_err(|e| {
            IndexError::General(format!("failed to read binary_index.bin: {e}"))
        })?;
        Self::parse_binary_index(&data)
            .ok_or_else(|| IndexError::General("corrupt binary_index.bin".to_string()))
    }

    fn parse_binary_index(data: &[u8]) -> Option<HashMap<u32, BinaryVector>> {
        if data.len() < 4 {
            return None;
        }
        let count = u32::from_le_bytes(data[0..4].try_into().ok()?) as usize;
        let mut index = HashMap::with_capacity(count);
        let mut offset = 4;

        for _ in 0..count {
            if offset + 8 > data.len() {
                return None;
            }
            let id = u32::from_le_bytes(data[offset..offset + 4].try_into().ok()?);
            offset += 4;
            let bv_len =
                u32::from_le_bytes(data[offset..offset + 4].try_into().ok()?) as usize;
            offset += 4;
            if offset + bv_len > data.len() {
                return None;
            }
            let bv = BinaryVector::from_bytes(&data[offset..offset + bv_len])?;
            offset += bv_len;
            index.insert(id, bv);
        }
        Some(index)
    }

    fn load_languages(semantic_path: &Path) -> HashMap<u32, String> {
        let lang_path = semantic_path.join("languages.json");
        let Ok(json) = std::fs::read_to_string(&lang_path) else {
            return HashMap::new();
        };
        serde_json::from_str::<HashMap<u32, String>>(&json).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileId, Range};
    use tempfile::TempDir;

    struct MockRecallBackend {
        model_name: String,
        dims: usize,
    }

    impl RecallBackend for MockRecallBackend {
        fn model_name(&self) -> &str {
            &self.model_name
        }

        fn dimensions(&self) -> usize {
            self.dims
        }

        fn encode(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, IndexError> {
            Ok(texts
                .iter()
                .map(|_| vec![0.25f32; self.dims])
                .collect::<Vec<_>>())
        }
    }

    #[test]
    fn test_build_chunk_record_extracts_symbol_snippet() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("sample.rs");
        std::fs::write(
            &file,
            "fn a() {}\nfn b(x: i32) -> i32 {\n    x + 1\n}\nfn c() {}\n",
        )
        .unwrap();

        let symbol = Symbol::new(
            SymbolId::new(7).unwrap(),
            "b",
            SymbolKind::Function,
            FileId::new(1).unwrap(),
            Range::new(1, 0, 3, 0),
        )
        .with_file_path(file.to_string_lossy().to_string())
        .with_signature("fn b(x: i32) -> i32");

        let mut cache = HashMap::new();
        let chunk = build_chunk_record(&symbol, None, 2000, 2, 3, &mut cache).unwrap();
        assert!(chunk.snippet.contains("fn b(x: i32) -> i32"));
        assert!(chunk
            .embedding_text
            .contains("# function fn b(x: i32) -> i32"));
    }

    #[test]
    fn test_chunk_indexer_rejects_dimension_mismatch() {
        let temp = TempDir::new().unwrap();
        let indexer = CodeChunkIndexer::new(temp.path());
        let backend = MockRecallBackend {
            model_name: "AllMiniLML6V2".to_string(),
            dims: 16,
        };

        let result = indexer.rebuild_from_symbols(
            &[],
            &backend,
            None,
            None,
            1000,
            2,
            3,
            None,
            false,
            &[],
            6,
            800,
            4096,
            96,
            Some(8),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_bm25_recall_uses_chunk_doc_type_filter() {
        let temp = TempDir::new().unwrap();
        let chunks = vec![CodeChunkRecord {
            chunk_id: 11,
            symbol_id: 11,
            file_path: "src/auth.rs".to_string(),
            language: Some("rust".to_string()),
            chunk_type: "function".to_string(),
            parent_scope: Some("AuthService".to_string()),
            line_start: 10,
            line_end: 30,
            signature: Some("fn authenticate(token: &str) -> bool".to_string()),
            doc_comment: Some("Authenticate request token".to_string()),
            snippet: "fn authenticate(token: &str) -> bool { true }".to_string(),
            embedding_text: "x".to_string(),
        }];

        write_tantivy_chunk_index(temp.path(), &chunks).unwrap();

        let backend = ChunkSearchBackend::open(temp.path()).unwrap();
        let hits = backend.bm25_search("authenticate token", 5, Some("rust")).unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].0, 11);
    }

    #[test]
    fn test_rebuild_adds_mixed_chunks_for_module_gap_and_text_files() {
        let temp = TempDir::new().unwrap();
        let code_file = temp.path().join("src").join("auth.ts");
        std::fs::create_dir_all(code_file.parent().unwrap()).unwrap();
        std::fs::write(
            &code_file,
            "// auth module helpers\n// used by CLI edit flow\n\nfunction startAuth() { return true }\n\nconst EDIT_TOOLS = [\"edit\"]\n\nfunction finishAuth() { return false }\n",
        )
        .unwrap();

        let readme = temp.path().join("README.md");
        std::fs::write(
            &readme,
            "# Authentication\n\nThis module handles auth flow and edit-tool permission checks.\n",
        )
        .unwrap();
        let env = temp.path().join(".env");
        std::fs::write(&env, "AUTH_PROVIDER=oauth\nEDIT_TOOL_ENABLED=true\n").unwrap();

        let symbols = vec![
            Symbol::new(
                SymbolId::new(1).unwrap(),
                "startAuth",
                SymbolKind::Function,
                FileId::new(1).unwrap(),
                Range::new(3, 0, 3, 34),
            )
            .with_file_path(code_file.to_string_lossy().to_string())
            .with_signature("function startAuth()"),
            Symbol::new(
                SymbolId::new(2).unwrap(),
                "finishAuth",
                SymbolKind::Function,
                FileId::new(1).unwrap(),
                Range::new(7, 0, 7, 36),
            )
            .with_file_path(code_file.to_string_lossy().to_string())
            .with_signature("function finishAuth()"),
        ];

        let indexed_paths = vec![code_file.clone(), readme.clone(), env.clone()];
        let indexer = CodeChunkIndexer::new(temp.path());
        let backend = MockRecallBackend {
            model_name: "AllMiniLML6V2".to_string(),
            dims: 16,
        };
        let stats = indexer
            .rebuild_from_symbols(
                &symbols,
                &backend,
                Some(temp.path()),
                None,
                2000,
                1,
                2,
                Some(indexed_paths.as_slice()),
                false,
                &[],
                6,
                800,
                4096,
                96,
                Some(16),
            )
            .unwrap();
        assert!(stats.chunks_indexed >= 5);

        let backend = ChunkSearchBackend::open(&temp.path().join("code_chunks")).unwrap();
        let chunks = backend.load_all_records().unwrap();
        assert!(chunks.iter().any(|c| c.chunk_type == "module_comment"));
        assert!(chunks.iter().any(|c| c.chunk_type == "inter_symbol_gap"));
        assert!(chunks.iter().any(|c| c.chunk_type == "doc_chunk"));
        assert!(chunks.iter().any(|c| c.chunk_type == "config_chunk"));
    }

    #[test]
    fn test_incremental_rebuild_replaces_changed_and_removes_deleted_files() {
        let temp = TempDir::new().unwrap();
        let file_a = temp.path().join("src").join("a.ts");
        let file_b = temp.path().join("src").join("b.ts");
        std::fs::create_dir_all(file_a.parent().unwrap()).unwrap();
        std::fs::write(&file_a, "function a() { return 1 }\n").unwrap();
        std::fs::write(&file_b, "function b() { return 2 }\n").unwrap();

        let mut symbols = vec![
            Symbol::new(
                SymbolId::new(1).unwrap(),
                "a",
                SymbolKind::Function,
                FileId::new(1).unwrap(),
                Range::new(0, 0, 0, 26),
            )
            .with_file_path(file_a.to_string_lossy().to_string())
            .with_signature("function a()"),
            Symbol::new(
                SymbolId::new(2).unwrap(),
                "b",
                SymbolKind::Function,
                FileId::new(2).unwrap(),
                Range::new(0, 0, 0, 26),
            )
            .with_file_path(file_b.to_string_lossy().to_string())
            .with_signature("function b()"),
        ];

        let indexer = CodeChunkIndexer::new(temp.path());
        let backend = MockRecallBackend {
            model_name: "AllMiniLML6V2".to_string(),
            dims: 16,
        };
        indexer
            .rebuild_from_symbols(
                &symbols,
                &backend,
                Some(temp.path()),
                None,
                2000,
                1,
                2,
                None,
                false,
                &[],
                6,
                800,
                4096,
                96,
                Some(16),
            )
            .unwrap();

        std::fs::write(&file_a, "function a() { return 10 }\n").unwrap();
        symbols.pop(); // file_b deleted from symbol index state

        indexer
            .rebuild_incremental_from_symbols(
                &symbols,
                &backend,
                Some(temp.path()),
                None,
                2000,
                1,
                2,
                false,
                &[],
                6,
                800,
                4096,
                96,
                Some(16),
                &[file_a.clone()],
                &[file_b.clone()],
                false,
            )
            .unwrap();

        let backend = ChunkSearchBackend::open(&temp.path().join("code_chunks")).unwrap();
        let chunks = backend.load_all_records().unwrap();
        assert!(chunks
            .iter()
            .any(|c| c.file_path == file_a.to_string_lossy()));
        assert!(!chunks
            .iter()
            .any(|c| c.file_path == file_b.to_string_lossy()));
    }

    #[test]
    fn test_dedup_chunk_records_by_file_range_and_type() {
        let mut chunks = vec![
            CodeChunkRecord {
                chunk_id: 1,
                symbol_id: 1,
                file_path: "./src/a.ts".to_string(),
                language: Some("typescript".to_string()),
                chunk_type: "flow_if_else".to_string(),
                parent_scope: Some("foo".to_string()),
                line_start: 10,
                line_end: 20,
                signature: Some("foo".to_string()),
                doc_comment: None,
                snippet: "if (x) {}".to_string(),
                embedding_text: "x".to_string(),
            },
            CodeChunkRecord {
                chunk_id: 2,
                symbol_id: 1,
                file_path: "./src/a.ts".to_string(),
                language: Some("typescript".to_string()),
                chunk_type: "flow_if_else".to_string(),
                parent_scope: Some("foo".to_string()),
                line_start: 10,
                line_end: 20,
                signature: Some("foo".to_string()),
                doc_comment: None,
                snippet: "if (x) { y(); }".to_string(),
                embedding_text: "y".to_string(),
            },
            CodeChunkRecord {
                chunk_id: 3,
                symbol_id: 1,
                file_path: "./src/a.ts".to_string(),
                language: Some("typescript".to_string()),
                chunk_type: "function".to_string(),
                parent_scope: Some("foo".to_string()),
                line_start: 10,
                line_end: 20,
                signature: Some("foo".to_string()),
                doc_comment: None,
                snippet: "function foo() {}".to_string(),
                embedding_text: "z".to_string(),
            },
        ];

        dedup_chunk_records(&mut chunks);
        assert_eq!(chunks.len(), 2);
        assert!(chunks.iter().any(|c| c.chunk_type == "flow_if_else"));
        assert!(chunks.iter().any(|c| c.chunk_type == "function"));
    }

    #[test]
    fn test_split_chunk_by_token_budget_splits_long_chunk() {
        let mut next_chunk_id = 100u32;
        let snippet = vec![
            "alpha beta gamma delta epsilon",
            "alpha beta gamma delta epsilon",
            "alpha beta gamma delta epsilon",
            "alpha beta gamma delta epsilon",
            "alpha beta gamma delta epsilon",
            "alpha beta gamma delta epsilon",
        ]
        .join("\n");
        let chunk = CodeChunkRecord {
            chunk_id: 1,
            symbol_id: 1,
            file_path: "./src/a.ts".to_string(),
            language: Some("typescript".to_string()),
            chunk_type: "function".to_string(),
            parent_scope: Some("foo".to_string()),
            line_start: 10,
            line_end: 40,
            signature: Some("function foo()".to_string()),
            doc_comment: None,
            snippet,
            embedding_text: "x".to_string(),
        };

        let out = split_chunk_by_token_budget(&chunk, &mut next_chunk_id, 10_000, 12, 16, 4);
        assert!(out.len() >= 2);
        for part in &out {
            assert!(approx_token_count(&part.snippet) <= 16);
        }
        assert_eq!(out[0].chunk_id, 1);
        assert!(out.iter().skip(1).all(|c| c.chunk_id >= 100));
    }
}
