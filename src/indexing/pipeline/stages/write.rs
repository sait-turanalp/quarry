//! Write stage - store resolved relationships to Tantivy
//!
//! Final stage of Phase 2: takes ResolvedBatch from RESOLVE stage
//! and writes relationships to DocumentIndex.
//!
//! Data flow:
//! - Receives: ResolvedBatch from RESOLVE stage
//! - Writes: Relationships to Tantivy via DocumentIndex
//! - Outputs: WriteStats with counts

use crate::indexing::pipeline::types::{ResolvedBatch, ResolvedRelationship};
use crate::relationship::Relationship;
use crate::storage::DocumentIndex;
use std::sync::Arc;

/// Write stage for storing resolved relationships.
///
/// Single-threaded: Tantivy IndexWriter is not Send.
/// Does NOT auto-commit — callers must explicitly call `commit()` or `flush()`.
/// This avoids the overhead of ~37 intermediate commits (segment finalize + fsync)
/// for the typical 372K relationships in the LINKS phase.
pub struct WriteStage {
    index: Arc<DocumentIndex>,
    /// Number of relationships written since last commit
    written_since_commit: usize,
    /// Whether a batch is currently active
    batch_started: bool,
}

/// Statistics from write operations.
#[derive(Debug, Default, Clone)]
pub struct WriteStats {
    /// Total relationships written
    pub written: usize,
    /// Number of commits performed
    pub commits: usize,
    /// Failed writes (logged but not fatal)
    pub failed: usize,
}

impl WriteStage {
    /// Create a new write stage with the document index.
    pub fn new(index: Arc<DocumentIndex>) -> Self {
        Self {
            index,
            written_since_commit: 0,
            batch_started: false,
        }
    }

    /// Ensure a batch is started before writing.
    fn ensure_batch_started(&mut self) -> Result<(), crate::storage::StorageError> {
        if !self.batch_started {
            self.index.start_batch()?;
            self.batch_started = true;
        }
        Ok(())
    }

    /// Write a batch of resolved relationships.
    ///
    /// Uses `store_relationships_batch` to acquire the RwLock once for the
    /// entire batch instead of per-relationship. Documents are queued in
    /// Tantivy's internal buffer; no commit until `commit()` or `flush()`.
    pub fn write(&mut self, batch: ResolvedBatch) -> WriteStats {
        let mut stats = WriteStats::default();

        if batch.relationships.is_empty() {
            return stats;
        }

        // Ensure batch is started before writing
        if let Err(e) = self.ensure_batch_started() {
            tracing::warn!(target: "pipeline", "Failed to start batch: {e}");
            stats.failed = batch.relationships.len();
            return stats;
        }

        // Convert to (from, to, Relationship) tuples for batch write
        let rels: Vec<_> = batch
            .relationships
            .into_iter()
            .map(|r| {
                let rel = Relationship {
                    kind: r.kind,
                    weight: 1.0,
                    metadata: r.metadata,
                };
                (r.from_id, r.to_id, rel)
            })
            .collect();

        match self.index.store_relationships_batch(&rels) {
            Ok((written, failed)) => {
                stats.written = written;
                stats.failed = failed;
                self.written_since_commit += written;
            }
            Err(e) => {
                tracing::warn!(target: "pipeline", "Batch write failed: {e}");
                stats.failed = rels.len();
            }
        }

        stats
    }

    /// Write a single resolved relationship.
    pub fn write_one(
        &mut self,
        resolved: ResolvedRelationship,
    ) -> Result<(), crate::storage::StorageError> {
        self.ensure_batch_started()?;

        let relationship = Relationship {
            kind: resolved.kind,
            weight: 1.0,
            metadata: resolved.metadata.clone(),
        };

        self.index
            .store_relationship(resolved.from_id, resolved.to_id, &relationship)?;
        self.written_since_commit += 1;

        Ok(())
    }

    /// Commit pending writes to disk.
    ///
    /// Call after each pass (Defines, then Calls) to ensure
    /// Pass 2 can query Pass 1 results.
    pub fn commit(&mut self) -> Result<usize, crate::storage::StorageError> {
        let count = self.written_since_commit;
        if count > 0 && self.batch_started {
            self.index.commit_batch()?;
            self.written_since_commit = 0;
            // Start new batch for subsequent writes
            self.index.start_batch()?;
        }
        Ok(count)
    }

    /// Flush any remaining pending writes.
    ///
    /// Call at end of Phase 2 to ensure all relationships are committed.
    pub fn flush(&mut self) -> Result<WriteStats, crate::storage::StorageError> {
        let written = self.written_since_commit;
        if written > 0 && self.batch_started {
            self.index.commit_batch()?;
            self.written_since_commit = 0;
            self.batch_started = false;
        }
        Ok(WriteStats {
            written,
            commits: if written > 0 { 1 } else { 0 },
            failed: 0,
        })
    }

    /// Get count of pending (uncommitted) relationships.
    pub fn pending_count(&self) -> usize {
        self.written_since_commit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RelationKind;
    use crate::config::Settings;
    use crate::types::SymbolId;
    use tempfile::TempDir;

    fn make_resolved(from: u32, to: u32, kind: RelationKind) -> ResolvedRelationship {
        ResolvedRelationship {
            from_id: SymbolId::new(from).unwrap(),
            to_id: SymbolId::new(to).unwrap(),
            kind,
            metadata: None,
        }
    }

    fn setup_index() -> (TempDir, Arc<DocumentIndex>) {
        let dir = TempDir::new().unwrap();
        let settings = Settings::default();
        let index = DocumentIndex::new(dir.path(), &settings).unwrap();
        (dir, Arc::new(index))
    }

    #[test]
    fn test_write_stage_basic() {
        let (_dir, index) = setup_index();
        let mut stage = WriteStage::new(index);

        let mut batch = ResolvedBatch::new();
        batch.push(make_resolved(1, 2, RelationKind::Calls));
        batch.push(make_resolved(3, 4, RelationKind::Defines));

        let stats = stage.write(batch);
        assert_eq!(stats.written, 2);
        assert_eq!(stats.failed, 0);
        assert_eq!(stats.commits, 0); // No auto-commit

        let flush_stats = stage.flush().unwrap();
        assert_eq!(flush_stats.written, 2);
        assert_eq!(flush_stats.commits, 1);
    }

    #[test]
    fn test_write_stage_empty_batch() {
        let (_dir, index) = setup_index();
        let mut stage = WriteStage::new(index);

        let batch = ResolvedBatch::new();
        let stats = stage.write(batch);
        assert_eq!(stats.written, 0);

        let flush_stats = stage.flush().unwrap();
        assert_eq!(flush_stats.written, 0);
        assert_eq!(flush_stats.commits, 0);
    }

    #[test]
    fn test_write_stage_commit_and_continue() {
        let (_dir, index) = setup_index();
        let mut stage = WriteStage::new(index);

        // Write first batch
        let mut batch1 = ResolvedBatch::new();
        batch1.push(make_resolved(1, 2, RelationKind::Defines));
        stage.write(batch1);

        // Commit (like end of Pass 1)
        let committed = stage.commit().unwrap();
        assert_eq!(committed, 1);

        // Write second batch
        let mut batch2 = ResolvedBatch::new();
        batch2.push(make_resolved(3, 4, RelationKind::Calls));
        stage.write(batch2);

        // Flush (like end of Pass 2)
        let flush_stats = stage.flush().unwrap();
        assert_eq!(flush_stats.written, 1);
    }
}
