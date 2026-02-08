//! Semantic embedding stage - parallel embedding generation
//!
//! COLLECT ─┬─> EMBED (this stage) ─> SimpleSemanticSearch
//!          └─> INDEX ─> Tantivy
//!
//! Receives EmbeddingBatch from COLLECT, generates embeddings using EmbeddingPool,
//! stores them in SimpleSemanticSearch. Runs in parallel with INDEX stage.

use crate::indexing::pipeline::types::{EmbeddingBatch, PipelineError, PipelineResult};
use crate::semantic::{
    EmbeddingPool, SemanticWorkerClient, SemanticWorkerClientConfig, SimpleSemanticSearch,
};
use crossbeam_channel::Receiver;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Statistics from the EMBED stage.
#[derive(Debug, Clone, Default)]
pub struct SemanticEmbedStats {
    /// Total candidates received
    pub received: usize,
    /// Successfully embedded
    pub embedded: usize,
    /// Skipped (empty doc, dimension mismatch)
    pub skipped: usize,
    /// Skipped via hash-match (lazy embedding)
    pub skipped_cached: usize,
    /// Time waiting on input channel
    pub input_wait: Duration,
    /// Total processing time
    pub elapsed: Duration,
}

impl SemanticEmbedStats {
    /// Check if all received candidates were processed.
    pub fn is_complete(&self) -> bool {
        self.embedded + self.skipped + self.skipped_cached == self.received
    }
}

/// Progress callback type for EMBED stage.
pub type EmbedProgressCallback = Arc<dyn Fn(u64) + Send + Sync>;

enum EmbedBackend {
    Pool(Arc<EmbeddingPool>),
    Worker(Mutex<SemanticWorkerClient>),
}

/// Semantic embedding stage using EmbeddingPool.
///
/// Receives EmbeddingBatch from COLLECT, generates embeddings in parallel,
/// stores them in SimpleSemanticSearch.
pub struct SemanticEmbedStage {
    backend: EmbedBackend,
    semantic: Arc<Mutex<SimpleSemanticSearch>>,
    progress_callback: Option<EmbedProgressCallback>,
}

impl SemanticEmbedStage {
    const MAX_ITEMS_PER_POOL_CALL: usize = 256;

    /// Create a new semantic embed stage.
    pub fn new(pool: Arc<EmbeddingPool>, semantic: Arc<Mutex<SimpleSemanticSearch>>) -> Self {
        Self {
            backend: EmbedBackend::Pool(pool),
            semantic,
            progress_callback: None,
        }
    }

    /// Create a semantic embed stage backed by a worker subprocess.
    pub fn with_worker(
        semantic: Arc<Mutex<SimpleSemanticSearch>>,
        config: SemanticWorkerClientConfig,
    ) -> PipelineResult<Self> {
        let worker =
            SemanticWorkerClient::new(config).map_err(|e| PipelineError::ChannelRecv(e))?;
        Ok(Self {
            backend: EmbedBackend::Worker(Mutex::new(worker)),
            semantic,
            progress_callback: None,
        })
    }

    /// Add a progress callback that receives the count of embeddings processed per batch.
    pub fn with_progress(mut self, callback: EmbedProgressCallback) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    /// Run the embed stage.
    ///
    /// Receives EmbeddingBatch from channel, generates embeddings, stores them.
    /// Runs until channel is closed.
    pub fn run(&self, receiver: Receiver<EmbeddingBatch>) -> PipelineResult<SemanticEmbedStats> {
        const STATS_LOG_INTERVAL: Duration = Duration::from_secs(10);

        tracing::info!(target: "semantic", "EMBED stage started, waiting for batches...");

        let start = Instant::now();
        let mut stats = SemanticEmbedStats::default();
        let mut last_stats_log = Instant::now();
        let mut batches_received = 0usize;

        loop {
            let recv_start = Instant::now();
            match receiver.recv() {
                Ok(batch) => {
                    batches_received += 1;
                    stats.input_wait += recv_start.elapsed();

                    let candidate_count = batch.candidates.len();
                    stats.received += candidate_count;

                    tracing::debug!(
                        target: "semantic",
                        "EMBED batch {}: {} candidates",
                        batches_received,
                        candidate_count
                    );

                    if !batch.candidates.is_empty() {
                        let (count, cached) = self.process_batch(&batch)?;
                        stats.embedded += count;
                        stats.skipped_cached += cached;
                        stats.skipped += candidate_count - count - cached;

                        // Report progress
                        if let Some(ref callback) = self.progress_callback {
                            callback(count as u64);
                        }

                        // Log pool stats periodically
                        if last_stats_log.elapsed() >= STATS_LOG_INTERVAL {
                            if let EmbedBackend::Pool(pool) = &self.backend {
                                pool.log_usage_stats();
                            }
                            last_stats_log = Instant::now();
                        }
                    }
                }
                Err(_) => break, // Channel closed
            }
        }

        // Log final pool stats
        if let EmbedBackend::Pool(pool) = &self.backend {
            pool.log_usage_stats();
        }

        stats.elapsed = start.elapsed();

        tracing::info!(
            target: "semantic",
            "EMBED complete: {}/{} embedded, {} cached skip in {:?} ({} batches)",
            stats.embedded,
            stats.received,
            stats.skipped_cached,
            stats.elapsed,
            batches_received
        );

        Ok(stats)
    }

    /// Process a batch of embedding candidates.
    ///
    /// Returns (embedded_count, skipped_cached_count).
    fn process_batch(&self, batch: &EmbeddingBatch) -> PipelineResult<(usize, usize)> {
        // Build file hash lookup for per-file cache checks.
        let file_hash_by_key: HashMap<_, _> = batch
            .file_hashes
            .iter()
            .map(|(file_key, hash)| (file_key.as_ref(), hash.as_str()))
            .collect();
        let file_key_by_id: HashMap<_, _> = batch
            .file_keys
            .iter()
            .map(|(file_id, file_key)| (*file_id, file_key.as_ref()))
            .collect();

        // Build embedding input from only non-cached files.
        let mut items = Vec::with_capacity(batch.candidates.len());
        let mut symbol_to_file = HashMap::<crate::SymbolId, &str>::new();
        let mut expected_by_file = HashMap::<&str, usize>::new();
        let mut skipped_cached = 0usize;
        {
            let sem = self.semantic.lock().map_err(|_| PipelineError::Parse {
                path: std::path::PathBuf::new(),
                reason: "Failed to lock semantic search".to_string(),
            })?;
            for (file_id, symbol_id, doc, lang) in &batch.candidates {
                let Some(file_key) = file_key_by_id.get(file_id) else {
                    continue;
                };
                if let Some(hash) = file_hash_by_key.get(file_key) {
                    if sem.is_file_embedded(file_key, hash) {
                        skipped_cached += 1;
                        continue;
                    }
                }

                items.push((*symbol_id, doc.as_ref(), lang.as_ref()));
                symbol_to_file.insert(*symbol_id, *file_key);
                *expected_by_file.entry(*file_key).or_default() += 1;
            }
        }

        if items.is_empty() {
            return Ok((0, skipped_cached));
        }

        // Generate embeddings with bounded per-call size to avoid large transient allocations.
        let mut embedded_symbol_ids = HashSet::with_capacity(items.len());
        let mut count = 0usize;
        for chunk in items.chunks(Self::MAX_ITEMS_PER_POOL_CALL) {
            let embeddings = self.embed_items(chunk)?;
            if embeddings.is_empty() {
                continue;
            }

            for (symbol_id, _, _) in &embeddings {
                embedded_symbol_ids.insert(*symbol_id);
            }

            let mut semantic = self.semantic.lock().map_err(|_| PipelineError::Parse {
                path: std::path::PathBuf::new(),
                reason: "Failed to lock semantic search".to_string(),
            })?;
            count += semantic.store_embeddings(embeddings);
        }

        if !embedded_symbol_ids.is_empty() {
            let mut embedded_by_file = HashMap::<&str, usize>::new();
            for symbol_id in &embedded_symbol_ids {
                if let Some(file_key) = symbol_to_file.get(symbol_id) {
                    *embedded_by_file.entry(*file_key).or_default() += 1;
                }
            }

            let mut semantic = self.semantic.lock().map_err(|_| PipelineError::Parse {
                path: std::path::PathBuf::new(),
                reason: "Failed to lock semantic search".to_string(),
            })?;

            for (file_key, expected_count) in expected_by_file {
                let embedded_count = embedded_by_file.get(file_key).copied().unwrap_or(0);
                if embedded_count != expected_count {
                    continue;
                }
                if let Some(hash) = file_hash_by_key.get(file_key) {
                    semantic.mark_file_embedded(file_key.to_string(), (*hash).to_string());
                }
            }
        }

        Ok((count, skipped_cached))
    }

    fn embed_items(
        &self,
        items: &[(crate::SymbolId, &str, &str)],
    ) -> PipelineResult<Vec<(crate::SymbolId, Vec<f32>, String)>> {
        match &self.backend {
            EmbedBackend::Pool(pool) => Ok(pool.embed_parallel(items)),
            EmbedBackend::Worker(worker) => {
                let mut worker_guard = worker.lock().map_err(|_| {
                    PipelineError::ChannelRecv("Failed to lock semantic worker client".to_string())
                })?;
                worker_guard.embed_parallel(items).map_err(|e| {
                    PipelineError::ChannelRecv(format!("Semantic worker embed failed: {e}"))
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_embed_stats_is_complete() {
        let mut stats = SemanticEmbedStats::default();
        assert!(stats.is_complete()); // 0 received, 0 processed

        stats.received = 10;
        stats.embedded = 8;
        stats.skipped = 2;
        assert!(stats.is_complete());

        stats.skipped = 1;
        assert!(!stats.is_complete()); // 8 + 1 != 10
    }
}
