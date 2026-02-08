use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use super::Sha256Hash;

/// Simple thread-safe in-memory memoization map keyed by `Sha256Hash`.
/// Value type must be `Clone` to allow cheap reads without exposing interior mutability.
pub struct ResolutionMemo<V> {
    inner: RwLock<HashMap<Sha256Hash, Arc<V>>>,
}

impl<V> Default for ResolutionMemo<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> ResolutionMemo<V> {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    pub fn insert(&self, key: Sha256Hash, value: V) {
        let mut map = self.inner.write();
        map.insert(key, Arc::new(value));
    }

    pub fn get(&self, key: &Sha256Hash) -> Option<Arc<V>> {
        let map = self.inner.read();
        map.get(key).cloned()
    }

    pub fn clear(&self) {
        let mut map = self.inner.write();
        map.clear();
    }
}
