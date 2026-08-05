//! Simple semantic search implementation for documentation comments

use crate::SymbolId;
use crate::semantic::static_model::OptimizedStaticModel;
use crate::vector::BinaryVector;
use fastembed::{EmbeddingModel, TextEmbedding};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;

/// Embedding format version. Increment when enrichment format changes to force re-embed.
const EMBEDDING_FORMAT_VERSION: u32 = 2;

/// Error type for semantic search operations
#[derive(Debug, thiserror::Error)]
pub enum SemanticSearchError {
    #[error("Failed to initialize embedding model: {0}")]
    ModelInitError(String),

    #[error("Failed to generate embedding: {0}")]
    EmbeddingError(String),

    #[error("No embeddings available for search")]
    NoEmbeddings,

    #[error("Storage error: {message}\nSuggestion: {suggestion}")]
    StorageError { message: String, suggestion: String },

    #[error("Dimension mismatch: expected {expected}, got {actual}\nSuggestion: {suggestion}")]
    DimensionMismatch {
        expected: usize,
        actual: usize,
        suggestion: String,
    },

    #[error("Invalid ID: {id}\nSuggestion: {suggestion}")]
    InvalidId { id: u32, suggestion: String },
}

/// Advanced semantic search engine for documentation analysis
///
/// This implementation uses state-of-the-art embeddings to find
/// semantically similar documentation across the entire codebase,
/// enabling natural language queries for code discovery.
pub struct SimpleSemanticSearch {
    /// Embeddings indexed by symbol ID
    embeddings: HashMap<SymbolId, Vec<f32>>,

    /// Language mapping for each symbol (for language-filtered search)
    symbol_languages: HashMap<SymbolId, String>,

    /// The embedding model (lazy-initialized for query-time use).
    model: Mutex<Option<QueryEmbeddingModel>>,

    /// Model name used for lazy initialization.
    model_name: String,

    /// Model dimensions for validation
    dimensions: usize,

    /// Metadata for tracking model info and timestamps
    metadata: Option<crate::semantic::SemanticMetadata>,

    /// Binary quantized index for fast Hamming distance pre-filtering (Step 7)
    binary_index: HashMap<SymbolId, BinaryVector>,

    /// Content hash tracking per file for lazy embedding skip (Step 5)
    /// Keyed by stable normalized file path, not transient FileId.
    embedded_file_hashes: HashMap<String, String>,
}

enum QueryEmbeddingModel {
    Fastembed(TextEmbedding),
    Optimized(OptimizedStaticModel),
}

impl std::fmt::Debug for SimpleSemanticSearch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimpleSemanticSearch")
            .field("embeddings_count", &self.embeddings.len())
            .field("binary_index_count", &self.binary_index.len())
            .field("embedded_file_count", &self.embedded_file_hashes.len())
            .field("dimensions", &self.dimensions)
            .field("model", &"<LazyTextEmbedding>")
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl SimpleSemanticSearch {
    /// Normalize file path into a stable cache key.
    pub fn stable_file_key(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    /// Compute a stable fingerprint for file-level embedding cache checks.
    fn fingerprint_file_hash(content_hash: &str) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&EMBEDDING_FORMAT_VERSION.to_le_bytes());
        hasher.update(content_hash.as_bytes());
        hasher.finalize().to_hex().to_string()
    }

    fn infer_dimensions(model_name: &str) -> Option<usize> {
        if Self::is_model2vec_model(model_name) {
            return None;
        }
        match model_name {
            "GraniteSmallEnglishR2"
            | "AllMiniLML6V2"
            | "MultilingualE5Small"
            | "JinaEmbeddingsV2BaseCode" => Some(384),
            "BGESmallZHV15" => Some(512),
            "MultilingualE5Base" => Some(768),
            "MultilingualE5Large" | "BGELargeZHV15" => Some(1024),
            _ => None,
        }
    }

    fn is_model2vec_model(model_name: &str) -> bool {
        if model_name.starts_with("model2vec:")
            || model_name.starts_with("minishlab/")
            || model_name.starts_with("sentence-transformers/static-retrieval")
        {
            return true;
        }

        let local_path = Path::new(model_name);
        local_path.join("model.safetensors").exists()
            && local_path.join("tokenizer.json").exists()
            && local_path.join("config.json").exists()
    }

    /// Best-effort detection for model2vec-compatible model identifiers.
    pub fn looks_like_model2vec_model(model_name: &str) -> bool {
        Self::is_model2vec_model(model_name)
    }

    fn model2vec_source(model_name: &str) -> &str {
        model_name.strip_prefix("model2vec:").unwrap_or(model_name)
    }

    fn init_model(
        model_name: &str,
        show_progress: bool,
    ) -> Result<QueryEmbeddingModel, SemanticSearchError> {
        if Self::is_model2vec_model(model_name) {
            let source = Self::model2vec_source(model_name);
            let opt = OptimizedStaticModel::from_local(source).map_err(|e| {
                SemanticSearchError::ModelInitError(format!(
                    "Failed to load optimized static model '{model_name}': {e}"
                ))
            })?;
            return Ok(QueryEmbeddingModel::Optimized(opt));
        }

        crate::vector::create_text_embedding_with_max_length(model_name, show_progress, Some(1024))
            .map(|(model, _)| QueryEmbeddingModel::Fastembed(model))
            .map_err(|e| SemanticSearchError::ModelInitError(format!("Invalid model name: {e}")))
    }

    fn embed_with_model(
        &self,
        text: &str,
        show_progress: bool,
    ) -> Result<Vec<f32>, SemanticSearchError> {
        let mut model_guard = self.model.lock().map_err(|_| {
            SemanticSearchError::ModelInitError("Failed to lock embedding model".to_string())
        })?;
        if model_guard.is_none() {
            *model_guard = Some(Self::init_model(&self.model_name, show_progress)?);
        }
        let model = model_guard.as_mut().ok_or_else(|| {
            SemanticSearchError::ModelInitError("Embedding model missing".to_string())
        })?;
        match model {
            QueryEmbeddingModel::Fastembed(model) => {
                let embeddings = model
                    .embed(vec![text], None)
                    .map_err(|e| SemanticSearchError::EmbeddingError(e.to_string()))?;
                embeddings.into_iter().next().ok_or_else(|| {
                    SemanticSearchError::EmbeddingError("No embedding returned".to_string())
                })
            }
            QueryEmbeddingModel::Optimized(model) => Ok(model.encode_single(text)),
        }
    }

    /// Create a new semantic search instance using default model (AllMiniLML6V2).
    ///
    /// For multilingual support, use `from_model_name` with "MultilingualE5Small".
    pub fn new() -> Result<Self, SemanticSearchError> {
        Self::with_model(EmbeddingModel::AllMiniLML6V2)
    }

    /// Create a semantic search instance from a model name string.
    ///
    /// # Arguments
    /// * `model_name` - Name of the model (e.g., "MultilingualE5Small", "AllMiniLML6V2")
    ///
    /// # Supported Models
    /// - `AllMiniLML6V2` - English-only, 384 dimensions (default)
    /// - `MultilingualE5Small` - 94 languages, 384 dimensions (recommended for multilingual)
    /// - `MultilingualE5Base` - 94 languages, 768 dimensions
    /// - `MultilingualE5Large` - 94 languages, 1024 dimensions
    /// - And many more (see `parse_embedding_model` documentation)
    ///
    /// # Example
    /// ```ignore
    /// let search = SimpleSemanticSearch::from_model_name("MultilingualE5Small")?;
    /// ```
    pub fn from_model_name(model_name: &str) -> Result<Self, SemanticSearchError> {
        let mut preloaded_model: Option<QueryEmbeddingModel> = None;
        let dimensions = if let Some(dims) = Self::infer_dimensions(model_name) {
            dims
        } else {
            // Fallback for unknown models: initialize once to discover dimensions.
            let mut model = Self::init_model(model_name, true)?;
            let discovered = match &mut model {
                QueryEmbeddingModel::Fastembed(text_model) => {
                    let test_embedding = text_model
                        .embed(vec!["test"], None)
                        .map_err(|e| SemanticSearchError::EmbeddingError(e.to_string()))?;
                    test_embedding.into_iter().next().unwrap().len()
                }
                QueryEmbeddingModel::Optimized(opt_model) => opt_model.dim(),
            };
            preloaded_model = Some(model);
            discovered
        };

        // Create initial metadata
        let metadata =
            crate::semantic::SemanticMetadata::new(model_name.to_string(), dimensions, 0);

        Ok(Self {
            embeddings: HashMap::new(),
            symbol_languages: HashMap::new(),
            model: Mutex::new(preloaded_model),
            model_name: model_name.to_string(),
            dimensions,
            metadata: Some(metadata),
            binary_index: HashMap::new(),
            embedded_file_hashes: HashMap::new(),
        })
    }

    /// Create with a specific model enum.
    pub fn with_model(model: EmbeddingModel) -> Result<Self, SemanticSearchError> {
        let model_name = crate::vector::model_to_string(&model);
        let dimensions = Self::infer_dimensions(&model_name).unwrap_or(384);

        // Create initial metadata
        let metadata = crate::semantic::SemanticMetadata::new(
            model_name.clone(),
            dimensions,
            0, // No embeddings yet
        );

        Ok(Self {
            embeddings: HashMap::new(),
            symbol_languages: HashMap::new(),
            model: Mutex::new(None),
            model_name,
            dimensions,
            metadata: Some(metadata),
            binary_index: HashMap::new(),
            embedded_file_hashes: HashMap::new(),
        })
    }

    /// Index a documentation comment for a symbol
    pub fn index_doc_comment(
        &mut self,
        symbol_id: SymbolId,
        doc: &str,
    ) -> Result<(), SemanticSearchError> {
        // Skip empty docs
        if doc.trim().is_empty() {
            return Ok(());
        }

        // Generate embedding
        let embedding = self.embed_with_model(doc, false)?;

        // Validate dimensions
        if embedding.len() != self.dimensions {
            return Err(SemanticSearchError::EmbeddingError(format!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.dimensions,
                embedding.len()
            )));
        }

        let binary = BinaryVector::from_embedding(&embedding);
        self.binary_index.insert(symbol_id, binary);
        self.embeddings.insert(symbol_id, embedding);
        Ok(())
    }

    /// Index a documentation comment for a symbol with language information
    pub fn index_doc_comment_with_language(
        &mut self,
        symbol_id: SymbolId,
        doc: &str,
        language: &str,
    ) -> Result<(), SemanticSearchError> {
        // First index the doc comment normally
        self.index_doc_comment(symbol_id, doc)?;

        // Then store the language mapping
        if self.embeddings.contains_key(&symbol_id) {
            self.symbol_languages
                .insert(symbol_id, language.to_string());
        }

        Ok(())
    }

    /// Store pre-generated embeddings (from parallel pool generation).
    ///
    /// Used when embeddings are generated in parallel by EmbeddingPool.
    pub fn store_embeddings(&mut self, items: Vec<(SymbolId, Vec<f32>, String)>) -> usize {
        let mut count = 0;
        for (symbol_id, embedding, language) in items {
            if embedding.len() == self.dimensions {
                let binary = BinaryVector::from_embedding(&embedding);
                self.binary_index.insert(symbol_id, binary);
                self.embeddings.insert(symbol_id, embedding);
                self.symbol_languages.insert(symbol_id, language);
                count += 1;
            }
        }
        tracing::debug!(
            target: "semantic",
            "store_embeddings: stored {} embeddings, binary_index now has {} entries",
            count,
            self.binary_index.len()
        );
        count
    }

    /// Search for similar documentation using a natural language query
    ///
    /// Returns symbol IDs with their similarity scores, sorted by score descending
    pub fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(SymbolId, f32)>, SemanticSearchError> {
        if self.embeddings.is_empty() {
            return Err(SemanticSearchError::NoEmbeddings);
        }

        // Generate query embedding
        let query_embedding = self.embed_with_model(query, false)?;

        // Calculate similarities
        let mut similarities: Vec<(SymbolId, f32)> = self
            .embeddings
            .iter()
            .map(|(id, embedding)| {
                let similarity = cosine_similarity(&query_embedding, embedding);
                (*id, similarity)
            })
            .collect();

        // Sort by similarity descending
        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // Return top results
        similarities.truncate(limit);
        Ok(similarities)
    }

    /// Search for similar documentation with language filtering.
    ///
    /// Uses two-phase search when binary index is available:
    /// - Phase 1: Hamming distance pre-filter → top 5*limit candidates (fast, bitwise)
    /// - Phase 2: Full cosine similarity → final top-limit results
    ///
    /// Falls back to brute-force cosine when binary index is empty.
    pub fn search_with_language(
        &self,
        query: &str,
        limit: usize,
        language: Option<&str>,
    ) -> Result<Vec<(SymbolId, f32)>, SemanticSearchError> {
        if self.embeddings.is_empty() {
            return Err(SemanticSearchError::NoEmbeddings);
        }

        // Generate query embedding
        let query_embedding = self.embed_with_model(query, false)?;
        let query_binary = BinaryVector::from_embedding(&query_embedding);

        // Language filter predicate
        let lang_matches = |id: &SymbolId| -> bool {
            match language {
                Some(lang) => self
                    .symbol_languages
                    .get(id)
                    .is_some_and(|symbol_lang| symbol_lang == lang),
                None => true,
            }
        };

        // Two-phase search: binary pre-filter → cosine refinement
        let candidates: Vec<SymbolId> = if !self.binary_index.is_empty() {
            let prefilter_limit = (limit * 5).max(100);

            // Phase 1: Hamming distance pre-filter (fast)
            let mut hamming_scores: Vec<(SymbolId, u32)> = self
                .binary_index
                .iter()
                .filter(|(id, _)| lang_matches(id))
                .map(|(id, bv)| (*id, query_binary.hamming_distance(bv)))
                .collect();

            hamming_scores.sort_by_key(|(_, dist)| *dist);
            hamming_scores.truncate(prefilter_limit);
            hamming_scores.into_iter().map(|(id, _)| id).collect()
        } else {
            // Fallback: all matching embeddings
            self.embeddings
                .keys()
                .filter(|id| lang_matches(id))
                .copied()
                .collect()
        };

        // Phase 2: Full cosine similarity on candidates only
        let mut similarities: Vec<(SymbolId, f32)> = candidates
            .into_iter()
            .filter_map(|id| {
                self.embeddings
                    .get(&id)
                    .map(|emb| (id, cosine_similarity(&query_embedding, emb)))
            })
            .collect();

        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        similarities.truncate(limit);
        Ok(similarities)
    }

    /// Search with a similarity threshold
    pub fn search_with_threshold(
        &self,
        query: &str,
        limit: usize,
        threshold: f32,
    ) -> Result<Vec<(SymbolId, f32)>, SemanticSearchError> {
        let results = self.search(query, limit)?;
        Ok(results
            .into_iter()
            .filter(|(_, score)| *score >= threshold)
            .collect())
    }

    /// Get the number of indexed embeddings
    pub fn embedding_count(&self) -> usize {
        self.embeddings.len()
    }

    /// Clear all embeddings
    pub fn clear(&mut self) {
        self.embeddings.clear();
        self.symbol_languages.clear();
        self.binary_index.clear();
        self.embedded_file_hashes.clear();
    }

    /// Remove embeddings for specific symbols
    ///
    /// This is used when re-indexing files to remove embeddings for symbols
    /// that no longer exist.
    pub fn remove_embeddings(&mut self, symbol_ids: &[SymbolId]) {
        for id in symbol_ids {
            self.embeddings.remove(id);
            self.symbol_languages.remove(id);
            self.binary_index.remove(id);
        }
    }

    /// Get the metadata if available
    pub fn metadata(&self) -> Option<&crate::semantic::SemanticMetadata> {
        self.metadata.as_ref()
    }

    // =========================================================================
    // Lazy Embedding (Step 5) — hash-based skip
    // =========================================================================

    /// Check if a file's embeddings are current (hash matches).
    pub fn is_file_embedded(&self, file_key: &str, content_hash: &str) -> bool {
        let expected = Self::fingerprint_file_hash(content_hash);
        self.embedded_file_hashes
            .get(file_key)
            .is_some_and(|h| h == &expected)
    }

    /// Mark a file as having up-to-date embeddings.
    pub fn mark_file_embedded(&mut self, file_key: String, content_hash: String) {
        let fingerprint = Self::fingerprint_file_hash(&content_hash);
        self.embedded_file_hashes
            .insert(file_key.clone(), fingerprint);
        tracing::debug!(
            target: "semantic",
            "mark_file_embedded: file_key={}, hash={}, embedded_file_hashes now has {} entries",
            file_key,
            content_hash,
            self.embedded_file_hashes.len()
        );
    }

    /// Clear embedding tracking for a file (used during re-index).
    pub fn clear_file_embeddings(&mut self, file_key: &str) {
        self.embedded_file_hashes.remove(file_key);
    }

    /// Number of files with tracked embeddings.
    pub fn embedded_file_count(&self) -> usize {
        self.embedded_file_hashes.len()
    }

    // =========================================================================
    // Metrics (Step 8)
    // =========================================================================

    /// Number of binary vectors in the index.
    pub fn binary_index_count(&self) -> usize {
        self.binary_index.len()
    }

    /// Memory usage: (float_bytes, binary_bytes).
    pub fn memory_usage(&self) -> (usize, usize) {
        let float_bytes = self.embeddings.len() * self.dimensions * std::mem::size_of::<f32>();
        let binary_bytes: usize = self.binary_index.values().map(|bv| bv.size_bytes()).sum();
        (float_bytes, binary_bytes)
    }

    /// Save embeddings to disk using the efficient vector storage
    ///
    /// # Arguments
    /// * `path` - Path where semantic data should be stored
    pub fn save(&self, path: &Path) -> Result<(), SemanticSearchError> {
        use crate::semantic::{SemanticMetadata, SemanticVectorStorage};
        use crate::vector::VectorDimension;

        tracing::info!(
            target: "semantic",
            "save() CALLED: path={:?}, embeddings={}, binary_index={}, embedded_file_hashes={}",
            path,
            self.embeddings.len(),
            self.binary_index.len(),
            self.embedded_file_hashes.len()
        );

        // Ensure the directory exists
        std::fs::create_dir_all(path).map_err(|e| SemanticSearchError::StorageError {
            message: format!("Failed to create semantic directory: {e}"),
            suggestion: "Check directory permissions".to_string(),
        })?;

        // Save metadata with actual model name
        let model_name = if let Some(ref meta) = self.metadata {
            meta.model_name.clone()
        } else {
            // Fallback to AllMiniLML6V2 if metadata not set (shouldn't happen)
            "AllMiniLML6V2".to_string()
        };

        let metadata = SemanticMetadata::new(model_name, self.dimensions, self.embeddings.len());
        metadata.save(path)?;

        // Create storage with our dimension
        let dimension = VectorDimension::new(self.dimensions).map_err(|e| {
            SemanticSearchError::StorageError {
                message: format!("Invalid dimension: {e}"),
                suggestion: "Dimension must be between 1 and 4096".to_string(),
            }
        })?;

        let mut storage = SemanticVectorStorage::new(path, dimension)?;

        // Borrow embeddings directly to avoid cloning large vectors before write.
        let embeddings: Vec<(SymbolId, &[f32])> = self
            .embeddings
            .iter()
            .map(|(id, embedding)| (*id, embedding.as_slice()))
            .collect();

        // Save all embeddings
        storage.save_batch_refs(&embeddings)?;

        // Save language mappings as a JSON file (convert SymbolId to u32 for serialization)
        let languages_path = path.join("languages.json");
        let languages_map: HashMap<u32, &str> = self
            .symbol_languages
            .iter()
            .map(|(id, lang)| (id.to_u32(), lang.as_str()))
            .collect();
        let mut languages_writer = BufWriter::new(File::create(&languages_path).map_err(|e| {
            SemanticSearchError::StorageError {
                message: format!("Failed to create language mappings file: {e}"),
                suggestion: "Check disk space and file permissions".to_string(),
            }
        })?);
        serde_json::to_writer(&mut languages_writer, &languages_map).map_err(|e| {
            SemanticSearchError::StorageError {
                message: format!("Failed to serialize language mappings: {e}"),
                suggestion: "This is likely a bug in the code".to_string(),
            }
        })?;
        languages_writer
            .flush()
            .map_err(|e| SemanticSearchError::StorageError {
                message: format!("Failed to flush language mappings: {e}"),
                suggestion: "Check disk space and file permissions".to_string(),
            })?;

        // Save binary index
        self.save_binary_index(path)?;

        // Save embedded file hashes
        self.save_embedded_hashes(path)?;

        Ok(())
    }

    /// Save binary index to `binary_index.bin`.
    ///
    /// Format: [u32 count][entries: u32 symbol_id + serialized BinaryVector]
    fn save_binary_index(&self, path: &Path) -> Result<(), SemanticSearchError> {
        tracing::info!(
            target: "semantic",
            "save_binary_index: binary_index.len()={}, path={:?}",
            self.binary_index.len(),
            path
        );

        let bin_path = path.join("binary_index.bin");
        let mut writer = BufWriter::new(File::create(&bin_path).map_err(|e| {
            SemanticSearchError::StorageError {
                message: format!("Failed to create binary index file: {e}"),
                suggestion: "Check disk space and file permissions".to_string(),
            }
        })?);

        writer
            .write_all(&(self.binary_index.len() as u32).to_le_bytes())
            .map_err(|e| SemanticSearchError::StorageError {
                message: format!("Failed to write binary index header: {e}"),
                suggestion: "Check disk space and file permissions".to_string(),
            })?;

        let mut bytes_written: usize = 4;
        for (sid, bv) in &self.binary_index {
            writer.write_all(&sid.to_u32().to_le_bytes()).map_err(|e| {
                SemanticSearchError::StorageError {
                    message: format!("Failed to write binary index symbol id: {e}"),
                    suggestion: "Check disk space and file permissions".to_string(),
                }
            })?;
            let bv_bytes = bv.to_bytes();
            writer
                .write_all(&(bv_bytes.len() as u32).to_le_bytes())
                .and_then(|_| writer.write_all(&bv_bytes))
                .map_err(|e| SemanticSearchError::StorageError {
                    message: format!("Failed to write binary index vector bytes: {e}"),
                    suggestion: "Check disk space and file permissions".to_string(),
                })?;
            bytes_written += 8 + bv_bytes.len();
        }
        writer
            .flush()
            .map_err(|e| SemanticSearchError::StorageError {
                message: format!("Failed to flush binary index: {e}"),
                suggestion: "Check disk space and file permissions".to_string(),
            })?;

        tracing::info!(target: "semantic", "save_binary_index: SUCCESS");
        tracing::info!(
            target: "semantic",
            "save_binary_index: writing {} bytes to {:?}",
            bytes_written,
            bin_path
        );
        Ok(())
    }

    /// Save embedded file hashes to `embedded_hashes.json`.
    fn save_embedded_hashes(&self, path: &Path) -> Result<(), SemanticSearchError> {
        tracing::info!(
            target: "semantic",
            "save_embedded_hashes: embedded_file_hashes.len()={}, path={:?}",
            self.embedded_file_hashes.len(),
            path
        );

        let hashes_path = path.join("embedded_hashes.json");

        #[derive(serde::Serialize)]
        struct HashFile<'a> {
            format_version: u32,
            hashes: &'a HashMap<String, String>,
        }

        let hash_file = HashFile {
            format_version: EMBEDDING_FORMAT_VERSION,
            hashes: &self.embedded_file_hashes,
        };

        let mut writer = BufWriter::new(File::create(&hashes_path).map_err(|e| {
            SemanticSearchError::StorageError {
                message: format!("Failed to create embedded hashes file: {e}"),
                suggestion: "Check disk space and file permissions".to_string(),
            }
        })?);
        serde_json::to_writer(&mut writer, &hash_file).map_err(|e| {
            SemanticSearchError::StorageError {
                message: format!("Failed to serialize embedded hashes: {e}"),
                suggestion: "This is likely a bug in the code".to_string(),
            }
        })?;
        writer
            .flush()
            .map_err(|e| SemanticSearchError::StorageError {
                message: format!("Failed to flush embedded hashes: {e}"),
                suggestion: "Check disk space and file permissions".to_string(),
            })?;
        let json_bytes = std::fs::metadata(&hashes_path)
            .map(|m| m.len() as usize)
            .unwrap_or(0);
        tracing::info!(
            target: "semantic",
            "save_embedded_hashes: writing {} bytes JSON to {:?}",
            json_bytes,
            hashes_path
        );

        tracing::info!(target: "semantic", "save_embedded_hashes: SUCCESS");
        Ok(())
    }

    /// Load embeddings from disk.
    ///
    /// Automatically uses the model specified in the metadata.
    ///
    /// # Arguments
    /// * `path` - Path where semantic data is stored
    pub fn load(path: &Path) -> Result<Self, SemanticSearchError> {
        use crate::semantic::{SemanticMetadata, SemanticVectorStorage};

        // Load metadata first
        let metadata = SemanticMetadata::load(path)?;

        // Open existing storage
        let mut storage = SemanticVectorStorage::open(path)?;

        // Verify dimension matches
        if storage.dimension().get() != metadata.dimension {
            return Err(SemanticSearchError::DimensionMismatch {
                expected: metadata.dimension,
                actual: storage.dimension().get(),
                suggestion: format!(
                    "Index was created with a {}-dimension model. Re-index with: quarry index <path> --force",
                    storage.dimension().get()
                ),
            });
        }

        // Load all embeddings
        let embeddings_vec = storage.load_all()?;

        // Verify count matches metadata. Mismatch means partial/corrupt store;
        // fail fast so callers can gracefully fall back instead of serving degraded rankings.
        if embeddings_vec.len() != metadata.embedding_count {
            return Err(SemanticSearchError::StorageError {
                message: format!(
                    "Embedding count mismatch in '{}': metadata={}, storage={}",
                    path.display(),
                    metadata.embedding_count,
                    embeddings_vec.len()
                ),
                suggestion: "Rebuild the semantic index: quarry index <path> --force".to_string(),
            });
        }

        // Convert to HashMap
        let mut embeddings = HashMap::with_capacity(embeddings_vec.len());
        for (id, embedding) in embeddings_vec {
            embeddings.insert(id, embedding);
        }

        let model_name = metadata.model_name.clone();

        // Load language mappings if they exist
        let languages_path = path.join("languages.json");
        let symbol_languages = if languages_path.exists() {
            let languages_json = std::fs::read_to_string(&languages_path).map_err(|e| {
                SemanticSearchError::StorageError {
                    message: format!("Failed to read language mappings: {e}"),
                    suggestion: "Language mappings file may be corrupted".to_string(),
                }
            })?;
            let languages_map: HashMap<u32, String> = serde_json::from_str(&languages_json)
                .map_err(|e| SemanticSearchError::StorageError {
                    message: format!("Failed to parse language mappings: {e}"),
                    suggestion: "Try rebuilding the semantic index".to_string(),
                })?;
            // Convert u32 keys back to SymbolId
            languages_map
                .into_iter()
                .filter_map(|(id, lang)| SymbolId::new(id).map(|sid| (sid, lang)))
                .collect()
        } else {
            HashMap::new()
        };

        // Load or rebuild binary index
        let binary_index = Self::load_binary_index(path, &embeddings);

        // Load embedded file hashes
        let embedded_file_hashes = Self::load_embedded_hashes(path);

        Ok(Self {
            embeddings,
            symbol_languages,
            model: Mutex::new(None),
            model_name,
            dimensions: metadata.dimension,
            metadata: Some(metadata),
            binary_index,
            embedded_file_hashes,
        })
    }

    /// Load binary index from `binary_index.bin`.
    /// Falls back to rebuilding from float embeddings if file is missing/corrupt.
    fn load_binary_index(
        path: &Path,
        embeddings: &HashMap<SymbolId, Vec<f32>>,
    ) -> HashMap<SymbolId, BinaryVector> {
        let bin_path = path.join("binary_index.bin");
        if let Ok(data) = std::fs::read(&bin_path) {
            if let Some(index) = Self::parse_binary_index(&data) {
                return index;
            }
            tracing::warn!("Corrupt binary_index.bin, rebuilding from float embeddings");
        }

        // Rebuild from float embeddings
        embeddings
            .iter()
            .map(|(id, emb)| (*id, BinaryVector::from_embedding(emb)))
            .collect()
    }

    fn parse_binary_index(data: &[u8]) -> Option<HashMap<SymbolId, BinaryVector>> {
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
            let sid_raw = u32::from_le_bytes(data[offset..offset + 4].try_into().ok()?);
            offset += 4;
            let bv_len = u32::from_le_bytes(data[offset..offset + 4].try_into().ok()?) as usize;
            offset += 4;
            if offset + bv_len > data.len() {
                return None;
            }
            let bv = BinaryVector::from_bytes(&data[offset..offset + bv_len])?;
            offset += bv_len;
            if let Some(sid) = SymbolId::new(sid_raw) {
                index.insert(sid, bv);
            }
        }
        Some(index)
    }

    /// Load embedded file hashes from `embedded_hashes.json`.
    /// Returns empty map if file missing, corrupt, or format version mismatch.
    fn load_embedded_hashes(path: &Path) -> HashMap<String, String> {
        let hashes_path = path.join("embedded_hashes.json");
        let Ok(json) = std::fs::read_to_string(&hashes_path) else {
            return HashMap::new();
        };

        #[derive(serde::Deserialize)]
        struct HashFile {
            format_version: u32,
            hashes: HashMap<String, String>,
        }

        let Ok(hash_file) = serde_json::from_str::<HashFile>(&json) else {
            tracing::warn!("Corrupt embedded_hashes.json, forcing full re-embed");
            return HashMap::new();
        };

        if hash_file.format_version != EMBEDDING_FORMAT_VERSION {
            tracing::info!(
                "Embedding format version changed ({} → {}), forcing full re-embed",
                hash_file.format_version,
                EMBEDDING_FORMAT_VERSION
            );
            return HashMap::new();
        }

        hash_file.hashes
    }
}

/// Calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude_a * magnitude_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::{SemanticMetadata, SemanticVectorStorage};
    use crate::vector::VectorDimension;

    #[test]
    #[ignore = "Downloads 86MB model - run with --ignored for semantic tests"]
    fn test_remove_embeddings() {
        let mut search = SimpleSemanticSearch::new().unwrap();

        // Add some embeddings with distinct content
        let id1 = SymbolId::new(1).unwrap();
        let id2 = SymbolId::new(2).unwrap();
        let id3 = SymbolId::new(3).unwrap();

        search
            .index_doc_comment(id1, "Parse JSON data from file")
            .unwrap();
        search
            .index_doc_comment(id2, "Connect to database server")
            .unwrap();
        search
            .index_doc_comment(id3, "Calculate hash of string")
            .unwrap();

        assert_eq!(search.embedding_count(), 3);

        // Remove specific embeddings
        search.remove_embeddings(&[id1, id3]);

        assert_eq!(search.embedding_count(), 1);

        // Verify correct embedding was kept - search for database content
        let results = search.search("database connection", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, id2);

        // Verify we can't find removed content with good similarity
        let json_results = search.search_with_threshold("parse JSON", 10, 0.6).unwrap();
        assert!(
            json_results.is_empty(),
            "Should not find removed JSON parsing doc"
        );

        let hash_results = search
            .search_with_threshold("calculate hash", 10, 0.6)
            .unwrap();
        assert!(
            hash_results.is_empty(),
            "Should not find removed hash calculation doc"
        );
    }

    #[test]
    #[ignore = "Downloads 86MB model - run with --ignored for semantic tests"]
    fn test_save_and_load() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();

        // Create and populate search instance
        // Skip test if model is not available
        let mut search = match SimpleSemanticSearch::new() {
            Ok(s) => s,
            Err(_) => {
                eprintln!("Skipping test: FastEmbed model not available");
                return;
            }
        };

        // Index some test data
        search
            .index_doc_comment(SymbolId::new(1).unwrap(), "This function parses JSON data")
            .unwrap();

        search
            .index_doc_comment(
                SymbolId::new(2).unwrap(),
                "Authenticates a user with credentials",
            )
            .unwrap();

        let original_count = search.embedding_count();

        // Save to disk
        search.save(temp_dir.path()).unwrap();

        // Load from disk
        let loaded = SimpleSemanticSearch::load(temp_dir.path()).unwrap();

        // Verify same number of embeddings
        assert_eq!(loaded.embedding_count(), original_count);

        // Verify search still works
        let results = loaded.search("parse JSON", 10).unwrap();
        assert!(!results.is_empty());

        // The first result should be our JSON parsing function
        assert_eq!(results[0].0, SymbolId::new(1).unwrap());
    }

    #[test]
    fn test_load_missing_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();

        // Try to load from non-existent path
        let result = SimpleSemanticSearch::load(temp_dir.path());

        assert!(result.is_err());
        match result.unwrap_err() {
            SemanticSearchError::StorageError { .. } => {}
            _ => panic!("Expected StorageError"),
        }
    }

    #[test]
    fn test_load_fails_on_embedding_count_mismatch() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Write metadata that advertises 2 embeddings.
        let metadata = SemanticMetadata::new("AllMiniLML6V2".to_string(), 4, 2);
        metadata.save(path).unwrap();

        // Write storage with only 1 embedding.
        let mut storage =
            SemanticVectorStorage::new(path, VectorDimension::new(4).unwrap()).unwrap();
        storage
            .save_embedding(SymbolId::new(1).unwrap(), &[0.1, 0.2, 0.3, 0.4])
            .unwrap();

        // Optional companion files expected by loader.
        std::fs::write(path.join("languages.json"), "{}").unwrap();
        std::fs::write(
            path.join("embedded_hashes.json"),
            r#"{"format_version":2,"hashes":{}}"#,
        )
        .unwrap();

        let err = SimpleSemanticSearch::load(path).unwrap_err();
        match err {
            SemanticSearchError::StorageError { message, .. } => {
                assert!(message.contains("Embedding count mismatch"));
            }
            other => panic!("Expected StorageError, got {other:?}"),
        }
    }

    #[test]
    #[ignore = "Downloads 86MB model - run with --ignored for semantic tests"]
    fn test_semantic_search_basic() {
        // Skip test if model is not available
        let mut search = match SimpleSemanticSearch::new() {
            Ok(s) => s,
            Err(_) => {
                eprintln!("Skipping test: FastEmbed model not available");
                return;
            }
        };

        // Index some doc comments
        let id1 = SymbolId::new(1).unwrap();
        let id2 = SymbolId::new(2).unwrap();
        let id3 = SymbolId::new(3).unwrap();

        search
            .index_doc_comment(id1, "Parse JSON data from a string")
            .unwrap();
        search
            .index_doc_comment(id2, "Serialize data structure to JSON")
            .unwrap();
        search
            .index_doc_comment(id3, "Calculate factorial of a number")
            .unwrap();

        // Search for JSON-related functions
        let results = search.search("parse JSON", 3).unwrap();

        // First two should be JSON-related
        assert!(results[0].1 > 0.7); // High similarity
        assert!(results[1].1 > 0.5); // Moderate similarity
        assert!(results[2].1 < 0.3); // Low similarity (factorial)

        // The parse function should be most similar
        assert_eq!(results[0].0, id1);
    }

    #[test]
    #[ignore = "Downloads 86MB model - run with --ignored for semantic tests"]
    fn test_similarity_threshold() {
        // Skip test if model is not available
        let mut search = match SimpleSemanticSearch::new() {
            Ok(s) => s,
            Err(_) => {
                eprintln!("Skipping test: FastEmbed model not available");
                return;
            }
        };

        // Index test data
        search
            .index_doc_comment(
                SymbolId::new(1).unwrap(),
                "Authentication and authorization",
            )
            .unwrap();
        search
            .index_doc_comment(SymbolId::new(2).unwrap(), "User login and authentication")
            .unwrap();
        search
            .index_doc_comment(SymbolId::new(3).unwrap(), "Matrix multiplication algorithm")
            .unwrap();

        // Search with threshold
        let results = search
            .search_with_threshold("user authentication", 10, 0.5)
            .unwrap();

        // Should only return auth-related results
        assert_eq!(results.len(), 2);
        for (_, score) in &results {
            assert!(*score >= 0.5);
        }
    }

    #[test]
    fn test_cosine_similarity() {
        // Identical vectors
        let v1 = vec![1.0, 0.0, 0.0];
        let v2 = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&v1, &v2) - 1.0).abs() < 0.001);

        // Orthogonal vectors
        let v3 = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&v1, &v3) - 0.0).abs() < 0.001);

        // Opposite vectors
        let v4 = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&v1, &v4) - (-1.0)).abs() < 0.001);
    }
}
