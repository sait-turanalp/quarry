//! Embed stage - vector embedding generation
//!
//! Generates embeddings for symbols and stores them in the vector index.
//! Runs after COLLECT stage, parallel to or after INDEX stage.
//!
//! Data flow:
//! - Receives: Vec<(SymbolId, EmbeddingText)> from COLLECT
//! - Uses: EmbeddingGenerator.generate_embeddings()
//! - Stores: VectorSearchEngine.index_vectors()
//! - Mapping: VectorId = SymbolId (same u32 value)

use crate::types::SymbolId;
use crate::vector::{EmbeddingGenerator, VectorError, VectorId, VectorSearchEngine};

/// Batch size for embedding generation.
/// Balances memory usage with batch efficiency.
const EMBED_BATCH_SIZE: usize = 256;

/// Embed stage for vector embedding generation.
pub struct EmbedStage<G: EmbeddingGenerator> {
    generator: G,
}

/// Result of embedding a batch of symbols.
#[derive(Debug, Default)]
pub struct EmbedStats {
    /// Number of symbols embedded
    pub symbols_embedded: usize,
    /// Number of batches processed
    pub batches_processed: usize,
    /// Number of symbols that failed to embed
    pub failed: usize,
}

impl<G: EmbeddingGenerator> EmbedStage<G> {
    /// Create a new embed stage with the given generator.
    pub fn new(generator: G) -> Self {
        Self { generator }
    }

    /// Get the embedding generator.
    pub fn generator(&self) -> &G {
        &self.generator
    }

    /// Embed a batch of symbols and store in the vector engine.
    ///
    /// # Arguments
    /// * `symbols` - Vec of (SymbolId, embedding_text) pairs
    /// * `engine` - Vector search engine for storage
    ///
    /// # Returns
    /// Statistics about the embedding operation
    pub fn embed_and_store(
        &self,
        symbols: &[(SymbolId, String)],
        engine: &mut VectorSearchEngine,
    ) -> Result<EmbedStats, VectorError> {
        if symbols.is_empty() {
            return Ok(EmbedStats::default());
        }

        let mut stats = EmbedStats::default();

        // Process in batches to manage memory
        for chunk in symbols.chunks(EMBED_BATCH_SIZE) {
            // Extract texts for embedding
            let texts: Vec<&str> = chunk.iter().map(|(_, text)| text.as_str()).collect();

            // Generate embeddings
            let embeddings = self.generator.generate_embeddings(&texts)?;

            // Create (VectorId, Vec<f32>) pairs
            let mut vector_pairs = Vec::with_capacity(chunk.len());
            for ((symbol_id, _), embedding) in chunk.iter().zip(embeddings.into_iter()) {
                // Map SymbolId to VectorId (same u32 value)
                if let Some(vector_id) = VectorId::new(symbol_id.value()) {
                    vector_pairs.push((vector_id, embedding));
                } else {
                    stats.failed += 1;
                }
            }

            // Index vectors
            engine.index_vectors(&vector_pairs)?;

            stats.symbols_embedded += vector_pairs.len();
            stats.batches_processed += 1;
        }

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::MockEmbeddingGenerator;

    #[test]
    fn test_embed_stage_creation() {
        let generator = MockEmbeddingGenerator::new();
        let stage = EmbedStage::new(generator);

        assert_eq!(stage.generator().dimension().get(), 384);
    }

    #[test]
    fn test_embed_empty_batch() {
        use crate::vector::VectorDimension;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let generator = MockEmbeddingGenerator::new();
        let stage = EmbedStage::new(generator);

        let mut engine =
            VectorSearchEngine::new(temp_dir.path(), VectorDimension::dimension_384()).unwrap();

        let symbols: Vec<(SymbolId, String)> = vec![];
        let stats = stage.embed_and_store(&symbols, &mut engine).unwrap();

        assert_eq!(stats.symbols_embedded, 0);
        assert_eq!(stats.batches_processed, 0);
    }

    #[test]
    fn test_embed_single_symbol() {
        use crate::vector::VectorDimension;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let generator = MockEmbeddingGenerator::new();
        let stage = EmbedStage::new(generator);

        let mut engine =
            VectorSearchEngine::new(temp_dir.path(), VectorDimension::dimension_384()).unwrap();

        let symbol_id = SymbolId::new(1).unwrap();
        let text =
            "# test.rs\n# function fn test_fn() -> i32\nA test function\nfn test_fn() -> i32"
                .to_string();

        let symbols = vec![(symbol_id, text)];
        let stats = stage.embed_and_store(&symbols, &mut engine).unwrap();

        assert_eq!(stats.symbols_embedded, 1);
        assert_eq!(stats.batches_processed, 1);
        assert_eq!(stats.failed, 0);
    }

    #[test]
    fn test_embed_generates_correct_vectors() {
        let generator = MockEmbeddingGenerator::new();
        let stage = EmbedStage::new(generator);

        let symbols: Vec<(SymbolId, String)> = (1..=10)
            .map(|i| {
                let id = SymbolId::new(i).unwrap();
                let text =
                    format!("# test.rs\n# function fn symbol_{i}()\nDocumentation for symbol {i}");
                (id, text)
            })
            .collect();

        let texts: Vec<&str> = symbols.iter().map(|(_, t)| t.as_str()).collect();
        let embeddings = stage.generator().generate_embeddings(&texts).unwrap();

        assert_eq!(embeddings.len(), 10);
        for embedding in &embeddings {
            assert_eq!(embedding.len(), 384);
        }
    }

    #[test]
    fn test_symbol_id_to_vector_id_mapping() {
        for i in 1..=100u32 {
            let symbol_id = SymbolId::new(i).unwrap();
            let vector_id = VectorId::new(symbol_id.value());

            assert!(vector_id.is_some());
            assert_eq!(vector_id.unwrap().get(), i);
        }

        assert!(VectorId::new(0).is_none());
    }

    #[test]
    fn test_batch_chunking_logic() {
        let count = EMBED_BATCH_SIZE + 100;
        let symbols: Vec<(SymbolId, String)> = (1..=count as u32)
            .map(|i| {
                let id = SymbolId::new(i).unwrap();
                let text = format!("function sym_{i}");
                (id, text)
            })
            .collect();

        let chunks: Vec<_> = symbols.chunks(EMBED_BATCH_SIZE).collect();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), EMBED_BATCH_SIZE);
        assert_eq!(chunks[1].len(), 100);
    }

    #[test]
    fn test_embed_and_store_integration() {
        use crate::vector::VectorDimension;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let generator = MockEmbeddingGenerator::new();
        let stage = EmbedStage::new(generator);

        let mut engine =
            VectorSearchEngine::new(temp_dir.path(), VectorDimension::dimension_384()).unwrap();

        let keywords = ["parse", "json", "error", "async", "validate", "format"];
        let symbols: Vec<(SymbolId, String)> = keywords
            .iter()
            .enumerate()
            .map(|(i, keyword)| {
                let id = SymbolId::new((i + 1) as u32).unwrap();
                let text = format!("function {keyword}_handler handles {keyword} operations");
                (id, text)
            })
            .collect();

        let stats = stage.embed_and_store(&symbols, &mut engine).unwrap();

        assert_eq!(stats.symbols_embedded, 6);
        assert_eq!(stats.batches_processed, 1);
        assert_eq!(stats.failed, 0);
    }
}
