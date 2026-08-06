//! Reload the in-memory index when the one on disk changes underneath it.
//!
//! The server loads the index once at startup and keeps it in memory, which is right for
//! latency and wrong for every workflow where the index is built afterwards. The plugin
//! install is exactly that shape: the MCP server starts when the plugin loads, the user
//! then runs `quarry index .`, and the server goes on answering from the empty snapshot it
//! read at boot. Every tool returns nothing while the CLI, which reads from disk on each
//! invocation, answers correctly. That looks like a broken product rather than a stale
//! cache, and there is nothing in the output to suggest otherwise.
//!
//! `--watch` did not cover this: it re-indexes when *source files* change, so an index
//! built by another process was never noticed.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::sync::RwLock;

use crate::config::Settings;
use crate::indexing::facade::IndexFacade;
use crate::storage::IndexPersistence;

/// How often to ask the filesystem whether the index moved on. A `stat` every few seconds
/// costs nothing next to being silently wrong.
const POLL: Duration = Duration::from_secs(3);

/// The file Tantivy rewrites on every commit, and the cheapest honest signal that the
/// index is not the one we loaded.
fn stamp(index_path: &Path) -> Option<SystemTime> {
    std::fs::metadata(index_path.join("tantivy").join("meta.json"))
        .and_then(|m| m.modified())
        .ok()
}

/// Watch the index directory and swap in a freshly loaded facade when it changes.
///
/// Reload failures are logged and ignored: a half-written index during someone else's
/// commit is normal, and the next poll will pick up the finished one.
pub async fn watch_index(
    facade: Arc<RwLock<IndexFacade>>,
    settings: Arc<Settings>,
    index_path: PathBuf,
) {
    let mut seen = stamp(&index_path);

    loop {
        tokio::time::sleep(POLL).await;

        let current = stamp(&index_path);
        if current == seen {
            continue;
        }

        let persistence = IndexPersistence::new(index_path.clone());
        match persistence.load_facade(settings.clone()) {
            Ok(fresh) => {
                let symbols = fresh.symbol_count();
                *facade.write().await = fresh;
                seen = current;
                tracing::info!(
                    target: "mcp",
                    "index changed on disk, reloaded with {symbols} symbols"
                );
            }
            Err(e) => {
                tracing::debug!(target: "mcp", "index reload deferred: {e}");
            }
        }
    }
}
