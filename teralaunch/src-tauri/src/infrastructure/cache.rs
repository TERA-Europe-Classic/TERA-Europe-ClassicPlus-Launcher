//! Hash cache abstraction for testability.
//!
//! This module provides a trait for caching file hashes, allowing the application
//! to use mock implementations in tests while using a real cache in production.

use crate::domain::CachedFileInfo;
use std::collections::HashMap;
use std::sync::RwLock;

/// Trait for hash cache operations, allowing mocking in tests.
pub trait HashCache: Send + Sync {
    /// Get cached information for a file path.
    fn get(&self, path: &str) -> Option<CachedFileInfo>;

    /// Set cached information for a file path.
    fn set(&self, path: &str, info: CachedFileInfo);

    /// Invalidate (remove) cached information for a file path.
    fn invalidate(&self, path: &str);

    /// Clear all cached information.
    fn clear(&self);

    /// Get the number of cached entries.
    fn len(&self) -> usize;

    /// Check if the cache is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// In-memory hash cache implementation using a RwLock-protected HashMap.
pub struct InMemoryHashCache {
    cache: RwLock<HashMap<String, CachedFileInfo>>,
}

impl InMemoryHashCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Create a cache with pre-populated entries.
    pub fn with_entries(entries: HashMap<String, CachedFileInfo>) -> Self {
        Self {
            cache: RwLock::new(entries),
        }
    }

    /// Get a snapshot of all cached entries.
    pub fn snapshot(&self) -> HashMap<String, CachedFileInfo> {
        self.cache.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Load entries into the cache, replacing existing ones.
    pub fn load(&self, entries: HashMap<String, CachedFileInfo>) {
        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());
        *cache = entries;
    }
}

impl Default for InMemoryHashCache {
    fn default() -> Self {
        Self::new()
    }
}

impl HashCache for InMemoryHashCache {
    fn get(&self, path: &str) -> Option<CachedFileInfo> {
        self.cache
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(path)
            .cloned()
    }

    fn set(&self, path: &str, info: CachedFileInfo) {
        self.cache
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(path.to_string(), info);
    }

    fn invalidate(&self, path: &str) {
        self.cache
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(path);
    }

    fn clear(&self) {
        self.cache
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    fn len(&self) -> usize {
        self.cache.read().unwrap_or_else(|e| e.into_inner()).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn make_cached_info(hash: &str) -> CachedFileInfo {
        CachedFileInfo {
            hash: hash.to_string(),
            last_modified: SystemTime::now(),
        }
    }

    #[test]
    fn cache_get_returns_none_for_missing() {
        let cache = InMemoryHashCache::new();
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn cache_set_and_get() {
        let cache = InMemoryHashCache::new();
        let info = make_cached_info("abc123");

        cache.set("file.txt", info.clone());
        let retrieved = cache.get("file.txt").unwrap();

        assert_eq!(retrieved.hash, "abc123");
    }

    #[test]
    fn cache_invalidate() {
        let cache = InMemoryHashCache::new();
        cache.set("file.txt", make_cached_info("abc123"));

        assert!(cache.get("file.txt").is_some());
        cache.invalidate("file.txt");
        assert!(cache.get("file.txt").is_none());
    }

    #[test]
    fn cache_clear() {
        let cache = InMemoryHashCache::new();
        cache.set("file1.txt", make_cached_info("hash1"));
        cache.set("file2.txt", make_cached_info("hash2"));

        assert_eq!(cache.len(), 2);
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_len() {
        let cache = InMemoryHashCache::new();
        assert_eq!(cache.len(), 0);

        cache.set("file1.txt", make_cached_info("hash1"));
        assert_eq!(cache.len(), 1);

        cache.set("file2.txt", make_cached_info("hash2"));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn cache_snapshot() {
        let cache = InMemoryHashCache::new();
        cache.set("file.txt", make_cached_info("abc123"));

        let snapshot = cache.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot.get("file.txt").unwrap().hash, "abc123");
    }

    #[test]
    fn cache_load() {
        let cache = InMemoryHashCache::new();
        cache.set("old.txt", make_cached_info("old"));

        let mut new_entries = HashMap::new();
        new_entries.insert("new.txt".to_string(), make_cached_info("new"));

        cache.load(new_entries);

        assert!(cache.get("old.txt").is_none());
        assert!(cache.get("new.txt").is_some());
    }

    #[test]
    fn cache_with_entries() {
        let mut entries = HashMap::new();
        entries.insert("file.txt".to_string(), make_cached_info("hash"));

        let cache = InMemoryHashCache::with_entries(entries);
        assert_eq!(cache.len(), 1);
        assert!(cache.get("file.txt").is_some());
    }

    #[test]
    fn cache_default_is_empty() {
        let cache = InMemoryHashCache::default();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }
}
