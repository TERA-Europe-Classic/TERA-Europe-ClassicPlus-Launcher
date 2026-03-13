//! Download-related Tauri commands
//!
//! This module contains commands for file download operations:
//! - Starting/resuming downloads
//! - Cancelling downloads
//! - Progress tracking
//!
//! # Testability
//!
//! This module provides testable inner functions that accept an `HttpClient`
//! implementation, allowing tests to use `MockHttpClient` for unit testing
//! without actual network access.

#![allow(dead_code)]

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use log::{error, info, warn};
use reqwest::header::RANGE;
use reqwest::Client;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::commands::config::get_game_path;
use crate::domain::DownloadError;
use crate::domain::{
    FileInfo, ProgressPayload, BUFFER_SIZE, BUFWRITER_CAPACITY, CHUNK_MIN_SIZE,
    CONNECT_TIMEOUT_SECS, HTTP_POOL_MAX_IDLE_PER_HOST, MAX_CONCURRENT_DOWNLOADS, MAX_PARTS,
    PART_ASSEMBLY_BUFFER_SIZE, PART_SIZE, PROGRESS_UPDATE_MS, STALL_TIMEOUT_SECS,
};
use crate::infrastructure::{EventEmitter, HttpClient, HttpResponse};
use crate::services::download_service;
use crate::services::download_service::{
    classify_error, DownloadHealth, ErrorClassification, RetryPolicy,
};
use crate::state::{
    add_downloaded_bytes, cancel_download, get_current_file_name, get_download_generation,
    get_downloaded_bytes as state_get_downloaded_bytes, increment_download_generation,
    is_download_cancelled, is_download_complete,
    reset_download_state as state_reset_download_state, set_current_file_name,
    set_download_cancelled, set_download_complete, set_download_in_progress, set_downloaded_bytes,
    sub_downloaded_bytes, try_start_download,
};
use crate::utils::{
    is_server_unreachable_error, stall_exceeded, validate_download_url, validate_path_within_base,
    RetryDelays,
};

/// Guard that ensures DOWNLOAD_IN_PROGRESS is reset when dropped
struct DownloadGuard;

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        set_download_in_progress(false);
    }
}

/// Gets the current downloaded byte count.
#[tauri::command]
pub fn get_downloaded_bytes() -> u64 {
    state_get_downloaded_bytes()
}

/// Cancels any ongoing downloads.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub fn cancel_downloads() {
    cancel_download();
}

/// Resets all download state to allow a fresh download attempt.
///
/// This is useful for recovering from "Download already in progress" errors
/// when no download is actually running (e.g., after a crash or unexpected error).
///
/// # Safety
/// This should only be called when you're certain no download is actually in progress,
/// as it will allow a new download to start even if one is running.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub fn reset_download_state() {
    info!("Resetting download state to allow fresh download attempt");
    state_reset_download_state();
}

/// Downloads all files in the update list.
///
/// This command manages concurrent downloads with progress tracking,
/// resumption support, and hash verification.
///
/// # Arguments
/// * `app_handle` - The Tauri app handle
/// * `window` - The Tauri window for emitting events
/// * `files_to_update` - List of files to download
/// * `resume_downloaded` - Optional byte count to resume from
///
/// # Returns
/// A list of downloaded file sizes on success
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn download_all_files(
    app_handle: tauri::AppHandle,
    window: tauri::Window,
    files_to_update: Vec<FileInfo>,
    resume_downloaded: Option<u64>,
) -> Result<Vec<u64>, String> {
    // Atomically check and set download in progress to prevent race conditions
    if !try_start_download() {
        return Err("Download already in progress".to_string());
    }
    let _download_guard = DownloadGuard; // Will clean up on panic or return

    // Increment generation to signal old progress tickers to exit
    let session_generation = increment_download_generation();

    let total_files = files_to_update.len();
    let total_size: u64 = files_to_update.iter().map(|f| f.size).sum();
    let initial_downloaded =
        download_service::compute_initial_downloaded(&files_to_update, resume_downloaded);

    if total_files == 0 {
        info!("No files to download");
        if let Err(e) = window.emit("download_complete", ()) {
            error!("Failed to emit download_complete event: {}", e);
        }
        return Ok(vec![]);
    }

    let mut results: Vec<Option<u64>> = vec![None; total_files];
    let mut files_by_index: Vec<Option<FileInfo>> = vec![None; total_files];
    set_downloaded_bytes(initial_downloaded);
    set_download_cancelled(false);
    set_download_complete(false);

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS));
    let global_start = Instant::now();

    // Create HTTP client once for all downloads to enable connection pooling
    let http_client = Arc::new(
        Client::builder()
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .read_timeout(Duration::from_secs(STALL_TIMEOUT_SECS))
            .pool_max_idle_per_host(HTTP_POOL_MAX_IDLE_PER_HOST)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?,
    );

    // Emit a smooth global progress tick to stabilize UI speed/ETA
    {
        let window_tick = window.clone();
        let total_bytes_tick = total_size;
        let total_files_tick = total_files;
        let generation = session_generation;
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(PROGRESS_UPDATE_MS));
            let mut last_bytes = initial_downloaded;
            let mut last_change = Instant::now();
            let mut consecutive_stalls: u8 = 0;
            loop {
                interval.tick().await;

                // Exit if a new download session started
                if get_download_generation() != generation {
                    break;
                }
                let d = state_get_downloaded_bytes();
                let elapsed = global_start.elapsed();
                let speed = if elapsed.as_secs() > 0 {
                    d.saturating_sub(initial_downloaded) / elapsed.as_secs()
                } else {
                    0
                };
                let current_file_name = get_current_file_name();

                if d != last_bytes {
                    last_bytes = d;
                    last_change = Instant::now();
                    consecutive_stalls = 0;
                } else if stall_exceeded(
                    last_bytes,
                    d,
                    last_change.elapsed().as_secs(),
                    STALL_TIMEOUT_SECS,
                ) {
                    consecutive_stalls += 1;
                    if consecutive_stalls == 1 {
                        // First stall: warn and reset timer to allow recovery
                        warn!(
                            "Download stall detected for {}, allowing recovery",
                            current_file_name
                        );
                        let _ = window_tick.emit(
                            "download_warning",
                            json!({
                                "message": "Download appears stalled, monitoring...",
                                "file": current_file_name,
                            }),
                        );
                        last_change = Instant::now();
                    } else {
                        // Second consecutive stall: error and cancel
                        error!("Download stalled after retry for {}", current_file_name);
                        let _ = window_tick.emit(
                            "download_error",
                            json!({
                                "message": "Download stalled. Please retry.",
                                "file": current_file_name,
                            }),
                        );
                        cancel_download();
                        set_download_complete(true);
                        break;
                    }
                }

                let payload = ProgressPayload {
                    file_name: current_file_name,
                    progress: if total_bytes_tick > 0 {
                        (d as f64 / total_bytes_tick as f64) * 100.0
                    } else {
                        0.0
                    },
                    speed: speed as f64,
                    downloaded_bytes: d,
                    total_bytes: total_bytes_tick,
                    base_downloaded: initial_downloaded,
                    total_files: total_files_tick,
                    elapsed_time: elapsed.as_secs_f64(),
                    current_file_index: 0,
                };
                let _ = window_tick.emit("global_download_progress", &payload);
                if d >= total_bytes_tick || is_download_cancelled() || is_download_complete() {
                    break;
                }
            }
        });
    }
    let mut join_set: JoinSet<(usize, Result<u64, String>)> = JoinSet::new();

    for (index, file_info) in files_to_update.into_iter().enumerate() {
        if is_download_cancelled() {
            let _ = window.emit("download_cancelled", ());
            break;
        }
        files_by_index[index] = Some(file_info.clone());
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        let app_handle_cl = app_handle.clone();
        let window_cl = window.clone();
        let file_info_cl = file_info.clone();
        let client_cl = http_client.clone();
        join_set.spawn(async move {
            let _permit = permit;
            let policy = RetryPolicy::default();
            let mut attempt: u8 = 0;

            let res = loop {
                let result = update_file(
                    app_handle_cl.clone(),
                    window_cl.clone(),
                    file_info_cl.clone(),
                    total_files,
                    index + 1,
                    total_size,
                    client_cl.clone(),
                )
                .await;

                match result {
                    Ok(size) => break Ok(size),
                    Err(e) if e == "cancelled" => break Err(e),
                    Err(e) => {
                        let classification = classify_error(&e);

                        match classification {
                            ErrorClassification::Cancelled => {
                                break Err("cancelled".to_string());
                            }
                            ErrorClassification::Permanent => {
                                // Don't retry permanent errors (404, permission denied, etc.)
                                // Note: Hash mismatch is now classified as Transient, not Permanent
                                break Err(format!("{}: {}", file_info_cl.path, e));
                            }
                            ErrorClassification::ServerUnreachable => {
                                // Only retry unreachable errors twice
                                if attempt >= 2 {
                                    let err = DownloadError::server_unreachable(attempt + 1, &e);
                                    break Err(format!("{}: {}", file_info_cl.path, err));
                                }
                                attempt = attempt.saturating_add(1);
                                tokio::time::sleep(Duration::from_millis(
                                    policy.delay_for_attempt(attempt),
                                ))
                                .await;
                            }
                            ErrorClassification::Transient => {
                                // Retry transient errors with exponential backoff
                                attempt = attempt.saturating_add(1);
                                if attempt > policy.max_retries {
                                    break Err(format!("{}: {}", file_info_cl.path, e));
                                }
                                tokio::time::sleep(Duration::from_millis(
                                    policy.delay_for_attempt(attempt),
                                ))
                                .await;
                            }
                        }
                    }
                }
            };
            (index, res)
        });
    }

    let mut failed_files: Vec<(String, String)> = Vec::new(); // (path, error)

    while let Some(jr) = join_set.join_next().await {
        match jr {
            Ok((idx, Ok(sz))) => {
                results[idx] = Some(sz);
            }
            Ok((idx, Err(e))) => {
                if e == "cancelled" {
                    let _ = window.emit("download_cancelled", ());
                    cancel_download();
                    break;
                } else {
                    // Track failure but continue with other files
                    let file_path = files_by_index[idx]
                        .as_ref()
                        .map(|f| f.path.clone())
                        .unwrap_or_else(|| format!("file_{}", idx));
                    warn!("File download failed: {} - {}", file_path, e);
                    let _ = window.emit(
                        "download_error",
                        json!({
                            "message": e.clone(),
                            "file": file_path.clone(),
                            "is_partial": true // Indicates download continues
                        }),
                    );
                    failed_files.push((file_path, e));
                }
            }
            Err(e) => {
                // JoinError is more serious but still track and continue
                warn!("Task join error: {}", e);
                failed_files.push(("unknown".to_string(), format!("Join error: {}", e)));
            }
        }
    }

    // If there were failures, emit summary and return error
    if !failed_files.is_empty() && !is_download_cancelled() {
        let failed_count = failed_files.len();
        let error_summary = failed_files
            .iter()
            .take(5) // Show first 5 failures
            .map(|(path, err)| format!("{}: {}", path, err))
            .collect::<Vec<_>>()
            .join("; ");

        let message = if failed_count > 5 {
            format!(
                "{} file(s) failed to download: {}... and {} more",
                failed_count,
                error_summary,
                failed_count - 5
            )
        } else {
            format!(
                "{} file(s) failed to download: {}",
                failed_count, error_summary
            )
        };

        let _ = window.emit(
            "download_error",
            json!({
                "message": message.clone(),
                "file": "",
                "failed_count": failed_count
            }),
        );

        set_download_complete(true);
        return Err(message);
    }

    let downloaded_sizes: Vec<u64> = results.into_iter().flatten().collect();

    if !is_download_cancelled() {
        // Emit verification start event
        let _ = window.emit(
            "download_verifying",
            json!({
                "status": "started",
                "total_files": total_files
            }),
        );

        // Post-download verification of files that completed
        let mut verified_count = 0usize;
        for maybe_file in files_by_index.into_iter() {
            if is_download_cancelled() {
                break;
            }
            let file_info = match maybe_file {
                Some(f) => f,
                None => continue,
            };
            let game_path = get_game_path()?;
            let file_path = game_path.join(&file_info.path);
            if !file_path.exists() {
                continue;
            }
            verified_count += 1;

            // Emit progress for verification
            let _ = window.emit(
                "download_verifying",
                json!({
                    "status": "verifying",
                    "file": file_info.path,
                    "verified": verified_count,
                    "total_files": total_files
                }),
            );

            let expected_hash = file_info.hash.clone();
            let file_path_for_size = file_path.clone();

            // Retry verification with exponential backoff (handles file locks, antivirus, etc.)
            const MAX_VERIFY_RETRIES: u8 = 5;
            let mut verify_attempt: u8 = 0;
            let mut calc: Option<String> = None;
            let mut last_error: Option<String> = None;

            while verify_attempt < MAX_VERIFY_RETRIES && !is_download_cancelled() {
                verify_attempt += 1;

                if verify_attempt > 1 {
                    // Emit retry event with the reason from the previous attempt
                    let reason = last_error
                        .clone()
                        .unwrap_or_else(|| "file access issue".to_string());
                    let _ = window.emit(
                        "download_verifying",
                        json!({
                            "status": "verify_retry",
                            "file": file_info.path,
                            "attempt": verify_attempt,
                            "max_attempts": MAX_VERIFY_RETRIES,
                            "reason": reason
                        }),
                    );
                    // Exponential backoff: 500ms, 1000ms, 2000ms, 4000ms
                    let delay_ms = 500u64 * 2u64.pow((verify_attempt - 2) as u32);
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }

                let file_path_clone = file_path.clone();
                match tokio::task::spawn_blocking(move || calculate_file_hash(&file_path_clone))
                    .await
                {
                    Ok(Ok(hash)) => {
                        calc = Some(hash);
                        break;
                    }
                    Ok(Err(e)) => {
                        // Hash calculation failed (file locked, permission denied, etc.)
                        warn!(
                            "Verification attempt {}/{} failed for {}: {}",
                            verify_attempt, MAX_VERIFY_RETRIES, file_info.path, e
                        );
                        last_error = Some(e);
                    }
                    Err(e) => {
                        // spawn_blocking task panicked or was cancelled
                        warn!(
                            "Verification task {}/{} failed for {}: {}",
                            verify_attempt, MAX_VERIFY_RETRIES, file_info.path, e
                        );
                        last_error = Some(e.to_string());
                    }
                }
            }

            // If all verification retries failed, log warning but continue anyway
            // The file was downloaded successfully, just couldn't verify the hash
            let calc = match calc {
                Some(h) => h,
                None => {
                    let error_detail = last_error.unwrap_or_else(|| "unknown error".to_string());
                    warn!(
                        "Could not verify {} after {} attempts ({}), assuming file is OK",
                        file_info.path, MAX_VERIFY_RETRIES, error_detail
                    );
                    // Emit warning event but continue - don't fail the download
                    let _ = window.emit(
                        "download_verifying",
                        json!({
                            "status": "verify_skipped",
                            "file": file_info.path,
                            "reason": error_detail
                        }),
                    );
                    // Skip verification for this file and continue to next
                    continue;
                }
            };
            if calc != expected_hash {
                // Get file size for debugging
                let actual_size = tokio::fs::metadata(&file_path_for_size)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);
                warn!(
                    "Hash mismatch for {}: expected {} but got {} (size: {} bytes, expected: {} bytes), redownloading",
                    file_info.path, expected_hash, calc, actual_size, file_info.size
                );

                // Emit hash mismatch event so UI can show retry status
                let _ = window.emit(
                    "download_verifying",
                    json!({
                        "status": "hash_mismatch",
                        "file": file_info.path,
                        "expected_size": file_info.size,
                        "actual_size": actual_size
                    }),
                );

                // Delete wrong file and retry with exponential backoff
                let _ = tokio::fs::remove_file(&file_path).await;

                // Subtract the failed file's bytes from progress counter
                // to prevent inflation during redownload
                sub_downloaded_bytes(file_info.size);

                let max_hash_retries: u8 = 3;
                let mut hash_attempt: u8 = 0;
                let mut redownload_success = false;

                while hash_attempt < max_hash_retries && !is_download_cancelled() {
                    hash_attempt += 1;
                    info!(
                        "Redownload attempt {}/{} for {} (hash mismatch)",
                        hash_attempt, max_hash_retries, file_info.path
                    );

                    // Emit retry event
                    let _ = window.emit(
                        "download_verifying",
                        json!({
                            "status": "retrying",
                            "file": file_info.path,
                            "attempt": hash_attempt,
                            "max_attempts": max_hash_retries
                        }),
                    );

                    let redownload_result = update_file(
                        app_handle.clone(),
                        window.clone(),
                        file_info.clone(),
                        total_files,
                        0,
                        total_size,
                        http_client.clone(),
                    )
                    .await;

                    match redownload_result {
                        Ok(_) => {
                            // Verify hash again after redownload
                            let file_path_verify = file_path.clone();
                            let verify_hash = tokio::task::spawn_blocking(move || {
                                calculate_file_hash(&file_path_verify)
                            })
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|r| r);

                            match verify_hash {
                                Ok(new_hash) if new_hash == expected_hash => {
                                    info!(
                                        "Successfully redownloaded and verified {}",
                                        file_info.path
                                    );
                                    redownload_success = true;
                                    break;
                                }
                                Ok(new_hash) => {
                                    warn!(
                                        "Redownload of {} still has hash mismatch: expected {}, got {}",
                                        file_info.path, expected_hash, new_hash
                                    );
                                    // Delete and try again, subtract bytes to keep progress accurate
                                    let _ = tokio::fs::remove_file(&file_path).await;
                                    sub_downloaded_bytes(file_info.size);
                                }
                                Err(e) => {
                                    warn!("Failed to verify hash after redownload: {}", e);
                                    // Emit verification error event so UI can show status
                                    let _ = window.emit(
                                        "download_verifying",
                                        json!({
                                            "status": "verification_error",
                                            "file": file_info.path,
                                            "error": e,
                                            "attempt": hash_attempt,
                                            "max_attempts": max_hash_retries
                                        }),
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Redownload attempt {}/{} failed for {}: {}",
                                hash_attempt, max_hash_retries, file_info.path, e
                            );
                        }
                    }

                    // Exponential backoff between retries
                    if hash_attempt < max_hash_retries {
                        let delay_ms = 500u64 * 2u64.pow(hash_attempt as u32);
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                }

                if !redownload_success && !is_download_cancelled() {
                    // Don't fail the entire download for one problematic file
                    // Skip this file and let the user try to play - they can repair later
                    let warning_msg = format!(
                        "Skipping {} after {} failed attempts - file may be corrupted. Try 'Repair Game Files' if you experience issues.",
                        file_info.path, max_hash_retries
                    );
                    warn!("{}", warning_msg);
                    let _ = window.emit(
                        "download_verifying",
                        json!({
                            "status": "file_skipped",
                            "file": file_info.path,
                            "reason": "persistent hash mismatch after redownload attempts"
                        }),
                    );
                    // Continue to next file instead of failing
                }
            }
        }
    }

    if is_download_cancelled() {
        info!("Download cancelled");
        set_download_complete(true);
    } else {
        // Emit verification complete event
        let _ = window.emit(
            "download_verifying",
            json!({
                "status": "completed",
                "total_files": total_files
            }),
        );

        info!("Download complete for {} file(s)", total_files);
        if let Err(e) = window.emit("download_complete", ()) {
            error!("Failed to emit download_complete event: {}", e);
        }
        set_download_complete(true);
    }

    Ok(downloaded_sizes)
}

/// Downloads a single file.
///
/// This is an internal function called by `download_all_files`. It is not
/// exposed as a Tauri command because it requires a shared HTTP client.
///
/// # Arguments
/// * `file_info` - Information about the file to download
/// * `client` - Shared HTTP client for connection pooling
#[cfg(not(tarpaulin_include))]
async fn update_file(
    _app_handle: tauri::AppHandle,
    _window: tauri::Window,
    file_info: FileInfo,
    _total_files: usize,
    _current_file_index: usize,
    _total_size: u64,
    client: Arc<Client>,
) -> Result<u64, String> {
    // Update the current file name for global progress tracking
    set_current_file_name(file_info.path.clone());

    let game_path = get_game_path()?;
    let file_path = game_path.join(&file_info.path);

    // Validate that the file path is within the game directory (prevent path traversal)
    let file_path = validate_path_within_base(&game_path, &file_path)?;

    if let Some(parent) = file_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }

    // HTTP client is now passed as parameter and shared across all downloads
    // This enables connection pooling and prevents connection exhaustion

    let mut corrected_url = file_info.url.clone();
    if let Some(pos) = corrected_url.find("/files/") {
        corrected_url = format!("{}{}", &corrected_url[..pos], &corrected_url[(pos + 7)..]);
    }

    // Defense-in-depth: Validate URL domain even though hash.rs also validates
    // This catches any code paths that bypass hash.rs validation
    validate_download_url(&corrected_url)?;

    let file_size = file_info.size;
    let file_info_path = file_info.path.clone();

    // Create file (automatically truncates/overwrites if exists)
    let file_handle = tokio::fs::File::create(&file_path)
        .await
        .map_err(|e| e.to_string())?;
    let mut file = BufWriter::with_capacity(BUFWRITER_CAPACITY, file_handle);

    // Server doesn't support Range requests (returns 416), so always download fresh
    let resume_from: u64 = 0;
    let allow_parallel = false;

    info!("Downloading file: {} ({} bytes)", file_info.path, file_size);
    let bytes_written: u64;

    // resume_from is always 0 since server doesn't support ranges, but keep structure for future
    if resume_from > 0 {
        info!("Attempting resume download from byte {}", resume_from);
        #[allow(unused_assignments)]
        let mut resume_from = resume_from; // Make mutable for the code below
        let range_probe = client
            .get(&corrected_url)
            .header(RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        // Only consider range supported if we get 206 Partial Content
        // A 416 with content-range header (bytes */0) means NOT supported
        let supports_range = range_probe.status() == reqwest::StatusCode::PARTIAL_CONTENT;

        if !supports_range {
            // Server doesn't support range requests - must start fresh
            info!(
                "Range not supported (status {}), resetting for fresh download",
                range_probe.status()
            );
            sub_downloaded_bytes(resume_from);
            resume_from = 0;
            drop(file);
            if let Err(e) = tokio::fs::remove_file(&file_path).await {
                error!("Failed to remove file for fresh download: {}", e);
            }
            let file_handle = tokio::fs::File::create(&file_path)
                .await
                .map_err(|e| e.to_string())?;
            file = BufWriter::with_capacity(BUFWRITER_CAPACITY, file_handle);
        } else if supports_range {
            let range_header = format!("bytes={}-", resume_from);
            let res = client
                .get(&corrected_url)
                .header(RANGE, range_header)
                .header("Accept-Encoding", "gzip, deflate, br")
                .header("Connection", "keep-alive")
                .header("Cache-Control", "no-cache")
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;

            // Check for empty response when resume should have content
            if res.content_length() == Some(0) && (file_size - resume_from) > 0 {
                return Err(format!(
                    "Server returned empty response for resume on {} (expected {} bytes remaining)",
                    file_info.path,
                    file_size - resume_from
                ));
            }

            let status = res.status();
            let has_content_range = res.headers().get("content-range").is_some();
            if status != reqwest::StatusCode::PARTIAL_CONTENT && !has_content_range {
                sub_downloaded_bytes(resume_from);
                resume_from = 0;
                drop(file);
                tokio::fs::remove_file(&file_path).await.ok();
                let file_handle = tokio::fs::File::create(&file_path)
                    .await
                    .map_err(|e| e.to_string())?;
                file = BufWriter::with_capacity(BUFWRITER_CAPACITY, file_handle);
            } else {
                let mut downloaded: u64 = 0;
                let mut stream = res.bytes_stream();

                while let Some(chunk_result) = stream.next().await {
                    if is_download_cancelled() {
                        return Err("cancelled".into());
                    }
                    let chunk = chunk_result.map_err(|e| e.to_string())?;
                    file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                    let len = chunk.len() as u64;
                    downloaded += len;
                    add_downloaded_bytes(len);
                }
                bytes_written = resume_from + downloaded;
                return Ok(bytes_written);
            }
        }

        sub_downloaded_bytes(resume_from);
        drop(file);
        tokio::fs::remove_file(&file_path).await.ok();
        let file_handle = tokio::fs::File::create(&file_path)
            .await
            .map_err(|e| e.to_string())?;
        file = BufWriter::with_capacity(1024 * 1024, file_handle);
    }

    if allow_parallel && file_size >= CHUNK_MIN_SIZE {
        // Check if server supports range requests; fallback if not
        let range_probe = client
            .get(&corrected_url)
            .header(RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        // Only consider range supported if we get 206 Partial Content
        // A 416 with content-range header (bytes */0) means NOT supported
        let supports_range = range_probe.status() == reqwest::StatusCode::PARTIAL_CONTENT;

        if !supports_range {
            info!(
                "Server does not support range requests. Falling back to single-stream for {}",
                file_info.path
            );
            // Single-stream fallback
            let res = client
                .get(&corrected_url)
                .header("Accept-Encoding", "gzip, deflate, br")
                .header("Connection", "keep-alive")
                .header("Cache-Control", "no-cache")
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;

            let mut downloaded: u64 = 0;
            let mut stream = res.bytes_stream();

            while let Some(chunk_result) = stream.next().await {
                if is_download_cancelled() {
                    return Err("cancelled".into());
                }
                let chunk = chunk_result.map_err(|e| e.to_string())?;
                file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                let len = chunk.len() as u64;
                downloaded += len;
                add_downloaded_bytes(len);
            }
            bytes_written = downloaded;
        } else {
            // Perform chunked parallel download using HTTP ranges into temp parts
            let num_parts = std::cmp::max(
                1,
                std::cmp::min(MAX_PARTS as u64, file_size.div_ceil(PART_SIZE)) as usize,
            );

            // Use the shared client passed from download_all_files for connection pooling
            let part_client = Arc::clone(&client);

            let mut join_set: JoinSet<Result<(), String>> = JoinSet::new();

            for part_idx in 0..num_parts {
                if is_download_cancelled() {
                    return Err("cancelled".into());
                }

                let start = (part_idx as u64) * PART_SIZE;
                let mut end = ((part_idx as u64 + 1) * PART_SIZE).saturating_sub(1);
                if end >= file_size {
                    end = file_size - 1;
                }
                let expected_part_size = end - start + 1;

                let part_url = corrected_url.clone();
                let part_path = file_path.with_extension(format!("part{}", part_idx));
                let file_info_path_clone = file_info_path.clone();
                let client_part = Arc::clone(&part_client);

                join_set.spawn(async move {
                    let range_header = format!("bytes={}-{}", start, end);
                    let res = client_part
                        .get(&part_url)
                        .header(RANGE, range_header)
                        .header("Accept-Encoding", "gzip, deflate, br")
                        .header("Connection", "keep-alive")
                        .header("Cache-Control", "no-cache")
                        .send()
                        .await
                        .map_err(|e| e.to_string())?
                        .error_for_status()
                        .map_err(|e| e.to_string())?;

                    // Check for empty response when range should have content - this is an error
                    if res.content_length() == Some(0) && expected_part_size > 0 {
                        return Err(format!(
                            "Server returned empty response for range request on {} (expected {} bytes)",
                            file_info_path_clone, expected_part_size
                        ));
                    }

                    let mut stream = res.bytes_stream();
                    let mut part_file = BufWriter::with_capacity(
                        BUFWRITER_CAPACITY,
                        tokio::fs::File::create(&part_path)
                            .await
                            .map_err(|e| e.to_string())?,
                    );
                    while let Some(chunk_result) = stream.next().await {
                        if is_download_cancelled() {
                            return Err("cancelled".into());
                        }
                        let chunk = chunk_result.map_err(|e| e.to_string())?;
                        part_file
                            .write_all(&chunk)
                            .await
                            .map_err(|e| e.to_string())?;
                        let len = chunk.len() as u64;
                        add_downloaded_bytes(len);
                    }

                    part_file.flush().await.map_err(|e| e.to_string())?;
                    Ok(())
                });
            }

            while let Some(res) = join_set.join_next().await {
                match res {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        // Clean up part files on any error to prevent disk space accumulation
                        // The retry logic will recreate parts as needed
                        for cleanup_idx in 0..num_parts {
                            let part_path =
                                file_path.with_extension(format!("part{}", cleanup_idx));
                            let _ = tokio::fs::remove_file(&part_path).await;
                        }
                        return Err(e);
                    }
                    Err(e) => {
                        return Err(format!("Join error: {}", e));
                    }
                }
            }

            // Assemble parts
            for part_idx in 0..num_parts {
                let part_path = file_path.with_extension(format!("part{}", part_idx));
                let mut part_f = tokio::fs::File::open(&part_path)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut buf = vec![0u8; PART_ASSEMBLY_BUFFER_SIZE];
                loop {
                    let n = part_f.read(&mut buf).await.map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    file.write_all(&buf[..n]).await.map_err(|e| e.to_string())?;
                }
                tokio::fs::remove_file(&part_path).await.ok();
            }
            bytes_written = file_size;
        }
    } else {
        // Single-stream download
        let res = client
            .get(&corrected_url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;

        // Check for empty response when file should have content - this is an error
        if res.content_length() == Some(0) && file_info.size > 0 {
            return Err(format!(
                "Server returned empty file for {} (expected {} bytes)",
                file_info.path, file_info.size
            ));
        }

        let mut downloaded: u64 = 0;
        let mut stream = res.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            if is_download_cancelled() {
                return Err("cancelled".into());
            }
            let chunk = chunk_result.map_err(|e| e.to_string())?;
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            let len = chunk.len() as u64;
            downloaded += len;
            add_downloaded_bytes(len);
        }
        bytes_written = downloaded;
    }

    // Ensure all data is written to disk
    file.flush().await.map_err(|e| e.to_string())?;
    let inner_file = file.into_inner();
    inner_file.sync_all().await.map_err(|e| e.to_string())?;

    // Validate downloaded size matches expected - return error to trigger retry
    if bytes_written != file_size {
        let error_msg = format!(
            "Download size mismatch for {}: expected {} bytes but got {} bytes",
            file_info_path, file_size, bytes_written
        );
        error!("{}", error_msg);
        // Subtract the incorrect bytes so progress tracking stays accurate
        sub_downloaded_bytes(bytes_written);
        // Delete the incomplete file
        if let Err(e) = tokio::fs::remove_file(&file_path).await {
            warn!("Failed to remove incomplete file: {}", e);
        }
        return Err(error_msg);
    }

    info!("File download completed: {}", file_info_path);

    Ok(bytes_written)
}

// ============================================================================
// Testable inner functions with HttpClient trait
// ============================================================================

/// Result of a file download operation.
#[derive(Debug, Clone)]
pub struct DownloadResult {
    /// Total bytes written to the file (including any resumed portion).
    pub bytes_written: u64,
    /// Whether this was a resumed download.
    pub was_resumed: bool,
}

/// Downloads file content using an HttpClient implementation.
///
/// This is the core testable download function that accepts any `HttpClient`
/// implementation, allowing mocking in tests.
///
/// # Arguments
/// * `client` - The HTTP client to use for downloading
/// * `url` - The URL to download from
/// * `path` - The path to write the downloaded content to
/// * `resume_from` - Byte offset to resume from (0 for fresh download)
/// * `check_cancelled` - Closure to check if download should be cancelled
/// * `on_progress` - Closure called with bytes downloaded so far
///
/// # Returns
/// A `DownloadResult` with the total bytes written and resume status.
pub async fn download_file_with_client<H, F, P>(
    client: &H,
    url: &str,
    path: &Path,
    resume_from: u64,
    check_cancelled: F,
    on_progress: P,
) -> Result<DownloadResult, String>
where
    H: HttpClient,
    F: Fn() -> bool,
    P: Fn(u64),
{
    // Check if we should resume
    let response = if resume_from > 0 {
        // First probe if server supports range requests
        let probe = client.get_range(url, 0, Some(0)).await?;
        let supports_range = probe.is_partial() || probe.supports_range;

        if supports_range {
            // Request from resume point
            let resp = client.get_range(url, resume_from, None).await?;
            // Note: This error path is difficult to test with MockHttpClient since it returns
            // the same response for all requests to the same URL, making it impossible to
            // have probe succeed but actual range request fail.
            if !resp.is_success() && !resp.is_partial() {
                return Err(format!("HTTP error: {}", resp.status));
            }
            resp
        } else {
            // Server doesn't support range, start fresh
            let resp = client.get(url).await?;
            // Note: This error path is difficult to test with MockHttpClient since probe
            // returning 200 (no range support) would also return 200 for the actual GET.
            if !resp.is_success() {
                return Err(format!("HTTP error: {}", resp.status));
            }
            // Return with resume_from = 0 to indicate fresh start
            return download_content_to_file(resp, path, 0, check_cancelled, on_progress).await;
        }
    } else {
        let resp = client.get(url).await?;
        if !resp.is_success() {
            return Err(format!("HTTP error: {}", resp.status));
        }
        resp
    };

    download_content_to_file(response, path, resume_from, check_cancelled, on_progress).await
}

/// Writes HTTP response content to a file.
async fn download_content_to_file<F, P>(
    response: HttpResponse,
    path: &Path,
    resume_from: u64,
    check_cancelled: F,
    on_progress: P,
) -> Result<DownloadResult, String>
where
    F: Fn() -> bool,
    P: Fn(u64),
{
    // Check for cancellation before starting
    if check_cancelled() {
        return Err("cancelled".into());
    }

    // Create parent directories if needed
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    // Open file for writing (append if resuming)
    let file_handle = if resume_from > 0 {
        tokio::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .await
            .map_err(|e| format!("Failed to open file for append: {}", e))?
    } else {
        tokio::fs::File::create(path)
            .await
            .map_err(|e| format!("Failed to create file: {}", e))?
    };

    let mut file = BufWriter::with_capacity(BUFWRITER_CAPACITY, file_handle);
    let body = &response.body;
    let total_bytes = body.len() as u64;

    // Write in chunks to allow cancellation checks
    const CHUNK_SIZE: usize = 65536; // 64KB chunks for progress reporting
    let mut written: u64 = 0;

    for chunk in body.chunks(CHUNK_SIZE) {
        if check_cancelled() {
            // Flush what we have before returning
            let _ = file.flush().await;
            return Err("cancelled".into());
        }

        file.write_all(chunk)
            .await
            .map_err(|e| format!("Failed to write to file: {}", e))?;

        written += chunk.len() as u64;
        on_progress(written);
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush file: {}", e))?;

    Ok(DownloadResult {
        bytes_written: resume_from + total_bytes,
        was_resumed: resume_from > 0,
    })
}

/// Corrects file URLs by removing duplicate "/files/" path segment.
///
/// Some URLs may have malformed paths with duplicate "/files/" segments.
/// This function normalizes them.
pub fn correct_file_url(url: &str) -> String {
    download_service::correct_download_url(url)
}

// ============================================================================
// Resilient download with retry support
// ============================================================================

/// Downloads a stream with retry support for transient errors.
///
/// This wraps the stream processing in a retry loop that:
/// - Catches transient errors (network glitches, server overload)
/// - Uses exponential backoff between retries
/// - Only gives up if server is truly unreachable OR retries exhausted for permanent errors
///
/// # Arguments
/// * `client` - HTTP client implementation
/// * `url` - URL to download from
/// * `path` - Target file path
/// * `policy` - Retry policy to use
/// * `check_cancelled` - Cancellation check closure
/// * `on_progress` - Progress callback
///
/// # Returns
/// * `Ok(DownloadResult)` on success
/// * `Err(DownloadError)` only if:
///   - Server is unreachable (after retries with exponential backoff)
///   - Permanent error (404, hash mismatch, etc.)
///   - User cancelled
pub async fn download_with_resilience<H, F, P>(
    client: &H,
    url: &str,
    path: &Path,
    policy: &RetryPolicy,
    check_cancelled: F,
    on_progress: P,
) -> Result<DownloadResult, DownloadError>
where
    H: HttpClient,
    F: Fn() -> bool + Clone,
    P: Fn(u64) + Clone,
{
    let mut health = DownloadHealth::new();
    let mut last_error = String::new();

    for (attempt, delay) in RetryDelays::new(policy.max_retries).enumerate() {
        if check_cancelled() {
            return Err(DownloadError::Cancelled);
        }

        // Apply backoff delay (except first attempt)
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }

        match download_file_with_client(
            client,
            url,
            path,
            0, // Always start fresh for now (server doesn't support resume)
            check_cancelled.clone(),
            on_progress.clone(),
        )
        .await
        {
            Ok(result) => {
                health.record_success();
                return Ok(result);
            }
            Err(e) => {
                last_error = e.clone();
                let classification = classify_error(&e);

                match classification {
                    ErrorClassification::Cancelled => {
                        return Err(DownloadError::Cancelled);
                    }
                    ErrorClassification::Permanent => {
                        return Err(DownloadError::from(e));
                    }
                    ErrorClassification::ServerUnreachable => {
                        // Only retry unreachable twice before giving up
                        if attempt >= 2 {
                            return Err(DownloadError::server_unreachable(attempt as u8 + 1, e));
                        }
                        health.record_retry();
                    }
                    ErrorClassification::Transient => {
                        health.record_retry();
                        // Continue to next retry attempt
                    }
                }
            }
        }
    }

    // Exhausted all retries
    if is_server_unreachable_error(&last_error) {
        Err(DownloadError::server_unreachable(
            policy.max_retries,
            last_error,
        ))
    } else {
        Err(DownloadError::from(last_error))
    }
}

/// Determines if a download error should cause the entire download to fail.
///
/// This is a helper function for the download loop to decide whether to emit
/// an error event or continue silently with the next file.
///
/// # Arguments
/// * `error` - The download error
///
/// # Returns
/// `true` if the error is fatal (server unreachable or permanent), `false` if transient
pub fn is_fatal_download_error(error: &DownloadError) -> bool {
    match error {
        DownloadError::Cancelled => true,
        DownloadError::ServerUnreachable { .. } => true,
        DownloadError::Http { status, .. } => *status >= 400 && *status < 500,
        // Hash mismatch is NOT fatal - it means corruption, so delete and retry
        DownloadError::HashMismatch { .. } => false,
        DownloadError::FileSystem(_) => true,
        DownloadError::Network(_) => false,
        DownloadError::StreamInterrupted { .. } => false,
    }
}

// ============================================================================
// Testable progress emission functions with EventEmitter trait
// ============================================================================

/// Parameters for download progress calculation.
#[derive(Debug, Clone)]
pub struct DownloadProgressParams {
    /// Current file being downloaded.
    pub current_file_name: String,
    /// Bytes downloaded so far.
    pub downloaded_bytes: u64,
    /// Total bytes to download.
    pub total_bytes: u64,
    /// Base (already downloaded) bytes for resumption.
    pub base_downloaded: u64,
    /// Total number of files.
    pub total_files: usize,
    /// Time elapsed since download started.
    pub elapsed_time: Duration,
    /// Current file index (1-based).
    pub current_file_index: usize,
}

/// Emits a global download progress event.
///
/// This is the testable inner function that can use any `EventEmitter`.
///
/// # Arguments
/// * `emitter` - The event emitter implementation
/// * `params` - Progress parameters
///
/// # Returns
/// `Ok(())` on success, `Err` on emission failure.
pub fn emit_download_progress<E: EventEmitter>(
    emitter: &E,
    params: &DownloadProgressParams,
) -> Result<(), String> {
    let session_downloaded = params
        .downloaded_bytes
        .saturating_sub(params.base_downloaded);
    let speed =
        download_service::calculate_speed(session_downloaded, params.elapsed_time.as_secs());
    let progress =
        download_service::calculate_progress(params.downloaded_bytes, params.total_bytes);

    let payload = ProgressPayload {
        file_name: params.current_file_name.clone(),
        progress,
        speed: speed as f64,
        downloaded_bytes: params.downloaded_bytes,
        total_bytes: params.total_bytes,
        base_downloaded: params.base_downloaded,
        total_files: params.total_files,
        elapsed_time: params.elapsed_time.as_secs_f64(),
        current_file_index: params.current_file_index,
    };

    emitter.emit("global_download_progress", &payload)
}

/// Emits a download stall error event.
///
/// # Arguments
/// * `emitter` - The event emitter implementation
/// * `current_file` - The file that was being downloaded when stall occurred
pub fn emit_download_stall_error<E: EventEmitter>(
    emitter: &E,
    current_file: &str,
) -> Result<(), String> {
    emitter.emit(
        "download_error",
        json!({
            "message": "Download stalled. Please retry.",
            "file": current_file,
        }),
    )
}

/// Emits a download complete event.
///
/// # Arguments
/// * `emitter` - The event emitter implementation
pub fn emit_download_complete<E: EventEmitter>(emitter: &E) -> Result<(), String> {
    emitter.emit("download_complete", ())
}

/// Emits a download cancelled event.
///
/// # Arguments
/// * `emitter` - The event emitter implementation
pub fn emit_download_cancelled<E: EventEmitter>(emitter: &E) -> Result<(), String> {
    emitter.emit("download_cancelled", ())
}

/// Emits a download error event.
///
/// # Arguments
/// * `emitter` - The event emitter implementation
/// * `message` - Error message
/// * `file` - The file that caused the error
pub fn emit_download_error<E: EventEmitter>(
    emitter: &E,
    message: &str,
    file: &str,
) -> Result<(), String> {
    emitter.emit(
        "download_error",
        json!({
            "message": message,
            "file": file,
        }),
    )
}

// ============================================================================
// Internal helper functions
// ============================================================================

/// Computes the initial downloaded byte count for a set of files.
fn compute_initial_downloaded(files: &[FileInfo], resume_override: Option<u64>) -> u64 {
    download_service::compute_initial_downloaded(files, resume_override)
}

/// Calculates the SHA-256 hash of a file with retry on file access errors.
///
/// On Windows, files may be briefly locked by other processes (antivirus, etc.)
/// immediately after download. This function retries with exponential backoff
/// to handle such transient access issues.
#[cfg(not(tarpaulin_include))]
fn calculate_file_hash<P: AsRef<std::path::Path>>(path: P) -> Result<String, String> {
    const MAX_OPEN_RETRIES: u8 = 3;
    let mut attempt = 0;

    let file = loop {
        match File::open(path.as_ref()) {
            Ok(f) => break f,
            Err(e) => {
                attempt += 1;
                if attempt >= MAX_OPEN_RETRIES {
                    return Err(format!(
                        "Failed to open file after {} attempts: {} - try deleting the file and relaunching the launcher",
                        MAX_OPEN_RETRIES, e
                    ));
                }
                // Exponential backoff: 500ms, 1000ms
                let delay_ms = 500u64 * (1 << (attempt - 1));
                log::debug!(
                    "File open attempt {}/{} failed for {:?}, retrying in {}ms: {}",
                    attempt,
                    MAX_OPEN_RETRIES,
                    path.as_ref(),
                    delay_ms,
                    e
                );
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
        }
    };

    let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; BUFFER_SIZE];

    loop {
        let bytes_read = reader.read(&mut buffer).map_err(|e| {
            format!(
                "Failed to read file during verification: {} - try deleting the file and relaunching the launcher",
                e
            )
        })?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::MockEventEmitter;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn test_get_downloaded_bytes() {
        set_downloaded_bytes(1234);
        assert_eq!(get_downloaded_bytes(), 1234);
    }

    #[test]
    fn compute_initial_downloaded_empty_files_array() {
        let files: Vec<FileInfo> = vec![];
        assert_eq!(compute_initial_downloaded(&files, None), 0);
        assert_eq!(compute_initial_downloaded(&files, Some(100)), 100);
    }

    #[test]
    fn compute_initial_downloaded_none_resume_override() {
        let files = vec![
            FileInfo {
                path: "a".to_string(),
                hash: "h".to_string(),
                size: 100,
                url: "u".to_string(),
                existing_size: 50,
            },
            FileInfo {
                path: "b".to_string(),
                hash: "h".to_string(),
                size: 200,
                url: "u".to_string(),
                existing_size: 100,
            },
        ];
        // Sum of resume_offsets: 50 + 100 = 150
        assert_eq!(compute_initial_downloaded(&files, None), 150);
    }

    #[test]
    fn compute_initial_downloaded_override_less_than_existing() {
        let files = vec![FileInfo {
            path: "a".to_string(),
            hash: "h".to_string(),
            size: 100,
            url: "u".to_string(),
            existing_size: 80,
        }];
        // Existing is 80, override is 50, so we use existing (80)
        assert_eq!(compute_initial_downloaded(&files, Some(50)), 80);
    }

    #[test]
    fn compute_initial_downloaded_clamping_with_positive_size() {
        let files = vec![FileInfo {
            path: "a".to_string(),
            hash: "h".to_string(),
            size: 50,
            url: "u".to_string(),
            existing_size: 0,
        }];
        // Override 100 exceeds total_size 50, should be clamped
        assert_eq!(compute_initial_downloaded(&files, Some(100)), 50);
    }

    // =========================================================================
    // EventEmitter tests
    // =========================================================================

    #[test]
    fn test_emit_download_progress_emits_correct_event() {
        let emitter = MockEventEmitter::new();
        let params = DownloadProgressParams {
            current_file_name: "test_file.dat".to_string(),
            downloaded_bytes: 500,
            total_bytes: 1000,
            base_downloaded: 0,
            total_files: 5,
            elapsed_time: Duration::from_secs(10),
            current_file_index: 1,
        };

        let result = emit_download_progress(&emitter, &params);
        assert!(result.is_ok());

        let events = emitter.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "global_download_progress");
    }

    #[test]
    fn test_emit_download_progress_payload_contains_expected_fields() {
        let emitter = MockEventEmitter::new();
        let params = DownloadProgressParams {
            current_file_name: "game_data.pak".to_string(),
            downloaded_bytes: 750,
            total_bytes: 1000,
            base_downloaded: 0,
            total_files: 3,
            elapsed_time: Duration::from_secs(5),
            current_file_index: 2,
        };

        emit_download_progress(&emitter, &params).unwrap();

        let events = emitter.events();
        let payload = &events[0].payload;

        // Verify payload contains expected fields
        assert!(payload.contains("\"file_name\":\"game_data.pak\""));
        assert!(payload.contains("\"downloaded_bytes\":750"));
        assert!(payload.contains("\"total_bytes\":1000"));
        assert!(payload.contains("\"total_files\":3"));
        assert!(payload.contains("\"current_file_index\":2"));
        // Progress should be 75%
        assert!(payload.contains("75"));
    }

    #[test]
    fn test_emit_download_progress_calculates_speed_correctly() {
        let emitter = MockEventEmitter::new();
        let params = DownloadProgressParams {
            current_file_name: "file.bin".to_string(),
            downloaded_bytes: 1000,
            total_bytes: 2000,
            base_downloaded: 0,
            total_files: 1,
            elapsed_time: Duration::from_secs(10),
            current_file_index: 1,
        };

        emit_download_progress(&emitter, &params).unwrap();

        let events = emitter.events();
        let payload = &events[0].payload;

        // Speed should be 1000 bytes / 10 seconds = 100 bytes/sec
        assert!(payload.contains("\"speed\":100"));
    }

    #[test]
    fn test_emit_download_progress_handles_zero_elapsed_time() {
        let emitter = MockEventEmitter::new();
        let params = DownloadProgressParams {
            current_file_name: "file.bin".to_string(),
            downloaded_bytes: 500,
            total_bytes: 1000,
            base_downloaded: 0,
            total_files: 1,
            elapsed_time: Duration::from_secs(0),
            current_file_index: 1,
        };

        let result = emit_download_progress(&emitter, &params);
        assert!(result.is_ok());

        let events = emitter.events();
        let payload = &events[0].payload;

        // Speed should be 0 when elapsed time is 0
        assert!(payload.contains("\"speed\":0"));
    }

    #[test]
    fn test_emit_download_progress_handles_zero_total_bytes() {
        let emitter = MockEventEmitter::new();
        let params = DownloadProgressParams {
            current_file_name: "empty.dat".to_string(),
            downloaded_bytes: 0,
            total_bytes: 0,
            base_downloaded: 0,
            total_files: 1,
            elapsed_time: Duration::from_secs(1),
            current_file_index: 1,
        };

        let result = emit_download_progress(&emitter, &params);
        assert!(result.is_ok());

        let events = emitter.events();
        let payload = &events[0].payload;

        // Progress should be 0 when total is 0
        assert!(payload.contains("\"progress\":0"));
    }

    #[test]
    fn test_emit_download_stall_error() {
        let emitter = MockEventEmitter::new();
        let result = emit_download_stall_error(&emitter, "stuck_file.pak");

        assert!(result.is_ok());
        let events = emitter.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "download_error");
        assert!(events[0].payload.contains("Download stalled"));
        assert!(events[0].payload.contains("stuck_file.pak"));
    }

    #[test]
    fn test_emit_download_complete() {
        let emitter = MockEventEmitter::new();
        let result = emit_download_complete(&emitter);

        assert!(result.is_ok());
        let events = emitter.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "download_complete");
    }

    #[test]
    fn test_emit_download_cancelled() {
        let emitter = MockEventEmitter::new();
        let result = emit_download_cancelled(&emitter);

        assert!(result.is_ok());
        let events = emitter.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "download_cancelled");
    }

    #[test]
    fn test_emit_download_error() {
        let emitter = MockEventEmitter::new();
        let result = emit_download_error(&emitter, "Connection failed", "broken_file.dat");

        assert!(result.is_ok());
        let events = emitter.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "download_error");
        assert!(events[0].payload.contains("Connection failed"));
        assert!(events[0].payload.contains("broken_file.dat"));
    }

    #[test]
    fn test_emit_with_failing_emitter() {
        let emitter = MockEventEmitter::failing();
        let params = DownloadProgressParams {
            current_file_name: "file.bin".to_string(),
            downloaded_bytes: 100,
            total_bytes: 200,
            base_downloaded: 0,
            total_files: 1,
            elapsed_time: Duration::from_secs(1),
            current_file_index: 1,
        };

        let result = emit_download_progress(&emitter, &params);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Mock emit failure");
    }

    #[test]
    fn test_multiple_progress_events_emitted() {
        let emitter = MockEventEmitter::new();

        // Simulate multiple progress updates
        for i in 1..=5 {
            let params = DownloadProgressParams {
                current_file_name: format!("file_{}.dat", i),
                downloaded_bytes: i * 100,
                total_bytes: 500,
                base_downloaded: 0,
                total_files: 5,
                elapsed_time: Duration::from_secs(i),
                current_file_index: i as usize,
            };
            emit_download_progress(&emitter, &params).unwrap();
        }

        let events = emitter.events();
        assert_eq!(events.len(), 5);

        // All should be progress events
        for event in &events {
            assert_eq!(event.event, "global_download_progress");
        }
    }

    #[test]
    fn test_correct_file_url_removes_duplicate_files_segment() {
        let url = "https://example.com/files/game/data.pak";
        let corrected = correct_file_url(url);
        assert_eq!(corrected, "https://example.com/game/data.pak");
    }

    #[test]
    fn test_correct_file_url_leaves_normal_url_unchanged() {
        let url = "https://example.com/game/data.pak";
        let corrected = correct_file_url(url);
        assert_eq!(corrected, "https://example.com/game/data.pak");
    }

    // =========================================================================
    // HttpClient download tests using MockHttpClient
    // =========================================================================

    use crate::infrastructure::MockHttpClient;
    use std::sync::atomic::AtomicBool;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_download_success() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("test_file.bin");
        let url = "https://example.com/file.bin";
        let file_content = b"Hello, this is test file content!";

        let mock = MockHttpClient::new();
        mock.add_response(
            url,
            HttpResponse {
                status: 200,
                body: file_content.to_vec(),
                content_length: Some(file_content.len() as u64),
                supports_range: false,
            },
        );

        let progress_bytes = Arc::new(AtomicU64::new(0));
        let progress_bytes_clone = progress_bytes.clone();

        let result = download_file_with_client(
            &mock,
            url,
            &file_path,
            0,
            || false, // Never cancelled
            move |bytes| {
                progress_bytes_clone.store(bytes, Ordering::SeqCst);
            },
        )
        .await;

        assert!(result.is_ok(), "Download should succeed: {:?}", result);
        let download_result = result.unwrap();
        assert_eq!(download_result.bytes_written, file_content.len() as u64);
        assert!(!download_result.was_resumed);

        // Verify file content
        let written_content = tokio::fs::read(&file_path)
            .await
            .expect("Failed to read file");
        assert_eq!(written_content, file_content);

        // Verify progress was reported
        assert_eq!(
            progress_bytes.load(Ordering::SeqCst),
            file_content.len() as u64
        );
    }

    #[tokio::test]
    async fn test_download_resume_with_range_support() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("resumed_file.bin");
        let url = "https://example.com/large_file.bin";

        // Simulate partial download: first 10 bytes already written
        let existing_content = b"0123456789";
        let remaining_content = b"abcdefghij";
        tokio::fs::write(&file_path, existing_content)
            .await
            .expect("Failed to write existing content");

        let mock = MockHttpClient::new();
        // Mock range probe response (206 Partial Content)
        mock.add_response(
            url,
            HttpResponse {
                status: 206,
                body: remaining_content.to_vec(),
                content_length: Some(remaining_content.len() as u64),
                supports_range: true,
            },
        );

        let result = download_file_with_client(
            &mock,
            url,
            &file_path,
            existing_content.len() as u64, // Resume from byte 10
            || false,
            |_| {},
        )
        .await;

        assert!(
            result.is_ok(),
            "Resume download should succeed: {:?}",
            result
        );
        let download_result = result.unwrap();
        assert!(download_result.was_resumed);
        assert_eq!(
            download_result.bytes_written,
            (existing_content.len() + remaining_content.len()) as u64
        );

        // Verify file content (existing + new)
        let final_content = tokio::fs::read(&file_path)
            .await
            .expect("Failed to read file");
        let mut expected = existing_content.to_vec();
        expected.extend_from_slice(remaining_content);
        assert_eq!(final_content, expected);
    }

    #[tokio::test]
    async fn test_download_network_error() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("error_file.bin");
        let url = "https://example.com/error.bin";

        let mock = MockHttpClient::new();
        mock.add_error(url, "Connection refused: network error");

        let result = download_file_with_client(&mock, url, &file_path, 0, || false, |_| {}).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(
            error.contains("network error") || error.contains("Connection refused"),
            "Error should mention network issue: {}",
            error
        );
    }

    #[tokio::test]
    async fn test_download_http_error_status() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("http_error_file.bin");
        let url = "https://example.com/not_found.bin";

        let mock = MockHttpClient::new();
        mock.add_response(
            url,
            HttpResponse {
                status: 404,
                body: b"Not Found".to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let result = download_file_with_client(&mock, url, &file_path, 0, || false, |_| {}).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(
            error.contains("404") || error.contains("HTTP error"),
            "Error should mention HTTP status: {}",
            error
        );
    }

    #[tokio::test]
    async fn test_download_cancellation() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("cancelled_file.bin");
        let url = "https://example.com/cancel_me.bin";

        // Create a large response to ensure cancellation check happens during write
        let large_content = vec![0u8; 200_000]; // 200KB

        let mock = MockHttpClient::new();
        mock.add_response(
            url,
            HttpResponse {
                status: 200,
                body: large_content,
                content_length: Some(200_000),
                supports_range: false,
            },
        );

        // Cancellation flag that gets set after some progress
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_clone = cancelled.clone();
        let progress_count = Arc::new(AtomicU64::new(0));
        let progress_count_clone = progress_count.clone();

        let result = download_file_with_client(
            &mock,
            url,
            &file_path,
            0,
            move || {
                // Cancel after first progress update
                if progress_count_clone.load(Ordering::SeqCst) > 0 {
                    return true;
                }
                cancelled_clone.load(Ordering::SeqCst)
            },
            move |bytes| {
                progress_count.fetch_add(1, Ordering::SeqCst);
                if bytes > 65536 {
                    cancelled.store(true, Ordering::SeqCst);
                }
            },
        )
        .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error, "cancelled", "Should return cancelled error");
    }

    #[tokio::test]
    async fn test_download_creates_parent_directories() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let nested_path = temp_dir
            .path()
            .join("deep")
            .join("nested")
            .join("dir")
            .join("file.bin");
        let url = "https://example.com/nested.bin";
        let content = b"nested content";

        let mock = MockHttpClient::new();
        mock.add_response(
            url,
            HttpResponse {
                status: 200,
                body: content.to_vec(),
                content_length: Some(content.len() as u64),
                supports_range: false,
            },
        );

        let result = download_file_with_client(&mock, url, &nested_path, 0, || false, |_| {}).await;

        assert!(result.is_ok(), "Should create directories: {:?}", result);
        assert!(nested_path.exists(), "File should exist at nested path");

        let written = tokio::fs::read(&nested_path).await.expect("Failed to read");
        assert_eq!(written, content);
    }

    #[tokio::test]
    async fn test_download_immediate_cancellation() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("immediate_cancel.bin");
        let url = "https://example.com/immediate.bin";

        let mock = MockHttpClient::new();
        mock.add_response(
            url,
            HttpResponse {
                status: 200,
                body: b"content".to_vec(),
                content_length: Some(7),
                supports_range: false,
            },
        );

        // Already cancelled before download starts
        let result = download_file_with_client(&mock, url, &file_path, 0, || true, |_| {}).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "cancelled");
    }

    #[tokio::test]
    async fn test_download_empty_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("empty.bin");
        let url = "https://example.com/empty.bin";

        let mock = MockHttpClient::new();
        mock.add_response(
            url,
            HttpResponse {
                status: 200,
                body: vec![],
                content_length: Some(0),
                supports_range: false,
            },
        );

        let result = download_file_with_client(&mock, url, &file_path, 0, || false, |_| {}).await;

        assert!(result.is_ok());
        let download_result = result.unwrap();
        assert_eq!(download_result.bytes_written, 0);

        let content = tokio::fs::read(&file_path).await.expect("Failed to read");
        assert!(content.is_empty());
    }

    #[tokio::test]
    async fn test_download_resume_no_range_support() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("no_range_file.bin");
        let url = "https://example.com/no_range.bin";

        // Simulate existing partial download
        let existing_content = b"partial";
        tokio::fs::write(&file_path, existing_content)
            .await
            .expect("Failed to write existing content");

        let full_content = b"full file content";

        let mock = MockHttpClient::new();
        // Server returns 200 (not 206), indicating no range support
        mock.add_response(
            url,
            HttpResponse {
                status: 200,
                body: full_content.to_vec(),
                content_length: Some(full_content.len() as u64),
                supports_range: false, // Server doesn't support ranges
            },
        );

        let result = download_file_with_client(
            &mock,
            url,
            &file_path,
            existing_content.len() as u64, // Try to resume
            || false,
            |_| {},
        )
        .await;

        assert!(
            result.is_ok(),
            "Should fall back to full download: {:?}",
            result
        );
        let download_result = result.unwrap();
        // When server doesn't support range, we start fresh
        assert!(!download_result.was_resumed);
        assert_eq!(download_result.bytes_written, full_content.len() as u64);

        // File should contain full content (not appended)
        let final_content = tokio::fs::read(&file_path)
            .await
            .expect("Failed to read file");
        assert_eq!(final_content, full_content);
    }

    #[tokio::test]
    async fn test_download_progress_callback_called() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("progress_test.bin");
        let url = "https://example.com/progress.bin";

        // Create content larger than chunk size to ensure multiple progress callbacks
        let content = vec![0xABu8; 100_000]; // 100KB

        let mock = MockHttpClient::new();
        mock.add_response(
            url,
            HttpResponse {
                status: 200,
                body: content.clone(),
                content_length: Some(content.len() as u64),
                supports_range: false,
            },
        );

        let progress_updates = Arc::new(std::sync::Mutex::new(Vec::new()));
        let progress_updates_clone = progress_updates.clone();

        let result = download_file_with_client(
            &mock,
            url,
            &file_path,
            0,
            || false,
            move |bytes| {
                progress_updates_clone.lock().unwrap().push(bytes);
            },
        )
        .await;

        assert!(result.is_ok());

        let updates = progress_updates.lock().unwrap();
        // Should have multiple progress updates (at least 2 for 100KB with 64KB chunks)
        assert!(
            updates.len() >= 2,
            "Should have multiple progress updates, got {}",
            updates.len()
        );

        // Progress should be monotonically increasing
        for window in updates.windows(2) {
            assert!(
                window[0] <= window[1],
                "Progress should increase: {} -> {}",
                window[0],
                window[1]
            );
        }

        // Final progress should equal total size
        assert_eq!(*updates.last().unwrap(), content.len() as u64);
    }

    #[test]
    fn test_correct_file_url_with_no_files_segment() {
        let url = "https://example.com/data/game.pak";
        let corrected = correct_file_url(url);
        assert_eq!(corrected, url); // Should remain unchanged
    }

    #[test]
    fn test_correct_file_url_with_files_at_end() {
        let url = "https://example.com/files/";
        let corrected = correct_file_url(url);
        assert_eq!(corrected, "https://example.com/");
    }

    #[test]
    fn test_compute_initial_downloaded_override_greater_than_existing() {
        let files = vec![FileInfo {
            path: "a".to_string(),
            hash: "h".to_string(),
            size: 100,
            url: "u".to_string(),
            existing_size: 30,
        }];
        // Override 80 is greater than existing (30), so use override
        assert_eq!(compute_initial_downloaded(&files, Some(80)), 80);
    }

    #[test]
    fn test_emit_download_progress_with_base_downloaded() {
        let emitter = MockEventEmitter::new();
        let params = DownloadProgressParams {
            current_file_name: "file.bin".to_string(),
            downloaded_bytes: 1500,
            total_bytes: 2000,
            base_downloaded: 500, // Previously downloaded
            total_files: 1,
            elapsed_time: Duration::from_secs(10),
            current_file_index: 1,
        };

        emit_download_progress(&emitter, &params).unwrap();

        let events = emitter.events();
        let payload = &events[0].payload;

        // Speed should be (1500 - 500) / 10 = 100 bytes/sec
        assert!(payload.contains("\"speed\":100"));
        assert!(payload.contains("\"base_downloaded\":500"));
    }

    #[tokio::test]
    async fn test_download_resume_with_failed_range_probe() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("resume_fail.bin");
        let url = "https://example.com/resume_fail.bin";

        // Write existing partial content
        let existing_content = b"partial";
        tokio::fs::write(&file_path, existing_content)
            .await
            .expect("Failed to write existing content");

        let full_content = b"full new content";

        let mock = MockHttpClient::new();
        // Range probe returns 200 (not 206), no range support
        mock.add_response(
            url,
            HttpResponse {
                status: 200,
                body: full_content.to_vec(),
                content_length: Some(full_content.len() as u64),
                supports_range: false,
            },
        );

        let result = download_file_with_client(
            &mock,
            url,
            &file_path,
            existing_content.len() as u64,
            || false,
            |_| {},
        )
        .await;

        assert!(result.is_ok());
        let download_result = result.unwrap();
        // Should start fresh when server doesn't support range
        assert!(!download_result.was_resumed);
    }

    #[tokio::test]
    async fn test_download_with_parent_directory_creation_failure() {
        // This tests that parent directories are created successfully
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let deep_path = temp_dir
            .path()
            .join("a")
            .join("b")
            .join("c")
            .join("d")
            .join("file.bin");
        let url = "https://example.com/deep.bin";
        let content = b"deep content";

        let mock = MockHttpClient::new();
        mock.add_response(
            url,
            HttpResponse {
                status: 200,
                body: content.to_vec(),
                content_length: Some(content.len() as u64),
                supports_range: false,
            },
        );

        let result = download_file_with_client(&mock, url, &deep_path, 0, || false, |_| {}).await;

        assert!(result.is_ok());
        assert!(deep_path.exists());
    }

    #[tokio::test]
    async fn download_file_with_client_range_request_returns_error() {
        // Test line 646: HTTP error when range request returns error status
        // The probe at line 636 will return 500 with supports_range=true, passing the
        // supports_range check at line 637. Then the actual range request at line 641
        // returns the same 500 error, triggering the error check at line 645.
        let mock = MockHttpClient::new();

        // Range request returns 500 error but supports_range is true
        // This simulates a server that supports ranges but returns an error
        mock.add_range_response(
            "http://example.com/file.pak",
            HttpResponse {
                status: 500,
                body: b"Internal Server Error".to_vec(),
                content_length: None,
                supports_range: true, // This makes supports_range check pass
            },
        );

        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test.pak");

        let result = download_file_with_client(
            &mock,
            "http://example.com/file.pak",
            &path,
            100, // resume_from > 0 to trigger range path
            || false,
            |_| {},
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HTTP error: 500"));
    }

    #[tokio::test]
    async fn download_file_with_client_no_range_support_then_get_error() {
        // Test line 655: HTTP error when server doesn't support range
        let mock = MockHttpClient::new();

        // Probe returns 200 (no range support) - uses get_range internally
        mock.add_range_response(
            "http://example.com/file.pak",
            HttpResponse {
                status: 200,
                body: vec![],
                content_length: Some(1000),
                supports_range: false,
            },
        );

        // Regular GET returns 503 error
        mock.add_response(
            "http://example.com/file.pak",
            HttpResponse {
                status: 503,
                body: b"Service Unavailable".to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test.pak");

        let result = download_file_with_client(
            &mock,
            "http://example.com/file.pak",
            &path,
            100, // resume_from > 0 to trigger range path
            || false,
            |_| {},
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HTTP error: 503"));
    }

    // =========================================================================
    // Tests for download_with_resilience
    // =========================================================================

    #[tokio::test]
    async fn test_download_with_resilience_success_first_try() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("success.bin");
        let url = "https://example.com/success.bin";
        let content = b"successful download content";

        let mock = MockHttpClient::new();
        mock.add_response(
            url,
            HttpResponse {
                status: 200,
                body: content.to_vec(),
                content_length: Some(content.len() as u64),
                supports_range: false,
            },
        );

        let policy = RetryPolicy::default();
        let result =
            download_with_resilience(&mock, url, &file_path, &policy, || false, |_| {}).await;

        assert!(result.is_ok());
        let download_result = result.unwrap();
        assert_eq!(download_result.bytes_written, content.len() as u64);

        // Verify file content
        let written = tokio::fs::read(&file_path)
            .await
            .expect("Failed to read file");
        assert_eq!(written, content);
    }

    #[tokio::test]
    async fn test_download_with_resilience_retries_transient() {
        // Test that transient errors are retried and eventually succeed
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("transient.bin");
        let url = "https://example.com/transient.bin";
        let content = b"content after retry";

        // This mock doesn't support multiple sequential responses,
        // so we test with a successful response that verifies retry logic doesn't break success
        let mock = MockHttpClient::new();
        mock.add_response(
            url,
            HttpResponse {
                status: 200,
                body: content.to_vec(),
                content_length: Some(content.len() as u64),
                supports_range: false,
            },
        );

        let policy = RetryPolicy::default();
        let result =
            download_with_resilience(&mock, url, &file_path, &policy, || false, |_| {}).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_download_with_resilience_fails_permanent() {
        // Test that 404 returns immediately without retrying
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("not_found.bin");
        let url = "https://example.com/not_found.bin";

        let mock = MockHttpClient::new();
        mock.add_response(
            url,
            HttpResponse {
                status: 404,
                body: b"Not Found".to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let policy = RetryPolicy::default();
        let result =
            download_with_resilience(&mock, url, &file_path, &policy, || false, |_| {}).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        // Should be HTTP error 404
        match err {
            DownloadError::Http { status, .. } => assert_eq!(status, 404),
            _ => panic!("Expected HTTP 404 error, got: {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_download_with_resilience_server_unreachable() {
        // Test that connection refused gives ServerUnreachable after limited retries
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("unreachable.bin");
        let url = "https://example.com/unreachable.bin";

        let mock = MockHttpClient::new();
        mock.add_error(url, "connection refused");

        let policy = RetryPolicy {
            max_retries: 5,
            base_delay_ms: 1, // Very short delay for testing
            max_delay_ms: 10,
            retry_stream_errors: true,
        };

        let result =
            download_with_resilience(&mock, url, &file_path, &policy, || false, |_| {}).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        // Should be ServerUnreachable after limited retries
        match err {
            DownloadError::ServerUnreachable { attempts, .. } => {
                // Should fail after 3 attempts (0, 1, 2 - where 2 is the limit)
                assert!(
                    attempts <= 3,
                    "Expected at most 3 attempts, got {}",
                    attempts
                );
            }
            _ => panic!("Expected ServerUnreachable error, got: {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_download_with_resilience_cancelled() {
        // Test that cancellation returns immediately
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("cancelled.bin");
        let url = "https://example.com/cancelled.bin";

        let mock = MockHttpClient::new();
        mock.add_response(
            url,
            HttpResponse {
                status: 200,
                body: b"content".to_vec(),
                content_length: Some(7),
                supports_range: false,
            },
        );

        let policy = RetryPolicy::default();

        // Already cancelled
        let result = download_with_resilience(
            &mock,
            url,
            &file_path,
            &policy,
            || true, // Always cancelled
            |_| {},
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, DownloadError::Cancelled));
    }

    #[tokio::test]
    async fn test_download_with_resilience_respects_policy() {
        // Test with custom retry policy
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("policy.bin");
        let url = "https://example.com/policy.bin";

        let mock = MockHttpClient::new();
        mock.add_error(url, "timeout"); // Transient error

        let policy = RetryPolicy {
            max_retries: 2, // Only 2 retries
            base_delay_ms: 1,
            max_delay_ms: 10,
            retry_stream_errors: true,
        };

        let result =
            download_with_resilience(&mock, url, &file_path, &policy, || false, |_| {}).await;

        // Should fail after max_retries
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_download_with_resilience_progress_callback() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("progress.bin");
        let url = "https://example.com/progress.bin";
        let content = vec![0u8; 100_000]; // 100KB to ensure progress callbacks

        let mock = MockHttpClient::new();
        mock.add_response(
            url,
            HttpResponse {
                status: 200,
                body: content.clone(),
                content_length: Some(content.len() as u64),
                supports_range: false,
            },
        );

        let policy = RetryPolicy::default();
        let progress_count = Arc::new(AtomicU64::new(0));
        let progress_count_clone = progress_count.clone();

        let result = download_with_resilience(
            &mock,
            url,
            &file_path,
            &policy,
            || false,
            move |_bytes| {
                progress_count_clone.fetch_add(1, Ordering::SeqCst);
            },
        )
        .await;

        assert!(result.is_ok());
        // Should have received progress callbacks
        assert!(
            progress_count.load(Ordering::SeqCst) > 0,
            "Expected progress callbacks"
        );
    }

    // =========================================================================
    // Tests for is_fatal_download_error
    // =========================================================================

    #[test]
    fn test_is_fatal_cancelled() {
        assert!(is_fatal_download_error(&DownloadError::Cancelled));
    }

    #[test]
    fn test_is_fatal_server_unreachable() {
        let err = DownloadError::server_unreachable(3, "connection refused");
        assert!(is_fatal_download_error(&err));
    }

    #[test]
    fn test_is_fatal_http_4xx() {
        let err = DownloadError::http(404, "Not Found");
        assert!(is_fatal_download_error(&err));

        let err = DownloadError::http(403, "Forbidden");
        assert!(is_fatal_download_error(&err));

        let err = DownloadError::http(401, "Unauthorized");
        assert!(is_fatal_download_error(&err));
    }

    #[test]
    fn test_is_fatal_http_5xx_not_fatal() {
        let err = DownloadError::http(500, "Internal Server Error");
        assert!(!is_fatal_download_error(&err));

        let err = DownloadError::http(503, "Service Unavailable");
        assert!(!is_fatal_download_error(&err));
    }

    #[test]
    fn test_hash_mismatch_not_fatal() {
        // Hash mismatch = corruption during download, should retry (delete + redownload)
        let err = DownloadError::HashMismatch {
            expected: "abc".to_string(),
            actual: "def".to_string(),
        };
        assert!(!is_fatal_download_error(&err));
    }

    #[test]
    fn test_is_fatal_filesystem() {
        let err = DownloadError::FileSystem("disk full".to_string());
        assert!(is_fatal_download_error(&err));
    }

    #[test]
    fn test_is_fatal_network_not_fatal() {
        let err = DownloadError::Network("timeout".to_string());
        assert!(!is_fatal_download_error(&err));
    }

    #[test]
    fn test_is_fatal_stream_interrupted_not_fatal() {
        let err = DownloadError::StreamInterrupted {
            bytes_received: 1024,
            error: "connection reset".to_string(),
        };
        assert!(!is_fatal_download_error(&err));
    }

    // =========================================================================
    // Tests for classify_error integration
    // =========================================================================

    #[test]
    fn test_classify_error_integration_transient() {
        assert_eq!(
            classify_error("connection timeout"),
            ErrorClassification::Transient
        );
        assert_eq!(
            classify_error("HTTP 500 internal error"),
            ErrorClassification::Transient
        );
        assert_eq!(
            classify_error("stream interrupted"),
            ErrorClassification::Transient
        );
    }

    #[test]
    fn test_classify_error_integration_permanent() {
        assert_eq!(
            classify_error("404 not found"),
            ErrorClassification::Permanent
        );
        // Note: hash mismatch is now Transient (corruption, should retry)
        // See test_classify_error_integration_transient for hash mismatch test
    }

    #[test]
    fn test_classify_error_integration_hash_mismatch_is_transient() {
        // Hash mismatch = file corruption during download, should delete and retry
        assert_eq!(
            classify_error("hash mismatch"),
            ErrorClassification::Transient
        );
    }

    #[test]
    fn test_classify_error_integration_cancelled() {
        assert_eq!(classify_error("cancelled"), ErrorClassification::Cancelled);
    }

    #[test]
    fn test_classify_error_integration_unreachable() {
        assert_eq!(
            classify_error("DNS resolution failed"),
            ErrorClassification::ServerUnreachable
        );
        assert_eq!(
            classify_error("connection refused"),
            ErrorClassification::ServerUnreachable
        );
    }
}
