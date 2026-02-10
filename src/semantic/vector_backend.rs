//! Lightweight symbol vector search backend using random-access mmap.
//!
//! Instead of loading all embeddings into a HashMap (~148 MB heap for 75K symbols),
//! this struct holds only:
//! - `binary_index` (~5.5 MB) for Hamming pre-filter
//! - `id_offset_map` (~2 MB) for random mmap access
//! - `languages` (~1.5 MB) for language filtering
//! - query model (~31 MB for model2vec) for embedding queries
//!
//! Search reads only ~500 candidate vectors from mmap pages.

use crate::semantic::static_model::OptimizedStaticModel;
use crate::semantic::{SemanticMetadata, SimpleSemanticSearch};
use crate::vector::{
    cosine_similarity, BinaryVector, MmapVectorStorage, SegmentOrdinal,
};
use crate::{IndexError, SymbolId};
use fastembed::TextEmbedding;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

/// Query-time embedding model for symbol vector search.
enum QueryModel {
    Optimized(OptimizedStaticModel),
    Fastembed(Mutex<TextEmbedding>),
}

/// Cached symbol vector search backend using random-access mmap.
pub struct SymbolVectorBackend {
    binary_index: HashMap<u32, BinaryVector>,
    id_offset_map: HashMap<u32, usize>,
    languages: HashMap<u32, String>,
    storage: MmapVectorStorage,
    model: QueryModel,
    embedding_count: usize,
}

impl SymbolVectorBackend {
    /// Open the symbol semantic index at the given path.
    pub fn open(semantic_path: &Path) -> Result<Self, IndexError> {
        if !semantic_path.join("metadata.json").exists() {
            return Err(IndexError::General(
                "symbol semantic metadata not found".to_string(),
            ));
        }

        let metadata = SemanticMetadata::load(semantic_path).map_err(|e| {
            IndexError::General(format!("failed to load symbol semantic metadata: {e}"))
        })?;

        let mut storage =
            MmapVectorStorage::open(semantic_path, SegmentOrdinal::new(0)).map_err(|e| {
                IndexError::General(format!("failed to open symbol vector storage: {e}"))
            })?;
        let id_offset_map = storage.build_id_offset_map().map_err(|e| {
            IndexError::General(format!("failed to build symbol id-offset map: {e}"))
        })?;

        let binary_index = load_binary_index(semantic_path)?;
        let languages = load_languages(semantic_path);
        let model = init_model(&metadata.model_name)?;
        let embedding_count = metadata.embedding_count;

        Ok(Self {
            binary_index,
            id_offset_map,
            languages,
            storage,
            model,
            embedding_count,
        })
    }

    /// Vector search returning `(SymbolId, cosine_score)` pairs.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
        language_filter: Option<&str>,
    ) -> Result<Vec<(SymbolId, f32)>, IndexError> {
        if limit == 0 || self.binary_index.is_empty() {
            return Ok(Vec::new());
        }

        let query_embedding = self.embed_query(query)?;
        let query_binary = BinaryVector::from_embedding(&query_embedding);

        let prefilter_limit = (limit * 5).max(100);
        let mut hamming_scores: Vec<(u32, u32)> = self
            .binary_index
            .iter()
            .filter(|(id, _)| match language_filter {
                Some(lang) => self.languages.get(id).is_some_and(|l| l == lang),
                None => true,
            })
            .map(|(&id, bv)| (id, query_binary.hamming_distance(bv)))
            .collect();

        hamming_scores.sort_unstable_by_key(|(_, dist)| *dist);
        hamming_scores.truncate(prefilter_limit);

        let ids_offsets: Vec<(u32, usize)> = hamming_scores
            .iter()
            .filter_map(|(id, _)| self.id_offset_map.get(id).map(|&off| (*id, off)))
            .collect();

        let vectors = self.storage.read_vectors_by_offsets(&ids_offsets);

        let mut results: Vec<(SymbolId, f32)> = vectors
            .into_iter()
            .filter_map(|(id, emb)| {
                SymbolId::new(id).map(|sid| (sid, cosine_similarity(&query_embedding, &emb)))
            })
            .collect();

        results
            .sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        Ok(results)
    }

    /// Number of indexed embeddings (from metadata).
    pub fn embedding_count(&self) -> usize {
        self.embedding_count
    }

    /// Number of binary index entries.
    pub fn binary_index_count(&self) -> usize {
        self.binary_index.len()
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>, IndexError> {
        match &self.model {
            QueryModel::Optimized(m) => Ok(m.encode_single(text)),
            QueryModel::Fastembed(m) => {
                let mut guard = m
                    .lock()
                    .map_err(|_| IndexError::General("symbol embed lock poisoned".to_string()))?;
                let embeddings = guard
                    .embed(vec![text], None)
                    .map_err(|e| IndexError::General(format!("symbol query embed failed: {e}")))?;
                embeddings
                    .into_iter()
                    .next()
                    .ok_or_else(|| IndexError::General("no embedding returned".to_string()))
            }
        }
    }
}

fn init_model(model_name: &str) -> Result<QueryModel, IndexError> {
    if SimpleSemanticSearch::looks_like_model2vec_model(model_name) {
        let source = model_name.strip_prefix("model2vec:").unwrap_or(model_name);
        let opt = OptimizedStaticModel::from_local(source).map_err(|e| {
            IndexError::General(format!(
                "failed to load symbol vector model '{model_name}': {e}"
            ))
        })?;
        return Ok(QueryModel::Optimized(opt));
    }
    let (te, _dims) =
        crate::vector::create_text_embedding_with_max_length(model_name, false, Some(1024))
            .map_err(|e| {
                IndexError::General(format!(
                    "failed to load symbol vector model '{model_name}': {e}"
                ))
            })?;
    Ok(QueryModel::Fastembed(Mutex::new(te)))
}

fn load_binary_index(semantic_path: &Path) -> Result<HashMap<u32, BinaryVector>, IndexError> {
    let bin_path = semantic_path.join("binary_index.bin");
    let data = std::fs::read(&bin_path)
        .map_err(|e| IndexError::General(format!("failed to read binary_index.bin: {e}")))?;
    parse_binary_index(&data)
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
        let bv_len = u32::from_le_bytes(data[offset..offset + 4].try_into().ok()?) as usize;
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
