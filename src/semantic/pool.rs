//! Embedding model pool for parallel embedding generation
//!
//! Provides multiple TextEmbedding instances that can be used concurrently
//! by different threads, enabling parallel embedding generation.

use crate::SymbolId;
use crate::config::SemanticBackend;
use crate::vector::EmbeddingRuntimeConfig;
use crossbeam_channel::{Receiver, Sender, bounded};
use fastembed::TextEmbedding;
use model2vec_rs::model::StaticModel;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::SemanticSearchError;

/// Model instance with an ID for tracking
struct ModelInstance {
    model: ModelBackendInstance,
    id: usize,
}

enum ModelBackendInstance {
    Fastembed(TextEmbedding),
    Model2Vec(StaticModel),
}

/// Pool of TextEmbedding models for parallel embedding generation.
///
/// Each model instance is expensive (~86MB), but having multiple allows
/// true parallel embedding generation with rayon.
pub struct EmbeddingPool {
    /// Channel to acquire models from the pool
    model_sender: Sender<ModelInstance>,
    model_receiver: Receiver<ModelInstance>,
    /// Number of models in the pool
    pool_size: usize,
    /// Model dimensions (all models have same dimensions)
    dimensions: usize,
    /// Model name for metadata
    model_name: String,
    /// Usage counters per model instance (for tracing)
    usage_counters: Vec<AtomicUsize>,
    /// Maximum total token budget per embedding request batch.
    max_batch_tokens: usize,
    /// Maximum token length for backend inference.
    max_sequence_length: usize,
}

impl EmbeddingPool {
    const DEFAULT_MAX_BATCH_TOKENS: usize = 4_096;

    /// Create a new embedding pool with the specified number of model instances.
    ///
    /// # Arguments
    /// * `pool_size` - Number of TextEmbedding instances to create
    /// * `model_name` - Name of the embedding model (e.g., "AllMiniLML6V2", "GraniteSmallEnglishR2")
    ///
    /// # Note
    /// Each model instance uses ~86MB of memory for AllMiniLML6V2.
    pub fn new(
        pool_size: usize,
        model_name: &str,
        backend: SemanticBackend,
        max_batch_tokens: usize,
        max_sequence_length: usize,
        runtime_config: Option<EmbeddingRuntimeConfig>,
    ) -> Result<Self, SemanticSearchError> {
        let pool_size = if backend == SemanticBackend::Model2vec {
            1
        } else {
            pool_size.max(1)
        };
        let (sender, receiver) = bounded(pool_size);
        let runtime_config = runtime_config.unwrap_or_default();

        let model_name_owned = model_name.to_string();

        tracing::info!(
            target: "semantic",
            "Initializing embedding pool: {pool_size} instances ({model_name}, backend={:?})",
            backend
        );

        let mut dimensions = 0;

        // Create usage counters for each model
        let usage_counters: Vec<AtomicUsize> =
            (0..pool_size).map(|_| AtomicUsize::new(0)).collect();

        // Create pool_size model instances
        for i in 0..pool_size {
            let model_instance = match backend {
                SemanticBackend::Fastembed => {
                    let show_progress = i == 0; // Only show progress for first model
                    let (mut text_model, _) = crate::vector::create_text_embedding_with_runtime(
                        model_name,
                        show_progress,
                        Some(max_sequence_length),
                        Some(&runtime_config),
                    )
                    .map_err(|e| {
                        SemanticSearchError::ModelInitError(format!(
                            "Failed to initialize model instance {}: {}",
                            i + 1,
                            e
                        ))
                    })?;

                    if i == 0 {
                        let test_embedding = text_model
                            .embed(vec!["test"], None)
                            .map_err(|e| SemanticSearchError::EmbeddingError(e.to_string()))?;
                        dimensions = test_embedding.into_iter().next().unwrap().len();
                    }
                    ModelBackendInstance::Fastembed(text_model)
                }
                SemanticBackend::Model2vec => {
                    let static_model = StaticModel::from_pretrained(model_name, None, None, None)
                        .map_err(|e| {
                        SemanticSearchError::ModelInitError(format!(
                            "Failed to initialize model2vec instance {}: {}",
                            i + 1,
                            e
                        ))
                    })?;
                    if i == 0 {
                        dimensions = static_model.encode_single("test").len();
                    }
                    ModelBackendInstance::Model2Vec(static_model)
                }
            };

            let instance = ModelInstance {
                model: model_instance,
                id: i,
            };
            sender
                .send(instance)
                .expect("Pool channel should not be closed");
        }

        tracing::info!(
            target: "semantic",
            "Embedding pool ready: {pool_size} instances, {dimensions} dimensions"
        );

        Ok(Self {
            model_sender: sender,
            model_receiver: receiver,
            pool_size,
            dimensions,
            model_name: model_name_owned,
            usage_counters,
            max_batch_tokens: max_batch_tokens.max(512),
            max_sequence_length: max_sequence_length.max(128),
        })
    }

    /// Create a pool with default model (AllMiniLML6V2)
    pub fn with_size(pool_size: usize) -> Result<Self, SemanticSearchError> {
        Self::new(
            pool_size,
            "AllMiniLML6V2",
            SemanticBackend::Fastembed,
            Self::DEFAULT_MAX_BATCH_TOKENS,
            512,
            None,
        )
    }

    /// Acquire a model from the pool (blocks if none available)
    fn acquire(&self) -> ModelInstance {
        let instance = self
            .model_receiver
            .recv()
            .expect("Pool should not be empty");

        // Increment usage counter for this model
        self.usage_counters[instance.id].fetch_add(1, Ordering::Relaxed);

        instance
    }

    /// Return a model to the pool
    fn release(&self, instance: ModelInstance) {
        let _ = self.model_sender.send(instance);
    }

    /// Get the embedding dimensions
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// Get the pool size
    pub fn pool_size(&self) -> usize {
        self.pool_size
    }

    /// Get the model name
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Generate embedding for a single document using a pooled model.
    ///
    /// Thread-safe: acquires model, generates embedding, returns model.
    pub fn embed_one(&self, text: &str) -> Result<Vec<f32>, SemanticSearchError> {
        if text.trim().is_empty() {
            return Err(SemanticSearchError::EmbeddingError(
                "Empty text".to_string(),
            ));
        }

        let mut instance = self.acquire();
        let result = match &mut instance.model {
            ModelBackendInstance::Fastembed(model) => model
                .embed(vec![text], None)
                .map_err(|e| SemanticSearchError::EmbeddingError(e.to_string()))
                .map(|mut v| v.remove(0)),
            ModelBackendInstance::Model2Vec(model) => Ok(model.encode_single(text)),
        };
        self.release(instance);
        result
    }

    /// Log usage statistics for all model instances.
    pub fn log_usage_stats(&self) {
        let counts: Vec<usize> = self
            .usage_counters
            .iter()
            .map(|c| c.load(Ordering::Relaxed))
            .collect();
        let total: usize = counts.iter().sum();

        if total > 0 {
            let usage_str: Vec<String> = counts
                .iter()
                .enumerate()
                .map(|(i, c)| format!("model[{i}]={c}"))
                .collect();

            tracing::info!(
                target: "semantic",
                "Embedding pool usage: {} (total: {total})",
                usage_str.join(", ")
            );
        }
    }

    /// Generate embeddings for multiple documents with length-aware adaptive batching.
    ///
    /// Uses adaptive batches based on estimated token length to keep memory bounded.
    /// Returns a Vec of (SymbolId, embedding, language) for successful embeddings.
    /// Failed embeddings are logged and skipped.
    pub fn embed_parallel(
        &self,
        items: &[(SymbolId, &str, &str)],
    ) -> Vec<(SymbolId, Vec<f32>, String)> {
        #[derive(Clone, Copy)]
        struct ItemRef<'a> {
            id: SymbolId,
            doc: &'a str,
            language: &'a str,
            estimated_tokens: usize,
        }

        fn estimate_tokens(text: &str) -> usize {
            // Conservative approximation for code payloads: ~2 chars/token.
            text.len().saturating_add(1).saturating_div(2).max(1)
        }

        fn max_items_for_tokens(tokens: usize) -> usize {
            match tokens {
                0..=512 => 32,
                513..=2048 => 16,
                2049..=4096 => 8,
                4097..=6144 => 4,
                _ => 2,
            }
        }

        // Filter out empty docs and pre-compute estimated token lengths.
        let mut valid_items: Vec<ItemRef<'_>> = items
            .iter()
            .filter(|(_, doc, _)| !doc.trim().is_empty())
            .map(|(id, doc, language)| ItemRef {
                id: *id,
                doc,
                language,
                estimated_tokens: estimate_tokens(doc),
            })
            .collect();

        if valid_items.is_empty() {
            return Vec::new();
        }

        // Reduce ONNX padding overhead by grouping similar lengths.
        valid_items.sort_by_key(|item| item.estimated_tokens);

        let mut results = Vec::with_capacity(valid_items.len());
        let mut cursor = 0usize;
        while cursor < valid_items.len() {
            let first = valid_items[cursor];
            let mut max_items = max_items_for_tokens(first.estimated_tokens);
            if max_items == 0 {
                max_items = 1;
            }

            let mut batch = Vec::with_capacity(max_items);
            let mut batch_tokens = 0usize;
            while cursor < valid_items.len() && batch.len() < max_items {
                let candidate = valid_items[cursor];
                if !batch.is_empty()
                    && batch_tokens + candidate.estimated_tokens > self.max_batch_tokens
                {
                    break;
                }
                batch_tokens += candidate.estimated_tokens;
                batch.push(candidate);
                cursor += 1;
            }
            if batch.is_empty() {
                batch.push(valid_items[cursor]);
                cursor += 1;
            }

            let texts: Vec<&str> = batch.iter().map(|item| item.doc).collect();

            let mut instance = self.acquire();
            let embeddings_result = match &mut instance.model {
                ModelBackendInstance::Fastembed(model) => model
                    .embed(texts, None)
                    .map_err(|e| SemanticSearchError::EmbeddingError(e.to_string())),
                ModelBackendInstance::Model2Vec(model) => {
                    let text_strings: Vec<String> = texts
                        .into_iter()
                        .map(std::string::ToString::to_string)
                        .collect();
                    Ok(model.encode_with_args(&text_strings, Some(self.max_sequence_length), 1024))
                }
            };
            self.release(instance);

            match embeddings_result {
                Ok(embeddings) => {
                    for (item, embedding) in batch.iter().zip(embeddings.into_iter()) {
                        if embedding.len() == self.dimensions {
                            results.push((item.id, embedding, item.language.to_string()));
                        } else {
                            tracing::warn!(
                                target: "semantic",
                                "Dimension mismatch for {}: expected {}, got {}",
                                item.id.to_u32(),
                                self.dimensions,
                                embedding.len()
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        target: "semantic",
                        "Batch embedding failed (items={}, est_tokens={}): {}",
                        batch.len(),
                        batch_tokens,
                        e
                    );
                }
            }
        }

        // Log usage stats after embedding run
        self.log_usage_stats();

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Downloads 86MB model - run with --ignored"]
    fn test_pool_creation() {
        let pool = EmbeddingPool::with_size(2).unwrap();
        assert_eq!(pool.pool_size(), 2);
        assert_eq!(pool.dimensions(), 384); // AllMiniLML6V2
    }

    #[test]
    #[ignore = "Downloads 86MB model - run with --ignored"]
    fn test_parallel_embedding() {
        let pool = EmbeddingPool::with_size(2).unwrap();

        let items = vec![
            (SymbolId::new(1).unwrap(), "Parse JSON data", "rust"),
            (SymbolId::new(2).unwrap(), "Connect to database", "rust"),
            (SymbolId::new(3).unwrap(), "Calculate hash", "rust"),
        ];

        let results = pool.embed_parallel(&items);
        assert_eq!(results.len(), 3);
    }
}
