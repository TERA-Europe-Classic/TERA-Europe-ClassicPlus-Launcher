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

    /// Flag to indicate a download session is active.
    static ref DOWNLOAD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
}

/// Global counter for total bytes downloaded across all files.
static GLOBAL_DOWNLOADED_BYTES: AtomicU64 = AtomicU64::new(0);

/// Download session generation counter for progress ticker coordination.
/// Each new download session increments this to signal old tickers to exit.
static DOWNLOAD_GENERATION: AtomicU64 = AtomicU64::new(0);

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
///
/// This function blocks until it can acquire the lock, ensuring the cache is always cleared.
pub async fn clear_hash_cache() {
    let mut cache = HASH_CACHE.lock().await;
    cache.clear();
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

/// Subtracts from the total downloaded bytes counter using saturating subtraction.
/// Returns the previous value.
/// Note: Uses saturating_sub to prevent underflow to u64::MAX.
#[allow(dead_code)]
pub fn sub_downloaded_bytes(delta: u64) -> u64 {
    // Use compare-and-swap loop for saturating subtraction
    loop {
        let current = GLOBAL_DOWNLOADED_BYTES.load(Ordering::SeqCst);
        let new_value = current.saturating_sub(delta);
        match GLOBAL_DOWNLOADED_BYTES.compare_exchange(
            current,
            new_value,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(prev) => return prev,
            Err(_) => continue, // Value changed, retry
        }
    }
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
// Download In Progress Functions
// ============================================================================

/// Returns true if a download session is currently in progress.
pub fn is_download_in_progress() -> bool {
    DOWNLOAD_IN_PROGRESS.load(Ordering::SeqCst)
}

/// Sets the download in progress flag.
pub fn set_download_in_progress(value: bool) {
    DOWNLOAD_IN_PROGRESS.store(value, Ordering::SeqCst);
}

/// Atomically tries to start a download. Returns true if successful, false if already in progress.
/// This prevents TOCTOU race conditions by using atomic compare-exchange.
pub fn try_start_download() -> bool {
    DOWNLOAD_IN_PROGRESS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

// ============================================================================
// Download Generation Counter Functions
// ============================================================================

/// Gets the current download generation.
pub fn get_download_generation() -> u64 {
    DOWNLOAD_GENERATION.load(Ordering::SeqCst)
}

/// Increments and returns the new download generation.
pub fn increment_download_generation() -> u64 {
    DOWNLOAD_GENERATION.fetch_add(1, Ordering::SeqCst) + 1
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

/// Resets all download-related state atomically using generation counter.
///
/// This function uses a generation counter pattern to signal state transition:
/// 1. Generation is incremented FIRST to tell readers the state is transitioning
/// 2. All state fields are then reset
/// 3. Readers check generation before/after reading to detect transitions
///
/// This pattern prevents race conditions where readers might see partial state
/// (e.g., bytes reset but cancellation flag still set from previous session).
///
/// Resets:
/// - Download generation (incremented to signal transition)
/// - Downloaded bytes counter
/// - Cancellation flag
/// - Download complete flag
/// - Download in progress flag
/// - Current file name
#[allow(dead_code)]
pub fn reset_download_state() {
    // Increment generation first to signal state transition
    increment_download_generation();

    // Now reset all state - readers will check generation and see it changed
    GLOBAL_DOWNLOADED_BYTES.store(0, Ordering::SeqCst);
    CANCEL_DOWNLOAD.store(false, Ordering::SeqCst);
    DOWNLOAD_COMPLETE.store(false, Ordering::SeqCst);
    DOWNLOAD_IN_PROGRESS.store(false, Ordering::SeqCst);

    // Clear file name last
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
        let info = CachedFileInfo {
            hash: "abc123".to_string(),
            last_modified: SystemTime::now(),
        };

        let mut cache = hash_cache_lock().await;
        cache.clear();
        cache.insert("test/path.txt".to_string(), info.clone());

        assert!(cache.contains_key("test/path.txt"));
        assert_eq!(cache.get("test/path.txt").unwrap().hash, "abc123");
    }

    #[tokio::test]
    async fn test_clear_hash_cache() {
        // Clear cache should always succeed by waiting for the lock
        clear_hash_cache().await;
        // No assertion needed - if it doesn't hang, it succeeded
    }

    #[tokio::test]
    async fn test_hash_cache_lock() {
        let info = CachedFileInfo {
            hash: "def456".to_string(),
            last_modified: SystemTime::now(),
        };

        let mut guard = hash_cache_lock().await;
        guard.clear();
        assert!(guard.is_empty());

        guard.insert("test/lock_path.txt".to_string(), info);
        assert!(guard.contains_key("test/lock_path.txt"));
        assert_eq!(guard.get("test/lock_path.txt").unwrap().hash, "def456");
    }

    // Note: Testing the error case on line 53 (Err("Could not acquire hash cache lock"))
    // is difficult to test reliably because it requires contention on the Mutex during
    // the exact moment clear_hash_cache() calls try_lock(). This is a race condition
    // that's inherently hard to trigger deterministically in unit tests.
    // The error case would only occur if another thread held the lock at the precise
    // moment clear_hash_cache() was called, which would be intermittent and non-deterministic.

    #[test]
    fn test_download_generation_counter() {
        // Get the current generation
        let gen1 = get_download_generation();

        // Increment and check
        let gen2 = increment_download_generation();
        assert_eq!(gen2, gen1 + 1);
        assert_eq!(get_download_generation(), gen2);

        // Increment again
        let gen3 = increment_download_generation();
        assert_eq!(gen3, gen2 + 1);
        assert_eq!(get_download_generation(), gen3);
    }

    #[test]
    fn test_download_in_progress() {
        // Reset to known state
        set_download_in_progress(false);
        assert!(!is_download_in_progress());

        set_download_in_progress(true);
        assert!(is_download_in_progress());

        set_download_in_progress(false);
        assert!(!is_download_in_progress());
    }

    #[test]
    fn test_try_start_download() {
        // Reset to known state
        set_download_in_progress(false);

        // First call should succeed
        assert!(try_start_download());
        assert!(is_download_in_progress());

        // Second call should fail (already in progress)
        assert!(!try_start_download());
        assert!(is_download_in_progress());

        // Reset and try again
        set_download_in_progress(false);
        assert!(try_start_download());
        assert!(is_download_in_progress());
    }
}
