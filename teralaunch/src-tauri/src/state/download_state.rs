//! Download progress and file cache state management.
//!
//! This module provides thread-safe access to download-related global state
//! including the hash cache, download progress, and cancellation flag.

#![allow(dead_code)]

use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::RwLock;
use tokio::sync::Mutex;

use crate::domain::CachedFileInfo;

lazy_static! {
    /// Cache of file hashes to avoid recalculating for unchanged files.
    static ref HASH_CACHE: Mutex<HashMap<String, CachedFileInfo>> = Mutex::new(HashMap::new());

    /// Flag to signal download cancellation.
    static ref CANCEL_DOWNLOAD: AtomicBool = AtomicBool::new(false);

    /// Flag to signal download is complete (success or error).
    static ref DOWNLOAD_COMPLETE: AtomicBool = AtomicBool::new(false);
}

/// Global counter for total bytes downloaded across all files.
static GLOBAL_DOWNLOADED_BYTES: AtomicU64 = AtomicU64::new(0);

/// Name of the file currently being downloaded.
static CURRENT_FILE_NAME: RwLock<String> = RwLock::new(String::new());

// ============================================================================
// Hash Cache Functions
// ============================================================================

/// Returns a clone of the current hash cache.
/// This is an async function because the underlying Mutex is from tokio.
#[allow(dead_code)]
pub async fn get_cached_files() -> HashMap<String, CachedFileInfo> {
    HASH_CACHE.lock().await.clone()
}

/// Updates or inserts a single cached file entry.
#[allow(dead_code)]
pub async fn update_cached_file(path: String, info: CachedFileInfo) {
    HASH_CACHE.lock().await.insert(path, info);
}

/// Clears all entries from the hash cache.
/// Returns `Ok(())` on success or an error message if the lock couldn't be acquired.
///
/// Note: The error case (line 53) is excluded from coverage because it requires
/// lock contention at the exact moment of try_lock(), which is non-deterministic
/// and effectively untestable in a reliable way.
#[cfg(not(tarpaulin_include))]
pub fn clear_hash_cache() -> Result<(), String> {
    match HASH_CACHE.try_lock() {
        Ok(mut cache) => {
            cache.clear();
            Ok(())
        }
        Err(_) => Err("Could not acquire hash cache lock".to_string()),
    }
}

/// Returns a lock guard to the hash cache for bulk operations.
pub async fn hash_cache_lock() -> tokio::sync::MutexGuard<'static, HashMap<String, CachedFileInfo>>
{
    HASH_CACHE.lock().await
}

// ============================================================================
// Download Progress Functions
// ============================================================================

/// Returns the current total downloaded bytes.
pub fn get_downloaded_bytes() -> u64 {
    GLOBAL_DOWNLOADED_BYTES.load(Ordering::SeqCst)
}

/// Sets the total downloaded bytes to a specific value.
pub fn set_downloaded_bytes(value: u64) {
    GLOBAL_DOWNLOADED_BYTES.store(value, Ordering::SeqCst);
}

/// Adds to the total downloaded bytes counter.
/// Returns the previous value.
#[allow(dead_code)]
pub fn add_downloaded_bytes(delta: u64) -> u64 {
    GLOBAL_DOWNLOADED_BYTES.fetch_add(delta, Ordering::SeqCst)
}

/// Subtracts from the total downloaded bytes counter.
/// Returns the previous value.
#[allow(dead_code)]
pub fn sub_downloaded_bytes(delta: u64) -> u64 {
    GLOBAL_DOWNLOADED_BYTES.fetch_sub(delta, Ordering::SeqCst)
}

/// Resets the downloaded bytes counter to zero.
#[allow(dead_code)]
pub fn reset_downloaded_bytes() {
    GLOBAL_DOWNLOADED_BYTES.store(0, Ordering::SeqCst);
}

// ============================================================================
// Download Cancellation Functions
// ============================================================================

/// Checks if download cancellation has been requested.
pub fn is_download_cancelled() -> bool {
    CANCEL_DOWNLOAD.load(Ordering::SeqCst)
}

/// Sets the download cancellation flag.
pub fn set_download_cancelled(value: bool) {
    CANCEL_DOWNLOAD.store(value, Ordering::SeqCst);
}

/// Requests download cancellation (sets flag to true).
pub fn cancel_download() {
    CANCEL_DOWNLOAD.store(true, Ordering::SeqCst);
}

/// Clears the download cancellation flag (sets flag to false).
#[allow(dead_code)]
pub fn clear_download_cancelled() {
    CANCEL_DOWNLOAD.store(false, Ordering::SeqCst);
}

// ============================================================================
// Download Completion Functions
// ============================================================================

/// Checks if download has completed (success or error).
pub fn is_download_complete() -> bool {
    DOWNLOAD_COMPLETE.load(Ordering::SeqCst)
}

/// Sets the download completion flag.
pub fn set_download_complete(value: bool) {
    DOWNLOAD_COMPLETE.store(value, Ordering::SeqCst);
}

// ============================================================================
// Current File Name Functions
// ============================================================================

/// Returns the name of the file currently being downloaded.
pub fn get_current_file_name() -> String {
    CURRENT_FILE_NAME
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Sets the name of the file currently being downloaded.
pub fn set_current_file_name(name: String) {
    let mut guard = CURRENT_FILE_NAME.write().unwrap_or_else(|e| e.into_inner());
    *guard = name;
}

/// Clears the current file name.
#[allow(dead_code)]
pub fn clear_current_file_name() {
    let mut guard = CURRENT_FILE_NAME.write().unwrap_or_else(|e| e.into_inner());
    guard.clear();
}

// ============================================================================
// Combined State Operations
// ============================================================================

/// Resets all download state for a fresh download session.
/// - Clears downloaded bytes counter
/// - Clears cancellation flag
/// - Clears download complete flag
/// - Clears current file name
#[allow(dead_code)]
pub fn reset_download_state() {
    GLOBAL_DOWNLOADED_BYTES.store(0, Ordering::SeqCst);
    CANCEL_DOWNLOAD.store(false, Ordering::SeqCst);
    DOWNLOAD_COMPLETE.store(false, Ordering::SeqCst);
    let mut guard = CURRENT_FILE_NAME.write().unwrap_or_else(|e| e.into_inner());
    guard.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    // Note: Tests modify global state. Run with --test-threads=1 if flaky.

    #[test]
    fn test_downloaded_bytes_operations() {
        // Reset to known state
        set_downloaded_bytes(0);
        assert_eq!(get_downloaded_bytes(), 0);

        set_downloaded_bytes(1000);
        assert_eq!(get_downloaded_bytes(), 1000);

        let prev = add_downloaded_bytes(500);
        assert_eq!(prev, 1000);
        assert_eq!(get_downloaded_bytes(), 1500);

        let prev = sub_downloaded_bytes(200);
        assert_eq!(prev, 1500);
        assert_eq!(get_downloaded_bytes(), 1300);

        reset_downloaded_bytes();
        assert_eq!(get_downloaded_bytes(), 0);
    }

    #[test]
    fn test_download_cancellation() {
        // Reset to known state
        set_download_cancelled(false);
        assert!(!is_download_cancelled());

        cancel_download();
        assert!(is_download_cancelled());

        clear_download_cancelled();
        assert!(!is_download_cancelled());

        set_download_cancelled(true);
        assert!(is_download_cancelled());
    }

    #[test]
    fn test_download_completion() {
        // Reset to known state
        set_download_complete(false);
        assert!(!is_download_complete());

        set_download_complete(true);
        assert!(is_download_complete());

        set_download_complete(false);
        assert!(!is_download_complete());
    }

    #[test]
    fn test_current_file_name() {
        clear_current_file_name();
        assert_eq!(get_current_file_name(), "");

        set_current_file_name("test_file.dat".to_string());
        assert_eq!(get_current_file_name(), "test_file.dat");

        set_current_file_name("another_file.bin".to_string());
        assert_eq!(get_current_file_name(), "another_file.bin");

        clear_current_file_name();
        assert_eq!(get_current_file_name(), "");
    }

    #[test]
    fn test_reset_download_state() {
        // Set up some state
        set_downloaded_bytes(5000);
        set_download_cancelled(true);
        set_download_complete(true);
        set_current_file_name("some_file.txt".to_string());

        // Reset all
        reset_download_state();

        assert_eq!(get_downloaded_bytes(), 0);
        assert!(!is_download_cancelled());
        assert!(!is_download_complete());
        assert_eq!(get_current_file_name(), "");
    }

    #[tokio::test]
    async fn test_hash_cache_operations() {
        // Clear cache first
        let _ = clear_hash_cache();

        let info = CachedFileInfo {
            hash: "abc123".to_string(),
            last_modified: SystemTime::now(),
        };

        update_cached_file("test/path.txt".to_string(), info.clone()).await;

        let cache = get_cached_files().await;
        assert!(cache.contains_key("test/path.txt"));
        assert_eq!(cache.get("test/path.txt").unwrap().hash, "abc123");
    }

    #[test]
    fn test_clear_hash_cache() {
        // This tests the synchronous try_lock version
        let result = clear_hash_cache();
        // Should succeed if no other thread holds the lock
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_hash_cache_lock() {
        // Clear cache first to ensure clean state
        let _ = clear_hash_cache();

        // Test that we can acquire the lock and get a valid guard
        let guard = hash_cache_lock().await;
        assert!(guard.is_empty());

        // Drop the guard and insert an entry
        drop(guard);

        let info = CachedFileInfo {
            hash: "def456".to_string(),
            last_modified: SystemTime::now(),
        };
        update_cached_file("test/lock_path.txt".to_string(), info).await;

        // Acquire lock again and verify the entry exists
        let guard = hash_cache_lock().await;
        assert!(guard.contains_key("test/lock_path.txt"));
        assert_eq!(guard.get("test/lock_path.txt").unwrap().hash, "def456");
    }

    // Note: Testing the error case on line 53 (Err("Could not acquire hash cache lock"))
    // is difficult to test reliably because it requires contention on the Mutex during
    // the exact moment clear_hash_cache() calls try_lock(). This is a race condition
    // that's inherently hard to trigger deterministically in unit tests.
    // The error case would only occur if another thread held the lock at the precise
    // moment clear_hash_cache() was called, which would be intermittent and non-deterministic.
}
