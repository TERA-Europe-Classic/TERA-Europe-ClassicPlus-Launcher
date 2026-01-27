//! Hash verification and file checking Tauri commands
//!
//! This module contains commands for:
//! - Checking which files need updates
//! - Generating hash files
//! - Clearing the hash cache
//!
//! # Testability
//!
//! The module provides `*_with_fs` variants of hash functions that accept a
//! `FileSystem` trait implementation, enabling unit testing with `MockFileSystem`.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::fs::{self, remove_file, File};
use std::io::{BufReader, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

use indicatif::{ProgressBar, ProgressStyle};
use log::{error, info};
use rayon::iter::{ParallelBridge, ParallelIterator};
use rayon::prelude::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use walkdir::WalkDir;

use crate::commands::config::{get_cache_file_path, get_game_path};
use crate::domain::{
    CachedFileInfo, FileCheckProgress, FileInfo, BUFFER_SIZE, CONNECT_TIMEOUT_SECS,
    DOWNLOAD_TIMEOUT_SECS, HTTP_POOL_MAX_IDLE_PER_HOST,
};
use crate::infrastructure::{EventEmitter, FileSystem};
use crate::services::hash_service;
use crate::state::clear_hash_cache;
use crate::utils::{is_ignored, resume_offset, validate_path_within_base};
use teralib::config::get_config_value;

/// Clears the hash cache to force recalculation.
///
/// This is used for the "repair client" functionality - it deletes
/// the cache file and clears the in-memory cache.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub fn clear_cache() -> Result<(), String> {
    // Clear the in-memory hash cache to prevent stale entries from old directory
    let _ = clear_hash_cache(); // Ignore error if lock is held
                                // Remove the disk cache file
    let cache_path = get_cache_file_path()?;
    if cache_path.exists() {
        remove_file(cache_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Checks if any files need to be updated.
///
/// This is a quick check that returns true if any files differ from the server.
///
/// # Arguments
/// * `window` - The Tauri window for emitting progress events
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn check_update_required(window: tauri::Window) -> Result<bool, String> {
    match get_files_to_update(window).await {
        Ok(files) => Ok(!files.is_empty()),
        Err(e) => Err(e),
    }
}

/// Gets the list of files that need to be updated.
///
/// Compares local files against the server hash file and returns
/// a list of files that are missing, corrupted, or outdated.
///
/// # Arguments
/// * `window` - The Tauri window for emitting progress events
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn get_files_to_update(window: tauri::Window) -> Result<Vec<FileInfo>, String> {
    info!("Starting get_files_to_update");

    let start_time = Instant::now();
    let server_hash_file = get_server_hash_file().await?;

    // Get the path to the game folder
    let local_game_path = get_game_path()?;
    info!("Local game path: {:?}", local_game_path);

    info!("Attempting to read server hash file");
    let files = server_hash_file["files"]
        .as_array()
        .ok_or("Invalid server hash file format")?;
    info!("Server hash file parsed, {} files found", files.len());

    info!("Starting file comparison");
    let loaded_cache = load_cache_from_disk()
        .await
        .unwrap_or_else(|_| HashMap::new());
    let cache = Arc::new(RwLock::new(loaded_cache));

    let progress_bar = ProgressBar::new(files.len() as u64);
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .expect("Invalid progress bar template - this is a bug")
            .progress_chars("##-"),
    );

    let processed_count = Arc::new(AtomicUsize::new(0));
    let files_to_update_count = Arc::new(AtomicUsize::new(0));
    let total_size = Arc::new(AtomicU64::new(0));

    let files_to_update: Vec<FileInfo> = files
        .par_iter()
        .enumerate()
        .filter_map(|(_index, file_info)| {
            let path = file_info["path"].as_str().unwrap_or("");
            let server_hash = file_info["hash"].as_str().unwrap_or("");
            let size = file_info["size"].as_u64().unwrap_or(0);
            let url = file_info["url"].as_str().unwrap_or("").to_string();

            let local_file_path = local_game_path.join(path);

            // Validate path to prevent path traversal attacks
            let local_file_path =
                match validate_path_within_base(&local_game_path, &local_file_path) {
                    Ok(p) => p,
                    Err(e) => {
                        error!("Path validation failed for {}: {}", path, e);
                        return None; // Skip this file
                    }
                };

            let current_count = processed_count.fetch_add(1, Ordering::SeqCst) + 1;
            if current_count % 100 == 0 || current_count == files.len() {
                let progress_payload = FileCheckProgress {
                    current_file: path.to_string(),
                    progress: (current_count as f64 / files.len() as f64) * 100.0,
                    current_count,
                    total_files: files.len(),
                    elapsed_time: start_time.elapsed().as_secs_f64(),
                    files_to_update: files_to_update_count.load(Ordering::SeqCst),
                };

                let _ = window
                    .emit("file_check_progress", progress_payload)
                    .map_err(|e| {
                        error!("Error emitting file_check_progress event: {}", e);
                        e.to_string()
                    });
            }

            progress_bar.inc(1);

            if !local_file_path.exists() {
                files_to_update_count.fetch_add(1, Ordering::SeqCst);
                total_size.fetch_add(size, Ordering::SeqCst);
                return Some(FileInfo {
                    path: path.to_string(),
                    hash: server_hash.to_string(),
                    size,
                    url,
                    existing_size: 0,
                });
            }

            let metadata = match fs::metadata(&local_file_path) {
                Ok(m) => m,
                Err(_) => {
                    files_to_update_count.fetch_add(1, Ordering::SeqCst);
                    total_size.fetch_add(size, Ordering::SeqCst);
                    return Some(FileInfo {
                        path: path.to_string(),
                        hash: server_hash.to_string(),
                        size,
                        url,
                        existing_size: 0,
                    });
                }
            };

            let last_modified = metadata.modified().ok();

            let cache_read = cache.read().unwrap_or_else(|e| e.into_inner());
            if let Some(cached_info) = cache_read.get(path) {
                if let Some(lm) = last_modified {
                    if cached_info.last_modified == lm && cached_info.hash == server_hash {
                        return None;
                    }
                }
            }
            drop(cache_read);

            if metadata.len() != size {
                files_to_update_count.fetch_add(1, Ordering::SeqCst);
                total_size.fetch_add(size, Ordering::SeqCst);
                return Some(FileInfo {
                    path: path.to_string(),
                    hash: server_hash.to_string(),
                    size,
                    url,
                    existing_size: resume_offset(metadata.len(), size),
                });
            }

            let local_hash = match calculate_file_hash(&local_file_path) {
                Ok(hash) => hash,
                Err(_) => {
                    files_to_update_count.fetch_add(1, Ordering::SeqCst);
                    total_size.fetch_add(size, Ordering::SeqCst);
                    return Some(FileInfo {
                        path: path.to_string(),
                        hash: server_hash.to_string(),
                        size,
                        url,
                        existing_size: resume_offset(metadata.len(), size),
                    });
                }
            };

            let mut cache_write = cache.write().unwrap_or_else(|e| e.into_inner());
            cache_write.insert(
                path.to_string(),
                CachedFileInfo {
                    hash: local_hash.clone(),
                    last_modified: last_modified.unwrap_or_else(SystemTime::now),
                },
            );
            drop(cache_write);

            if local_hash != server_hash {
                files_to_update_count.fetch_add(1, Ordering::SeqCst);
                total_size.fetch_add(size, Ordering::SeqCst);
                Some(FileInfo {
                    path: path.to_string(),
                    hash: server_hash.to_string(),
                    size,
                    url,
                    existing_size: resume_offset(metadata.len(), size),
                })
            } else {
                None
            }
        })
        .collect();

    progress_bar.finish_with_message("File comparison completed");

    // Ensure the UI receives a final 100% progress update
    let final_progress = FileCheckProgress {
        current_file: String::new(),
        progress: 100.0,
        current_count: files.len(),
        total_files: files.len(),
        elapsed_time: start_time.elapsed().as_secs_f64(),
        files_to_update: files_to_update_count.load(Ordering::SeqCst),
    };

    let _ = window
        .emit("file_check_progress", final_progress)
        .map_err(|e| {
            error!("Error emitting final file_check_progress event: {}", e);
            e.to_string()
        });

    // Save the updated cache to disk
    let final_cache = cache.read().unwrap_or_else(|e| e.into_inner()).clone();
    if let Err(e) = save_cache_to_disk(&final_cache).await {
        error!("Failed to save cache to disk: {}", e);
    }

    let total_time = start_time.elapsed();
    info!(
        "File comparison completed. Files to update: {}",
        files_to_update.len()
    );

    // Emit a final event with complete statistics
    let _ = window.emit(
        "file_check_completed",
        json!({
            "total_files": files.len(),
            "files_to_update": files_to_update.len(),
            "total_size": total_size.load(Ordering::SeqCst),
            "total_time_seconds": total_time.as_secs(),
            "average_time_per_file_ms": (total_time.as_millis() as f64) / (files.len() as f64)
        }),
    );

    Ok(files_to_update)
}

/// Generates a hash file for all game files.
///
/// This is used for server-side hash file generation and debugging.
///
/// # Arguments
/// * `window` - The Tauri window for emitting progress events
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn generate_hash_file(window: tauri::Window) -> Result<String, String> {
    let start_time = Instant::now();

    let game_path = get_game_path()?;
    info!("Game path: {:?}", game_path);
    let output_path = game_path.join("hash-file.json");
    info!("Output path: {:?}", output_path);

    // List of files and directories to ignore
    let ignored_paths: HashSet<&str> = [
        "$Patch",
        "Binaries/cookies.dat",
        "S1Game/GuildFlagUpload",
        "S1Game/GuildLogoUpload",
        "S1Game/ImageCache",
        "S1Game/Logs",
        "S1Game/Screenshots",
        "S1Game/Config/S1Engine.ini",
        "S1Game/Config/S1Game.ini",
        "S1Game/Config/S1Input.ini",
        "S1Game/Config/S1Lightmass.ini",
        "S1Game/Config/S1Option.ini",
        "S1Game/Config/S1SystemSettings.ini",
        "S1Game/Config/S1TBASettings.ini",
        "S1Game/Config/S1UI.ini",
        "Launcher.exe",
        "local.db",
        "version.ini",
        "unins000.dat",
        "unins000.exe",
    ]
    .iter()
    .cloned()
    .collect();

    let total_files = WalkDir::new(&game_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| !is_ignored(e.path(), &game_path, &ignored_paths))
        .count();
    info!("Total files to process: {}", total_files);

    let progress_bar = ProgressBar::new(total_files as u64);
    let progress_style = ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
        .map_err(|e| e.to_string())?
        .progress_chars("##-");
    progress_bar.set_style(progress_style);

    let processed_files = AtomicU64::new(0);
    let total_size = AtomicU64::new(0);
    let files = Arc::new(Mutex::new(Vec::new()));

    let result: Result<(), String> = WalkDir::new(&game_path)
        .into_iter()
        .par_bridge()
        .try_for_each(|entry| -> Result<(), String> {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_file() && !is_ignored(path, &game_path, &ignored_paths) {
                let relative_path = match path.strip_prefix(&game_path) {
                    Ok(p) => match p.to_str() {
                        Some(s) => s.replace("\\", "/"),
                        None => return Ok(()), // Non-UTF8 path, skip
                    },
                    Err(_) => return Ok(()), // Path not under game_path, skip
                };
                info!("Processing file: {}", relative_path);

                let file = File::open(path).map_err(|e| e.to_string())?;
                let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);
                let mut hasher = Sha256::new();
                let mut buffer = [0u8; BUFFER_SIZE];
                let mut size: u64 = 0;

                loop {
                    let bytes_read = reader.read(&mut buffer).map_err(|e| e.to_string())?;
                    if bytes_read == 0 {
                        break;
                    }
                    hasher.update(&buffer[..bytes_read]);
                    size += bytes_read as u64;
                }
                let hash = format!("{:x}", hasher.finalize());
                let file_server_url = get_config_value("FILE_SERVER_URL");
                let url = format!("{}/{}", file_server_url, relative_path);

                files.blocking_lock().push(FileInfo {
                    path: relative_path.clone(),
                    hash,
                    size,
                    url,
                    existing_size: 0,
                });

                total_size.fetch_add(size, Ordering::Relaxed);
                let current_processed = processed_files.fetch_add(1, Ordering::Relaxed) + 1;
                progress_bar.set_position(current_processed);

                let progress = (current_processed as f64 / total_files as f64) * 100.0;
                window
                    .emit(
                        "hash_file_progress",
                        json!({
                            "current_file": relative_path,
                            "progress": progress,
                            "processed_files": current_processed,
                            "total_files": total_files,
                            "total_size": total_size.load(Ordering::Relaxed)
                        }),
                    )
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        });

    if let Err(e) = result {
        error!("Error during file processing: {:?}", e);
        return Err(e);
    }

    progress_bar.finish_with_message("File processing completed");

    info!("Generating JSON");
    let json = serde_json::to_string(&json!({
        "files": files.lock().await.clone()
    }))
    .map_err(|e| e.to_string())?;

    info!("Writing hash file");
    let mut file = File::create(&output_path).map_err(|e| e.to_string())?;
    file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;

    let duration = start_time.elapsed();
    let total_processed = processed_files.load(Ordering::Relaxed);
    let total_size = total_size.load(Ordering::Relaxed);
    info!("Hash file generation completed in {:?}", duration);
    info!("Total files processed: {}", total_processed);
    info!("Total size: {} bytes", total_size);

    Ok(format!(
        "Hash file generated successfully. Processed {} files with a total size of {} bytes in {:?}",
        total_processed, total_size, duration
    ))
}

// ============================================================================
// Internal helper functions
// ============================================================================

/// Fetches the hash file from the server.
#[cfg(not(tarpaulin_include))]
async fn get_server_hash_file() -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .pool_max_idle_per_host(HTTP_POOL_MAX_IDLE_PER_HOST)
        .build()
        .map_err(|e| e.to_string())?;

    let res = client
        .get(get_config_value("HASH_FILE_URL"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(json)
}

/// Calculates the SHA-256 hash of a file using std::fs.
///
/// This is the production wrapper that uses the real filesystem.
#[cfg(not(tarpaulin_include))]
fn calculate_file_hash<P: AsRef<std::path::Path>>(path: P) -> Result<String, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let reader = BufReader::with_capacity(BUFFER_SIZE, file);
    hash_service::calculate_hash_from_reader(reader)
}

/// Calculates the SHA-256 hash of a file using a FileSystem implementation.
///
/// This is the testable inner function that accepts a FileSystem trait object,
/// allowing tests to use MockFileSystem instead of touching the real filesystem.
///
/// # Arguments
/// * `fs` - A FileSystem implementation (StdFileSystem or MockFileSystem)
/// * `path` - Path to the file to hash
///
/// # Returns
/// * `Ok(String)` - The hex-encoded SHA-256 hash
/// * `Err(String)` - Error message if file cannot be read
///
/// # Example
/// ```ignore
/// use crate::infrastructure::{FileSystem, MockFileSystem};
///
/// let mut mock = MockFileSystem::new();
/// mock.add_file("/test.pak", b"test content");
/// let result = calculate_file_hash_with_fs(&mock, Path::new("/test.pak"));
/// assert!(result.is_ok());
/// ```
pub fn calculate_file_hash_with_fs<F: FileSystem>(fs: &F, path: &Path) -> Result<String, String> {
    let content = fs.read_file(path)?;
    let reader = std::io::Cursor::new(content);
    hash_service::calculate_hash_from_reader(reader)
}

/// Checks if a single file needs updating using a FileSystem implementation.
///
/// This is the testable inner function for file comparison logic.
///
/// # Arguments
/// * `fs` - A FileSystem implementation
/// * `path` - Path to the local file
/// * `expected_hash` - The expected SHA-256 hash from the server
/// * `expected_size` - The expected file size in bytes
///
/// # Returns
/// * `Ok(FileNeedsUpdate)` - Whether the file needs updating and why
pub fn check_file_needs_update_with_fs<F: FileSystem>(
    fs: &F,
    path: &Path,
    expected_hash: &str,
    expected_size: u64,
) -> FileUpdateStatus {
    // Check if file exists
    if !fs.exists(path) {
        return FileUpdateStatus::Missing;
    }

    // Check metadata (size)
    // Note: MockFileSystem returns Ok for metadata if file exists, so MetadataError
    // branch is only reachable in production when filesystem errors occur after
    // exists() returns true (e.g., permission issues, race conditions).
    let metadata = match fs.metadata(path) {
        Ok(m) => m,
        Err(_) => return FileUpdateStatus::MetadataError,
    };

    // Size mismatch means file needs update
    if metadata.size != expected_size {
        return FileUpdateStatus::SizeMismatch {
            expected: expected_size,
            actual: metadata.size,
        };
    }

    // Calculate hash and compare
    match calculate_file_hash_with_fs(fs, path) {
        Ok(actual_hash) => {
            if hash_service::hashes_match(&actual_hash, expected_hash) {
                FileUpdateStatus::UpToDate
            } else {
                FileUpdateStatus::HashMismatch {
                    expected: expected_hash.to_string(),
                    actual: actual_hash,
                }
            }
        }
        Err(e) => FileUpdateStatus::HashError(e),
    }
}

/// Result of checking whether a file needs to be updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileUpdateStatus {
    /// File is up to date (hash matches)
    UpToDate,
    /// File does not exist
    Missing,
    /// File size does not match expected
    SizeMismatch { expected: u64, actual: u64 },
    /// File hash does not match expected
    HashMismatch { expected: String, actual: String },
    /// Could not read file metadata
    MetadataError,
    /// Could not calculate file hash
    HashError(String),
}

impl FileUpdateStatus {
    /// Returns true if the file needs to be updated/downloaded.
    pub fn needs_update(&self) -> bool {
        !matches!(self, FileUpdateStatus::UpToDate)
    }
}

// ============================================================================
// Testable progress emission functions with EventEmitter trait
// ============================================================================

/// Parameters for file check progress.
#[derive(Debug, Clone)]
pub struct FileCheckProgressParams {
    /// Current file being checked.
    pub current_file: String,
    /// Current file count (1-based).
    pub current_count: usize,
    /// Total number of files to check.
    pub total_files: usize,
    /// Time elapsed since check started.
    pub elapsed_time: Duration,
    /// Number of files that need updating so far.
    pub files_to_update: usize,
}

/// Emits a file check progress event.
///
/// This is the testable inner function that can use any `EventEmitter`.
///
/// # Arguments
/// * `emitter` - The event emitter implementation
/// * `params` - Progress parameters
///
/// # Returns
/// `Ok(())` on success, `Err` on emission failure.
pub fn emit_file_check_progress<E: EventEmitter>(
    emitter: &E,
    params: &FileCheckProgressParams,
) -> Result<(), String> {
    let progress = if params.total_files > 0 {
        (params.current_count as f64 / params.total_files as f64) * 100.0
    } else {
        0.0
    };

    let payload = FileCheckProgress {
        current_file: params.current_file.clone(),
        progress,
        current_count: params.current_count,
        total_files: params.total_files,
        elapsed_time: params.elapsed_time.as_secs_f64(),
        files_to_update: params.files_to_update,
    };

    emitter.emit("file_check_progress", payload)
}

/// Parameters for file check completion.
#[derive(Debug, Clone)]
pub struct FileCheckCompletedParams {
    /// Total number of files checked.
    pub total_files: usize,
    /// Number of files that need updating.
    pub files_to_update: usize,
    /// Total size of files to update in bytes.
    pub total_size: u64,
    /// Total time taken in seconds.
    pub total_time_seconds: u64,
    /// Average time per file in milliseconds.
    pub average_time_per_file_ms: f64,
}

/// Emits a file check completed event.
///
/// # Arguments
/// * `emitter` - The event emitter implementation
/// * `params` - Completion parameters
pub fn emit_file_check_completed<E: EventEmitter>(
    emitter: &E,
    params: &FileCheckCompletedParams,
) -> Result<(), String> {
    emitter.emit(
        "file_check_completed",
        json!({
            "total_files": params.total_files,
            "files_to_update": params.files_to_update,
            "total_size": params.total_size,
            "total_time_seconds": params.total_time_seconds,
            "average_time_per_file_ms": params.average_time_per_file_ms
        }),
    )
}

/// Parameters for hash file generation progress.
#[derive(Debug, Clone)]
pub struct HashFileProgressParams {
    /// Current file being processed.
    pub current_file: String,
    /// Number of files processed so far.
    pub processed_files: u64,
    /// Total number of files to process.
    pub total_files: usize,
    /// Total size of processed files in bytes.
    pub total_size: u64,
}

/// Emits a hash file generation progress event.
///
/// # Arguments
/// * `emitter` - The event emitter implementation
/// * `params` - Progress parameters
pub fn emit_hash_file_progress<E: EventEmitter>(
    emitter: &E,
    params: &HashFileProgressParams,
) -> Result<(), String> {
    let progress = if params.total_files > 0 {
        (params.processed_files as f64 / params.total_files as f64) * 100.0
    } else {
        0.0
    };

    emitter.emit(
        "hash_file_progress",
        json!({
            "current_file": params.current_file,
            "progress": progress,
            "processed_files": params.processed_files,
            "total_files": params.total_files,
            "total_size": params.total_size
        }),
    )
}

/// Saves the file cache to disk.
#[cfg(not(tarpaulin_include))]
async fn save_cache_to_disk(cache: &HashMap<String, CachedFileInfo>) -> Result<(), String> {
    let cache_path = get_cache_file_path()?;
    let serialized = serde_json::to_string(cache).map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(move || std::fs::write(&cache_path, serialized))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Loads the file cache from disk.
#[cfg(not(tarpaulin_include))]
async fn load_cache_from_disk() -> Result<HashMap<String, CachedFileInfo>, String> {
    let cache_path = get_cache_file_path()?;
    let contents = tokio::task::spawn_blocking(move || std::fs::read_to_string(&cache_path))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let cache: HashMap<String, CachedFileInfo> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    Ok(cache)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::{MockEventEmitter, MockFileSystem};
    use crate::services::hash_service::calculate_hash_from_bytes;

    // ========================================================================
    // Tests for calculate_file_hash (real filesystem - negative tests only)
    // ========================================================================

    #[test]
    fn test_calculate_file_hash_nonexistent() {
        let result = calculate_file_hash("/nonexistent/file.txt");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to open file"));
    }

    // ========================================================================
    // Tests for calculate_file_hash_with_fs (using MockFileSystem)
    // ========================================================================

    #[test]
    fn test_calculate_hash_with_fs_existing_file() {
        let content = b"test content for hashing";
        let mock = MockFileSystem::new().with_file("/test.pak", content);

        let result = calculate_file_hash_with_fs(&mock, Path::new("/test.pak"));

        assert!(result.is_ok());
        let hash = result.unwrap();
        // Verify the hash matches what we'd expect
        let expected_hash = calculate_hash_from_bytes(content);
        assert_eq!(hash, expected_hash);
    }

    #[test]
    fn test_calculate_hash_with_fs_missing_file() {
        let mock = MockFileSystem::new();

        let result = calculate_file_hash_with_fs(&mock, Path::new("/nonexistent.pak"));

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_calculate_hash_with_fs_empty_file() {
        let mock = MockFileSystem::new().with_file("/empty.pak", b"");

        let result = calculate_file_hash_with_fs(&mock, Path::new("/empty.pak"));

        assert!(result.is_ok());
        // SHA-256 of empty string
        let expected_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(result.unwrap(), expected_hash);
    }

    #[test]
    fn test_calculate_hash_with_fs_binary_content() {
        // Test with binary content (non-UTF8)
        let binary_content: Vec<u8> = (0u8..=255).collect();
        let mock = MockFileSystem::new().with_file("/binary.pak", &binary_content);

        let result = calculate_file_hash_with_fs(&mock, Path::new("/binary.pak"));

        assert!(result.is_ok());
        let hash = result.unwrap();
        assert_eq!(hash.len(), 64); // SHA-256 produces 64 hex chars
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_calculate_hash_with_fs_large_content() {
        // Test with content larger than typical buffer size
        let large_content = vec![0xABu8; 1024 * 1024]; // 1MB of 0xAB bytes
        let mock = MockFileSystem::new().with_file("/large.pak", &large_content);

        let result = calculate_file_hash_with_fs(&mock, Path::new("/large.pak"));

        assert!(result.is_ok());
        let expected_hash = calculate_hash_from_bytes(&large_content);
        assert_eq!(result.unwrap(), expected_hash);
    }

    // ========================================================================
    // Tests for check_file_needs_update_with_fs
    // ========================================================================

    #[test]
    fn test_check_file_needs_update_missing_file() {
        let mock = MockFileSystem::new();

        let status = check_file_needs_update_with_fs(
            &mock,
            Path::new("/game/Data/file.pak"),
            "somehash",
            1000,
        );

        assert_eq!(status, FileUpdateStatus::Missing);
        assert!(status.needs_update());
    }

    #[test]
    fn test_check_file_needs_update_up_to_date() {
        let content = b"correct file content";
        let expected_hash = calculate_hash_from_bytes(content);
        let mock = MockFileSystem::new().with_file("/game/Data/file.pak", content);

        let status = check_file_needs_update_with_fs(
            &mock,
            Path::new("/game/Data/file.pak"),
            &expected_hash,
            content.len() as u64,
        );

        assert_eq!(status, FileUpdateStatus::UpToDate);
        assert!(!status.needs_update());
    }

    #[test]
    fn test_check_file_needs_update_size_mismatch() {
        let content = b"file content";
        let mock = MockFileSystem::new().with_file("/game/Data/file.pak", content);

        // Pass a different expected size
        let status = check_file_needs_update_with_fs(
            &mock,
            Path::new("/game/Data/file.pak"),
            "somehash",
            9999, // Wrong size
        );

        match status {
            FileUpdateStatus::SizeMismatch { expected, actual } => {
                assert_eq!(expected, 9999);
                assert_eq!(actual, content.len() as u64);
            }
            _ => panic!("Expected SizeMismatch, got {:?}", status),
        }
        assert!(status.needs_update());
    }

    #[test]
    fn test_check_file_needs_update_hash_mismatch() {
        let content = b"file content";
        let actual_hash = calculate_hash_from_bytes(content);
        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let mock = MockFileSystem::new().with_file("/game/Data/file.pak", content);

        let status = check_file_needs_update_with_fs(
            &mock,
            Path::new("/game/Data/file.pak"),
            wrong_hash,
            content.len() as u64, // Correct size
        );

        match &status {
            FileUpdateStatus::HashMismatch { expected, actual } => {
                assert_eq!(expected, &wrong_hash);
                assert_eq!(actual, &actual_hash);
            }
            _ => panic!("Expected HashMismatch, got {:?}", status),
        }
        assert!(status.needs_update());
    }

    #[test]
    fn test_check_file_needs_update_hash_case_insensitive() {
        let content = b"file content";
        let expected_hash = calculate_hash_from_bytes(content).to_uppercase();
        let mock = MockFileSystem::new().with_file("/game/Data/file.pak", content);

        let status = check_file_needs_update_with_fs(
            &mock,
            Path::new("/game/Data/file.pak"),
            &expected_hash, // Uppercase hash
            content.len() as u64,
        );

        assert_eq!(status, FileUpdateStatus::UpToDate);
    }

    #[test]
    fn test_check_file_needs_update_metadata_error() {
        // Test line 535: FileUpdateStatus::MetadataError when metadata() fails
        let fs = MockFileSystem::new()
            .with_file("/game/test.pak", b"content")
            .with_metadata_error("/game/test.pak"); // File exists but metadata fails

        let result = check_file_needs_update_with_fs(
            &fs,
            Path::new("/game/test.pak"),
            "expected_hash",
            1000,
        );

        assert!(matches!(result, FileUpdateStatus::MetadataError));
    }

    // ========================================================================
    // Tests for FileUpdateStatus
    // ========================================================================

    #[test]
    fn test_file_update_status_needs_update() {
        assert!(!FileUpdateStatus::UpToDate.needs_update());
        assert!(FileUpdateStatus::Missing.needs_update());
        assert!(FileUpdateStatus::SizeMismatch {
            expected: 100,
            actual: 50
        }
        .needs_update());
        assert!(FileUpdateStatus::HashMismatch {
            expected: "a".to_string(),
            actual: "b".to_string()
        }
        .needs_update());
        assert!(FileUpdateStatus::MetadataError.needs_update());
        assert!(FileUpdateStatus::HashError("error".to_string()).needs_update());
    }

    // ========================================================================
    // Integration-style tests with multiple files
    // ========================================================================

    #[test]
    fn test_multiple_files_mixed_status() {
        let correct_content = b"correct content";
        let correct_hash = calculate_hash_from_bytes(correct_content);
        let wrong_content = b"wrong content";

        let mock = MockFileSystem::new()
            .with_file("/game/Data/correct.pak", correct_content)
            .with_file("/game/Data/wrong_hash.pak", wrong_content)
            .with_file("/game/Data/wrong_size.pak", b"short");

        // File with correct hash
        let status1 = check_file_needs_update_with_fs(
            &mock,
            Path::new("/game/Data/correct.pak"),
            &correct_hash,
            correct_content.len() as u64,
        );
        assert_eq!(status1, FileUpdateStatus::UpToDate);

        // File with wrong hash (but correct size)
        let status2 = check_file_needs_update_with_fs(
            &mock,
            Path::new("/game/Data/wrong_hash.pak"),
            &correct_hash, // Wrong hash for this content
            wrong_content.len() as u64,
        );
        assert!(matches!(status2, FileUpdateStatus::HashMismatch { .. }));

        // Missing file
        let status3 = check_file_needs_update_with_fs(
            &mock,
            Path::new("/game/Data/missing.pak"),
            "somehash",
            1000,
        );
        assert_eq!(status3, FileUpdateStatus::Missing);

        // File with wrong size (size check happens before hash)
        let status4 = check_file_needs_update_with_fs(
            &mock,
            Path::new("/game/Data/wrong_size.pak"),
            "somehash",
            1000, // Much larger than actual
        );
        assert!(matches!(status4, FileUpdateStatus::SizeMismatch { .. }));
    }

    #[test]
    fn test_directory_handling() {
        let mock = MockFileSystem::new().with_dir("/game/Data");

        // Directories should not be treated as files
        // MockFileSystem.exists() returns true for directories
        // but metadata will return is_file: false
        let status = check_file_needs_update_with_fs(&mock, Path::new("/game/Data"), "somehash", 0);

        // Directory metadata returns size 0, so size matches
        // But reading a directory as a file should fail
        assert!(matches!(
            status,
            FileUpdateStatus::HashError(_) | FileUpdateStatus::UpToDate
        ));
    }

    // ========================================================================
    // EventEmitter tests for file check progress
    // ========================================================================

    #[test]
    fn test_emit_file_check_progress_emits_correct_event() {
        let emitter = MockEventEmitter::new();
        let params = FileCheckProgressParams {
            current_file: "Data/test.pak".to_string(),
            current_count: 50,
            total_files: 100,
            elapsed_time: Duration::from_secs(5),
            files_to_update: 10,
        };

        let result = emit_file_check_progress(&emitter, &params);
        assert!(result.is_ok());

        let events = emitter.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "file_check_progress");
    }

    #[test]
    fn test_emit_file_check_progress_payload_contains_expected_fields() {
        let emitter = MockEventEmitter::new();
        let params = FileCheckProgressParams {
            current_file: "Data/game.pak".to_string(),
            current_count: 75,
            total_files: 100,
            elapsed_time: Duration::from_secs(10),
            files_to_update: 5,
        };

        emit_file_check_progress(&emitter, &params).unwrap();

        let events = emitter.events();
        let payload = &events[0].payload;

        // Verify payload contains expected fields
        assert!(payload.contains("\"current_file\":\"Data/game.pak\""));
        assert!(payload.contains("\"current_count\":75"));
        assert!(payload.contains("\"total_files\":100"));
        assert!(payload.contains("\"files_to_update\":5"));
        // Progress should be 75%
        assert!(payload.contains("75"));
    }

    #[test]
    fn test_emit_file_check_progress_handles_zero_total() {
        let emitter = MockEventEmitter::new();
        let params = FileCheckProgressParams {
            current_file: String::new(),
            current_count: 0,
            total_files: 0,
            elapsed_time: Duration::from_secs(0),
            files_to_update: 0,
        };

        let result = emit_file_check_progress(&emitter, &params);
        assert!(result.is_ok());

        let events = emitter.events();
        let payload = &events[0].payload;
        // Progress should be 0 when total is 0
        assert!(payload.contains("\"progress\":0"));
    }

    #[test]
    fn test_emit_file_check_completed() {
        let emitter = MockEventEmitter::new();
        let params = FileCheckCompletedParams {
            total_files: 1000,
            files_to_update: 50,
            total_size: 1_000_000,
            total_time_seconds: 30,
            average_time_per_file_ms: 30.0,
        };

        let result = emit_file_check_completed(&emitter, &params);
        assert!(result.is_ok());

        let events = emitter.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "file_check_completed");

        let payload = &events[0].payload;
        assert!(payload.contains("\"total_files\":1000"));
        assert!(payload.contains("\"files_to_update\":50"));
        assert!(payload.contains("\"total_size\":1000000"));
        assert!(payload.contains("\"total_time_seconds\":30"));
    }

    #[test]
    fn test_emit_hash_file_progress() {
        let emitter = MockEventEmitter::new();
        let params = HashFileProgressParams {
            current_file: "S1Game/CookedPC/file.gpk".to_string(),
            processed_files: 500,
            total_files: 1000,
            total_size: 5_000_000_000,
        };

        let result = emit_hash_file_progress(&emitter, &params);
        assert!(result.is_ok());

        let events = emitter.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "hash_file_progress");

        let payload = &events[0].payload;
        assert!(payload.contains("S1Game/CookedPC/file.gpk"));
        assert!(payload.contains("\"processed_files\":500"));
        assert!(payload.contains("\"total_files\":1000"));
        // Progress should be 50%
        assert!(payload.contains("50"));
    }

    #[test]
    fn test_emit_hash_file_progress_handles_zero_total() {
        let emitter = MockEventEmitter::new();
        let params = HashFileProgressParams {
            current_file: String::new(),
            processed_files: 0,
            total_files: 0,
            total_size: 0,
        };

        let result = emit_hash_file_progress(&emitter, &params);
        assert!(result.is_ok());

        let events = emitter.events();
        let payload = &events[0].payload;
        assert!(payload.contains("\"progress\":0"));
    }

    #[test]
    fn test_emit_with_failing_emitter_file_check() {
        let emitter = MockEventEmitter::failing();
        let params = FileCheckProgressParams {
            current_file: "test.pak".to_string(),
            current_count: 1,
            total_files: 10,
            elapsed_time: Duration::from_secs(1),
            files_to_update: 0,
        };

        let result = emit_file_check_progress(&emitter, &params);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Mock emit failure");
    }

    #[test]
    fn test_emit_with_failing_emitter_completed() {
        let emitter = MockEventEmitter::failing();
        let params = FileCheckCompletedParams {
            total_files: 100,
            files_to_update: 5,
            total_size: 10000,
            total_time_seconds: 10,
            average_time_per_file_ms: 100.0,
        };

        let result = emit_file_check_completed(&emitter, &params);
        assert!(result.is_err());
    }

    #[test]
    fn test_emit_with_failing_emitter_hash_progress() {
        let emitter = MockEventEmitter::failing();
        let params = HashFileProgressParams {
            current_file: "test.pak".to_string(),
            processed_files: 10,
            total_files: 100,
            total_size: 1000,
        };

        let result = emit_hash_file_progress(&emitter, &params);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_progress_events_for_file_check() {
        let emitter = MockEventEmitter::new();

        // Simulate checking multiple files
        for i in 1..=5 {
            let params = FileCheckProgressParams {
                current_file: format!("file_{}.pak", i),
                current_count: i,
                total_files: 5,
                elapsed_time: Duration::from_millis(i as u64 * 100),
                files_to_update: if i % 2 == 0 { i / 2 } else { 0 },
            };
            emit_file_check_progress(&emitter, &params).unwrap();
        }

        let events = emitter.events();
        assert_eq!(events.len(), 5);

        // All should be file_check_progress events
        for event in &events {
            assert_eq!(event.event, "file_check_progress");
        }
    }

    #[test]
    fn test_progress_events_at_intervals() {
        let emitter = MockEventEmitter::new();

        // Simulate emitting at intervals (every 100 files, like in production code)
        let total_files = 1000;
        for i in (100..=total_files).step_by(100) {
            let params = FileCheckProgressParams {
                current_file: format!("file_{}.pak", i),
                current_count: i,
                total_files,
                elapsed_time: Duration::from_secs(i as u64 / 100),
                files_to_update: i / 10,
            };
            emit_file_check_progress(&emitter, &params).unwrap();
        }

        let events = emitter.events();
        assert_eq!(events.len(), 10); // 100, 200, ..., 1000

        // Check that progress increases
        for (idx, event) in events.iter().enumerate() {
            let expected_progress = ((idx + 1) * 10) as f64;
            assert!(event.payload.contains(&expected_progress.to_string()));
        }
    }

    // ========================================================================
    // Additional edge case tests
    // ========================================================================

    #[test]
    fn test_calculate_hash_with_fs_large_binary_file() {
        // Test with 2MB of random binary data
        let large_content: Vec<u8> = (0..2_000_000).map(|i| (i % 256) as u8).collect();
        let mock = MockFileSystem::new().with_file("/large_binary.pak", &large_content);

        let result = calculate_file_hash_with_fs(&mock, Path::new("/large_binary.pak"));

        assert!(result.is_ok());
        let hash = result.unwrap();
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));

        // Verify consistency - hashing same content twice should give same hash
        let result2 = calculate_file_hash_with_fs(&mock, Path::new("/large_binary.pak"));
        assert_eq!(result2.unwrap(), hash);
    }

    #[test]
    fn test_check_file_needs_update_zero_size_file() {
        let empty_hash = calculate_hash_from_bytes(b"");
        let mock = MockFileSystem::new().with_file("/empty.pak", b"");

        let status =
            check_file_needs_update_with_fs(&mock, Path::new("/empty.pak"), &empty_hash, 0);

        assert_eq!(status, FileUpdateStatus::UpToDate);
    }

    #[test]
    fn test_check_file_needs_update_actual_larger_than_expected() {
        let content = b"this file is too large";
        let mock = MockFileSystem::new().with_file("/large.pak", content);

        let status = check_file_needs_update_with_fs(
            &mock,
            Path::new("/large.pak"),
            "somehash",
            10, // Expected size smaller than actual
        );

        match status {
            FileUpdateStatus::SizeMismatch { expected, actual } => {
                assert_eq!(expected, 10);
                assert_eq!(actual, content.len() as u64);
            }
            _ => panic!("Expected SizeMismatch, got {:?}", status),
        }
    }

    #[test]
    fn test_file_check_progress_with_varying_elapsed_times() {
        let emitter = MockEventEmitter::new();

        // Test with different elapsed time scenarios
        let test_cases = vec![
            (1, Duration::from_millis(100)),
            (50, Duration::from_secs(5)),
            (99, Duration::from_secs(30)),
            (100, Duration::from_secs(60)),
        ];

        for (count, elapsed) in test_cases {
            let params = FileCheckProgressParams {
                current_file: format!("file_{}.pak", count),
                current_count: count,
                total_files: 100,
                elapsed_time: elapsed,
                files_to_update: count / 5,
            };
            emit_file_check_progress(&emitter, &params).unwrap();
        }

        let events = emitter.events();
        assert_eq!(events.len(), 4);
    }

    #[test]
    fn test_emit_file_check_completed_with_zero_files() {
        let emitter = MockEventEmitter::new();
        let params = FileCheckCompletedParams {
            total_files: 0,
            files_to_update: 0,
            total_size: 0,
            total_time_seconds: 0,
            average_time_per_file_ms: 0.0,
        };

        let result = emit_file_check_completed(&emitter, &params);
        assert!(result.is_ok());

        let events = emitter.events();
        let payload = &events[0].payload;
        assert!(payload.contains("\"total_files\":0"));
    }

    #[test]
    fn test_emit_hash_file_progress_with_large_numbers() {
        let emitter = MockEventEmitter::new();
        let params = HashFileProgressParams {
            current_file: "huge_file.gpk".to_string(),
            processed_files: 999999,
            total_files: 1000000,
            total_size: 999_999_999_999,
        };

        let result = emit_hash_file_progress(&emitter, &params);
        assert!(result.is_ok());

        let events = emitter.events();
        let payload = &events[0].payload;
        assert!(payload.contains("\"processed_files\":999999"));
        assert!(payload.contains("\"total_size\":999999999999"));
    }

    #[test]
    fn test_multiple_file_status_checks_in_sequence() {
        let content1 = b"file 1 content";
        let content2 = b"file 2 content";
        let hash1 = calculate_hash_from_bytes(content1);
        let hash2 = calculate_hash_from_bytes(content2);

        let mock = MockFileSystem::new()
            .with_file("/file1.pak", content1)
            .with_file("/file2.pak", content2);

        // Check file 1 - should be up to date
        let status1 = check_file_needs_update_with_fs(
            &mock,
            Path::new("/file1.pak"),
            &hash1,
            content1.len() as u64,
        );
        assert_eq!(status1, FileUpdateStatus::UpToDate);

        // Check file 2 - should be up to date
        let status2 = check_file_needs_update_with_fs(
            &mock,
            Path::new("/file2.pak"),
            &hash2,
            content2.len() as u64,
        );
        assert_eq!(status2, FileUpdateStatus::UpToDate);

        // Check file 3 - should be missing
        let status3 =
            check_file_needs_update_with_fs(&mock, Path::new("/file3.pak"), "anyhash", 100);
        assert_eq!(status3, FileUpdateStatus::Missing);
    }

    #[test]
    fn test_check_file_with_special_characters_in_path() {
        let content = b"special path content";
        let hash = calculate_hash_from_bytes(content);
        let special_path = "/game/Data/файл.pak"; // Cyrillic characters

        let mock = MockFileSystem::new().with_file(special_path, content);

        let status = check_file_needs_update_with_fs(
            &mock,
            Path::new(special_path),
            &hash,
            content.len() as u64,
        );

        assert_eq!(status, FileUpdateStatus::UpToDate);
    }

    #[test]
    fn test_file_update_status_all_variants() {
        // Test all enum variants
        assert!(!FileUpdateStatus::UpToDate.needs_update());
        assert!(FileUpdateStatus::Missing.needs_update());
        assert!(FileUpdateStatus::MetadataError.needs_update());

        assert!(FileUpdateStatus::SizeMismatch {
            expected: 1000,
            actual: 500
        }
        .needs_update());

        assert!(FileUpdateStatus::HashMismatch {
            expected: "abc123".to_string(),
            actual: "def456".to_string()
        }
        .needs_update());

        assert!(FileUpdateStatus::HashError("IO error".to_string()).needs_update());
    }

    #[test]
    fn test_emit_events_with_maximum_values() {
        let emitter = MockEventEmitter::new();

        // Test with maximum u64 values
        let params = FileCheckProgressParams {
            current_file: "max_test.pak".to_string(),
            current_count: usize::MAX,
            total_files: usize::MAX,
            elapsed_time: Duration::from_secs(u64::MAX / 1000), // Avoid overflow
            files_to_update: usize::MAX,
        };

        let result = emit_file_check_progress(&emitter, &params);
        assert!(result.is_ok());
    }
}
