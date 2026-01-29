//! Download service for file download orchestration.
//!
//! This module provides pure functions for download management:
//! - Download progress calculation
//! - URL correction
//! - Batch download orchestration logic

#![allow(dead_code)]

use crate::domain::{FileInfo, MAX_RETRIES, MAX_RETRY_DELAY_MS};
use crate::utils::resume_offset;

/// Calculates the initial downloaded byte count for resuming downloads.
///
/// Sums up the resume offsets for all files, optionally using a stored
/// override value if it's greater than the calculated sum. Clamps the
/// result to not exceed total size.
///
/// # Arguments
/// * `files` - Slice of files to download
/// * `resume_override` - Optional stored progress value
///
/// # Returns
/// The number of bytes already downloaded
///
/// # Examples
/// ```ignore
/// let files = vec![
///     FileInfo { size: 100, existing_size: 50, .. },
///     FileInfo { size: 200, existing_size: 100, .. },
/// ];
/// let downloaded = compute_initial_downloaded(&files, None);
/// assert_eq!(downloaded, 150); // 50 + 100
/// ```
pub fn compute_initial_downloaded(files: &[FileInfo], resume_override: Option<u64>) -> u64 {
    let sum_existing: u64 = files
        .iter()
        .map(|f| resume_offset(f.existing_size, f.size))
        .sum();

    let total_size: u64 = files.iter().map(|f| f.size).sum();

    let mut base = sum_existing;
    if let Some(override_bytes) = resume_override {
        if override_bytes > base {
            base = override_bytes;
        }
    }

    // Clamp to total size if positive
    if total_size > 0 && base > total_size {
        total_size
    } else {
        base
    }
}

/// Calculates download progress as a percentage.
///
/// # Arguments
/// * `downloaded` - Bytes downloaded so far
/// * `total` - Total bytes to download
///
/// # Returns
/// Progress percentage (0.0 to 100.0)
///
/// # Examples
/// ```ignore
/// assert_eq!(calculate_progress(50, 100), 50.0);
/// assert_eq!(calculate_progress(0, 100), 0.0);
/// assert_eq!(calculate_progress(100, 100), 100.0);
/// assert_eq!(calculate_progress(100, 0), 0.0); // Handle zero total
/// ```
pub fn calculate_progress(downloaded: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (downloaded as f64 / total as f64) * 100.0
    }
}

/// Calculates download speed in bytes per second.
///
/// # Arguments
/// * `bytes_downloaded` - Bytes downloaded in this session
/// * `elapsed_secs` - Seconds elapsed since download started
///
/// # Returns
/// Speed in bytes per second
///
/// # Examples
/// ```ignore
/// assert_eq!(calculate_speed(1024, 1), 1024);
/// assert_eq!(calculate_speed(5000, 5), 1000);
/// assert_eq!(calculate_speed(1000, 0), 0); // Handle zero time
/// ```
pub fn calculate_speed(bytes_downloaded: u64, elapsed_secs: u64) -> u64 {
    if elapsed_secs == 0 {
        0
    } else {
        bytes_downloaded / elapsed_secs
    }
}

/// Calculates estimated time remaining for download.
///
/// # Arguments
/// * `bytes_remaining` - Bytes left to download
/// * `speed` - Current download speed in bytes per second
///
/// # Returns
/// Estimated seconds remaining
///
/// # Examples
/// ```ignore
/// assert_eq!(calculate_eta(1000, 100), 10);
/// assert_eq!(calculate_eta(1000, 0), u64::MAX); // Infinite if no speed
/// assert_eq!(calculate_eta(0, 100), 0); // Done
/// ```
pub fn calculate_eta(bytes_remaining: u64, speed: u64) -> u64 {
    if speed == 0 {
        if bytes_remaining == 0 {
            0
        } else {
            u64::MAX // Infinite/unknown
        }
    } else {
        bytes_remaining / speed
    }
}

/// Corrects a download URL by removing duplicate "/files/" segments.
///
/// The server sometimes returns URLs with double path segments that need
/// to be normalized.
///
/// # Arguments
/// * `url` - The URL to correct
///
/// # Returns
/// The corrected URL
///
/// # Examples
/// ```ignore
/// let url = "https://server.com/files/data/file.pak";
/// let corrected = correct_download_url(url);
/// assert_eq!(corrected, "https://server.com/data/file.pak");
/// ```
pub fn correct_download_url(url: &str) -> String {
    if let Some(pos) = url.find("/files/") {
        format!("{}/{}", &url[..pos], &url[(pos + 7)..])
    } else {
        url.to_string()
    }
}

/// Result of planning a download batch.
#[derive(Debug, Clone, PartialEq)]
pub struct DownloadPlan {
    /// Total number of files to download
    pub total_files: usize,
    /// Total bytes to download
    pub total_bytes: u64,
    /// Bytes already downloaded (resumable)
    pub initial_downloaded: u64,
    /// Bytes remaining to download
    pub bytes_remaining: u64,
}

/// Plans a download batch from a list of files.
///
/// # Arguments
/// * `files` - Files to download
/// * `resume_override` - Optional stored progress value
///
/// # Returns
/// Download plan with statistics
pub fn plan_download(files: &[FileInfo], resume_override: Option<u64>) -> DownloadPlan {
    let total_files = files.len();
    let total_bytes: u64 = files.iter().map(|f| f.size).sum();
    let initial_downloaded = compute_initial_downloaded(files, resume_override);
    let bytes_remaining = total_bytes.saturating_sub(initial_downloaded);

    DownloadPlan {
        total_files,
        total_bytes,
        initial_downloaded,
        bytes_remaining,
    }
}

/// Download progress snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct DownloadProgress {
    /// Current file being downloaded
    pub current_file: String,
    /// Progress percentage (0-100)
    pub progress: f64,
    /// Speed in bytes per second
    pub speed: u64,
    /// Total bytes downloaded
    pub downloaded_bytes: u64,
    /// Total bytes to download
    pub total_bytes: u64,
    /// Elapsed time in seconds
    pub elapsed_secs: f64,
    /// Estimated time remaining in seconds
    pub eta_secs: u64,
}

/// Creates a progress snapshot from current state.
///
/// # Arguments
/// * `current_file` - Name of current file being downloaded
/// * `downloaded` - Total bytes downloaded so far
/// * `total` - Total bytes to download
/// * `base_downloaded` - Initial downloaded bytes (for speed calculation)
/// * `elapsed_secs` - Seconds since download started
///
/// # Returns
/// Progress snapshot
pub fn create_progress_snapshot(
    current_file: String,
    downloaded: u64,
    total: u64,
    base_downloaded: u64,
    elapsed_secs: f64,
) -> DownloadProgress {
    let progress = calculate_progress(downloaded, total);
    let session_downloaded = downloaded.saturating_sub(base_downloaded);
    let speed = if elapsed_secs > 0.0 {
        (session_downloaded as f64 / elapsed_secs) as u64
    } else {
        0
    };
    let bytes_remaining = total.saturating_sub(downloaded);
    let eta_secs = calculate_eta(bytes_remaining, speed);

    DownloadProgress {
        current_file,
        progress,
        speed,
        downloaded_bytes: downloaded,
        total_bytes: total,
        elapsed_secs,
        eta_secs,
    }
}

/// Determines if a file should skip the existing content check.
///
/// Returns true if the file doesn't exist at the expected path,
/// meaning any resume offset should be ignored.
///
/// # Arguments
/// * `resume_from` - Calculated resume offset
/// * `file_exists` - Whether the file exists on disk
///
/// # Returns
/// The adjusted resume offset (0 if file doesn't exist)
pub fn adjust_resume_offset(resume_from: u64, file_exists: bool) -> u64 {
    if resume_from > 0 && !file_exists {
        0
    } else {
        resume_from
    }
}

/// Checks if parallel chunk download is beneficial for a file.
///
/// Parallel downloads are only beneficial for large files where the
/// overhead of multiple connections is justified.
///
/// # Arguments
/// * `file_size` - Size of the file in bytes
/// * `chunk_min_size` - Minimum size for parallel downloads
/// * `allow_parallel` - Whether parallel downloads are enabled
///
/// # Returns
/// `true` if parallel download should be used
pub fn should_use_parallel_download(
    file_size: u64,
    chunk_min_size: u64,
    allow_parallel: bool,
) -> bool {
    allow_parallel && file_size >= chunk_min_size
}

/// Calculates chunk boundaries for parallel download.
///
/// # Arguments
/// * `file_size` - Total file size
/// * `part_size` - Size of each part
/// * `max_parts` - Maximum number of parts
///
/// # Returns
/// Vector of (start, end) byte ranges
pub fn calculate_chunk_ranges(file_size: u64, part_size: u64, max_parts: usize) -> Vec<(u64, u64)> {
    if file_size == 0 || part_size == 0 {
        return vec![];
    }

    let num_parts = std::cmp::max(
        1,
        std::cmp::min(max_parts as u64, file_size.div_ceil(part_size)) as usize,
    );

    let mut ranges = Vec::with_capacity(num_parts);
    for part_idx in 0..num_parts {
        let start = (part_idx as u64) * part_size;
        let mut end = ((part_idx as u64 + 1) * part_size).saturating_sub(1);
        if end >= file_size {
            end = file_size - 1;
        }
        if start <= end {
            ranges.push((start, end));
        }
    }

    ranges
}

/// Configuration for download retry behavior.
#[derive(Debug, Clone, PartialEq)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_retries: u8,
    /// Base delay in milliseconds for exponential backoff
    pub base_delay_ms: u64,
    /// Maximum delay cap in milliseconds
    pub max_delay_ms: u64,
    /// Whether to retry on stream interruptions
    pub retry_stream_errors: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: MAX_RETRIES,
            base_delay_ms: 500,
            max_delay_ms: MAX_RETRY_DELAY_MS,
            retry_stream_errors: true,
        }
    }
}

impl RetryPolicy {
    /// Create a policy for aggressive retrying (more attempts, shorter delays)
    pub fn aggressive() -> Self {
        Self {
            max_retries: 8,
            base_delay_ms: 250,
            max_delay_ms: 15_000,
            retry_stream_errors: true,
        }
    }

    /// Create a conservative policy (fewer attempts, longer delays)
    pub fn conservative() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 60_000,
            retry_stream_errors: true,
        }
    }

    /// Calculate delay for a given attempt using this policy
    pub fn delay_for_attempt(&self, attempt: u8) -> u64 {
        let delay = self.base_delay_ms.saturating_mul(2u64.pow(attempt as u32));
        std::cmp::min(delay, self.max_delay_ms)
    }

    /// Check if we should retry given the attempt count
    pub fn should_retry(&self, attempt: u8) -> bool {
        attempt < self.max_retries
    }
}

/// Result of classifying a download error for retry decisions.
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorClassification {
    /// Transient error that should be retried (network glitch, server overload)
    Transient,
    /// Permanent error that should not be retried (404, 401, 403)
    Permanent,
    /// Server appears unreachable (DNS failure, connection refused)
    ServerUnreachable,
    /// User cancelled the operation
    Cancelled,
}

/// Classifies an error message for retry decision making.
///
/// # Arguments
/// * `error_msg` - The error message to classify
///
/// # Returns
/// Classification indicating whether to retry
pub fn classify_error(error_msg: &str) -> ErrorClassification {
    let msg = error_msg.to_lowercase();

    // Check for cancellation first
    if msg == "cancelled" || msg.contains("cancelled by user") {
        return ErrorClassification::Cancelled;
    }

    // Check for server unreachable patterns
    // Network-level "not found" errors are connectivity issues, not permanent
    if msg.contains("dns")
        || msg.contains("no route")
        || msg.contains("network unreachable")
        || msg.contains("host unreachable")
        || msg.contains("name resolution")
        || msg.contains("host not found")
        || msg.contains("route not found")
        || msg.contains("interface not found")
        || msg.contains("address not found")
        || (msg.contains("connection refused") && !msg.contains("temporarily"))
    {
        return ErrorClassification::ServerUnreachable;
    }

    // Check for specific transient HTTP status codes
    if msg.contains("408") || msg.contains("request timeout") {
        return ErrorClassification::Transient;
    }
    if msg.contains("416") || msg.contains("range not satisfiable") {
        return ErrorClassification::Transient;
    }

    // Check for permanent errors (4xx except 408, 416, 429)
    // NOTE: Hash mismatch is NOT permanent - it means corruption during download,
    // so we should delete the file and retry the download from scratch
    // HTTP 404 is permanent, but only if it looks like an HTTP error
    if msg.contains("404")
        || (msg.contains("not found") && !msg.contains("host") && !msg.contains("route"))
        || msg.contains("401")
        || msg.contains("unauthorized")
        || msg.contains("403")
        || msg.contains("forbidden")
        || msg.contains("invalid url")
        || msg.contains("invalid request")
        || msg.contains("invalid path")
    {
        return ErrorClassification::Permanent;
    }

    // Hash mismatch = corruption during download, should retry (delete + redownload)
    if msg.contains("hash mismatch") || msg.contains("checksum") {
        return ErrorClassification::Transient;
    }

    // Check for transient errors
    if msg.contains("timeout")
        || msg.contains("timed out")
        || msg.contains("connection reset")
        || msg.contains("connection closed")
        || msg.contains("broken pipe")
        || msg.contains("temporarily")
        || msg.contains("network")
        || msg.contains("500")
        || msg.contains("502")
        || msg.contains("503")
        || msg.contains("504")
        || msg.contains("429")
        || msg.contains("too many")
        || msg.contains("eof")
        || msg.contains("incomplete")
        || msg.contains("aborted")
        || msg.contains("reset")
        || msg.contains("interrupted")
    {
        return ErrorClassification::Transient;
    }

    // Default to transient (optimistic - retry unknown errors)
    ErrorClassification::Transient
}

/// Determines if a download should be retried based on error and attempt count.
///
/// # Arguments
/// * `error_msg` - The error message
/// * `attempt` - Current attempt number (0-indexed)
/// * `policy` - Retry policy to use
///
/// # Returns
/// `true` if the download should be retried
pub fn should_retry_download(error_msg: &str, attempt: u8, policy: &RetryPolicy) -> bool {
    if attempt >= policy.max_retries {
        return false;
    }

    match classify_error(error_msg) {
        ErrorClassification::Transient => true,
        ErrorClassification::ServerUnreachable => attempt < 2, // Only retry unreachable twice
        ErrorClassification::Permanent | ErrorClassification::Cancelled => false,
    }
}

/// Calculates an adaptive read timeout based on file size and estimated speed.
///
/// For large files on slow connections, the default timeout may not be enough.
/// This calculates a reasonable timeout that accounts for transfer time.
///
/// # Arguments
/// * `file_size` - Size of the file in bytes
/// * `estimated_speed_bps` - Estimated download speed in bytes per second
/// * `min_timeout_secs` - Minimum timeout to return
///
/// # Returns
/// Recommended timeout in seconds
pub fn calculate_adaptive_timeout(
    file_size: u64,
    estimated_speed_bps: u64,
    min_timeout_secs: u64,
) -> u64 {
    if estimated_speed_bps == 0 {
        return min_timeout_secs;
    }

    // Calculate time needed to download file at estimated speed
    let transfer_time_secs = file_size / estimated_speed_bps;

    // Add 50% buffer for variability, minimum of min_timeout
    let timeout = transfer_time_secs + (transfer_time_secs / 2);
    std::cmp::max(timeout, min_timeout_secs)
}

/// Estimates download speed from historical data.
///
/// # Arguments
/// * `bytes_downloaded` - Total bytes downloaded in session
/// * `elapsed_secs` - Seconds elapsed
/// * `default_speed` - Default speed if no data available
///
/// # Returns
/// Estimated speed in bytes per second
pub fn estimate_speed(bytes_downloaded: u64, elapsed_secs: f64, default_speed: u64) -> u64 {
    if elapsed_secs < 1.0 || bytes_downloaded == 0 {
        return default_speed;
    }
    (bytes_downloaded as f64 / elapsed_secs) as u64
}

/// Tracks the health of an ongoing download for adaptive behavior.
#[derive(Debug, Clone)]
pub struct DownloadHealth {
    /// Number of successful chunks received
    pub successful_chunks: u64,
    /// Number of retried chunks
    pub retried_chunks: u64,
    /// Current streak of successful chunks
    pub success_streak: u64,
    /// Highest error count before recovery
    pub max_consecutive_errors: u8,
    /// Current consecutive error count
    pub consecutive_errors: u8,
}

impl Default for DownloadHealth {
    fn default() -> Self {
        Self {
            successful_chunks: 0,
            retried_chunks: 0,
            success_streak: 0,
            max_consecutive_errors: 0,
            consecutive_errors: 0,
        }
    }
}

impl DownloadHealth {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful chunk download
    pub fn record_success(&mut self) {
        self.successful_chunks += 1;
        self.success_streak += 1;
        self.consecutive_errors = 0;
    }

    /// Record a retried chunk (error followed by success)
    pub fn record_retry(&mut self) {
        self.retried_chunks += 1;
        self.consecutive_errors = self.consecutive_errors.saturating_add(1);
        self.max_consecutive_errors =
            std::cmp::max(self.max_consecutive_errors, self.consecutive_errors);
        self.success_streak = 0;
    }

    /// Calculate health score (0.0 = poor, 1.0 = excellent)
    pub fn health_score(&self) -> f64 {
        let total = self.successful_chunks + self.retried_chunks;
        if total == 0 {
            return 1.0;
        }
        self.successful_chunks as f64 / total as f64
    }

    /// Determine if download is healthy enough to continue
    pub fn is_healthy(&self) -> bool {
        // Consider unhealthy if more than 50% retries or 5+ consecutive errors
        self.health_score() > 0.5 && self.consecutive_errors < 5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_file_info(size: u64, existing_size: u64) -> FileInfo {
        FileInfo {
            path: "test.pak".to_string(),
            hash: "abc123".to_string(),
            size,
            url: "https://example.com/test.pak".to_string(),
            existing_size,
        }
    }

    // ========================================================================
    // Tests for compute_initial_downloaded
    // ========================================================================

    #[test]
    fn compute_initial_downloaded_empty() {
        let files: Vec<FileInfo> = vec![];
        assert_eq!(compute_initial_downloaded(&files, None), 0);
    }

    #[test]
    fn compute_initial_downloaded_no_existing() {
        let files = vec![make_file_info(100, 0), make_file_info(200, 0)];
        assert_eq!(compute_initial_downloaded(&files, None), 0);
    }

    #[test]
    fn compute_initial_downloaded_partial() {
        let files = vec![make_file_info(100, 50), make_file_info(200, 100)];
        assert_eq!(compute_initial_downloaded(&files, None), 150);
    }

    #[test]
    fn compute_initial_downloaded_with_override_higher() {
        let files = vec![make_file_info(100, 50)];
        assert_eq!(compute_initial_downloaded(&files, Some(75)), 75);
    }

    #[test]
    fn compute_initial_downloaded_with_override_lower() {
        let files = vec![make_file_info(100, 50)];
        assert_eq!(compute_initial_downloaded(&files, Some(25)), 50);
    }

    #[test]
    fn compute_initial_downloaded_clamps_to_total() {
        let files = vec![make_file_info(50, 0)];
        assert_eq!(compute_initial_downloaded(&files, Some(100)), 50);
    }

    #[test]
    fn compute_initial_downloaded_zero_size_files() {
        let files = vec![make_file_info(0, 0)];
        // With zero total, override is not clamped
        assert_eq!(compute_initial_downloaded(&files, Some(100)), 100);
    }

    // ========================================================================
    // Tests for calculate_progress
    // ========================================================================

    #[test]
    fn calculate_progress_zero() {
        assert_eq!(calculate_progress(0, 100), 0.0);
    }

    #[test]
    fn calculate_progress_half() {
        assert_eq!(calculate_progress(50, 100), 50.0);
    }

    #[test]
    fn calculate_progress_complete() {
        assert_eq!(calculate_progress(100, 100), 100.0);
    }

    #[test]
    fn calculate_progress_zero_total() {
        assert_eq!(calculate_progress(100, 0), 0.0);
    }

    #[test]
    fn calculate_progress_exceeds_total() {
        // Can happen with override values
        assert!(calculate_progress(150, 100) > 100.0);
    }

    #[test]
    fn calculate_progress_precision() {
        let progress = calculate_progress(1, 3);
        assert!((progress - 33.333333).abs() < 0.001);
    }

    // ========================================================================
    // Tests for calculate_speed
    // ========================================================================

    #[test]
    fn calculate_speed_basic() {
        assert_eq!(calculate_speed(1024, 1), 1024);
        assert_eq!(calculate_speed(5000, 5), 1000);
    }

    #[test]
    fn calculate_speed_zero_time() {
        assert_eq!(calculate_speed(1000, 0), 0);
    }

    #[test]
    fn calculate_speed_zero_bytes() {
        assert_eq!(calculate_speed(0, 10), 0);
    }

    #[test]
    fn calculate_speed_large_values() {
        let gb = 1024u64 * 1024 * 1024;
        assert_eq!(calculate_speed(gb, 100), gb / 100);
    }

    // ========================================================================
    // Tests for calculate_eta
    // ========================================================================

    #[test]
    fn calculate_eta_basic() {
        assert_eq!(calculate_eta(1000, 100), 10);
        assert_eq!(calculate_eta(5000, 500), 10);
    }

    #[test]
    fn calculate_eta_zero_speed() {
        assert_eq!(calculate_eta(1000, 0), u64::MAX);
    }

    #[test]
    fn calculate_eta_zero_remaining() {
        assert_eq!(calculate_eta(0, 100), 0);
        assert_eq!(calculate_eta(0, 0), 0);
    }

    #[test]
    fn calculate_eta_large_download() {
        let gb = 1024u64 * 1024 * 1024;
        let mb_per_sec = 1024 * 1024;
        assert_eq!(calculate_eta(gb, mb_per_sec), 1024);
    }

    // ========================================================================
    // Tests for correct_download_url
    // ========================================================================

    #[test]
    fn correct_download_url_with_files() {
        let url = "https://server.com/files/data/file.pak";
        assert_eq!(
            correct_download_url(url),
            "https://server.com/data/file.pak"
        );
    }

    #[test]
    fn correct_download_url_without_files() {
        let url = "https://server.com/data/file.pak";
        assert_eq!(correct_download_url(url), url);
    }

    #[test]
    fn correct_download_url_multiple_files() {
        // Only removes first occurrence
        let url = "https://server.com/files/files/file.pak";
        assert_eq!(
            correct_download_url(url),
            "https://server.com/files/file.pak"
        );
    }

    #[test]
    fn correct_download_url_empty() {
        assert_eq!(correct_download_url(""), "");
    }

    #[test]
    fn correct_download_url_files_only() {
        let url = "https://server.com/files/";
        assert_eq!(correct_download_url(url), "https://server.com/");
    }

    // ========================================================================
    // Tests for plan_download
    // ========================================================================

    #[test]
    fn plan_download_empty() {
        let plan = plan_download(&[], None);
        assert_eq!(plan.total_files, 0);
        assert_eq!(plan.total_bytes, 0);
        assert_eq!(plan.initial_downloaded, 0);
        assert_eq!(plan.bytes_remaining, 0);
    }

    #[test]
    fn plan_download_fresh() {
        let files = vec![make_file_info(100, 0), make_file_info(200, 0)];
        let plan = plan_download(&files, None);
        assert_eq!(plan.total_files, 2);
        assert_eq!(plan.total_bytes, 300);
        assert_eq!(plan.initial_downloaded, 0);
        assert_eq!(plan.bytes_remaining, 300);
    }

    #[test]
    fn plan_download_partial() {
        let files = vec![make_file_info(100, 50), make_file_info(200, 100)];
        let plan = plan_download(&files, None);
        assert_eq!(plan.total_files, 2);
        assert_eq!(plan.total_bytes, 300);
        assert_eq!(plan.initial_downloaded, 150);
        assert_eq!(plan.bytes_remaining, 150);
    }

    #[test]
    fn plan_download_with_override() {
        let files = vec![make_file_info(100, 50)];
        let plan = plan_download(&files, Some(75));
        assert_eq!(plan.initial_downloaded, 75);
        assert_eq!(plan.bytes_remaining, 25);
    }

    // ========================================================================
    // Tests for create_progress_snapshot
    // ========================================================================

    #[test]
    fn create_progress_snapshot_basic() {
        let snapshot = create_progress_snapshot("file.pak".to_string(), 50, 100, 0, 5.0);
        assert_eq!(snapshot.current_file, "file.pak");
        assert_eq!(snapshot.progress, 50.0);
        assert_eq!(snapshot.downloaded_bytes, 50);
        assert_eq!(snapshot.total_bytes, 100);
        assert_eq!(snapshot.speed, 10); // 50 bytes / 5 seconds
    }

    #[test]
    fn create_progress_snapshot_with_base() {
        let snapshot = create_progress_snapshot(
            "file.pak".to_string(),
            100,
            200,
            50, // base downloaded
            10.0,
        );
        // Speed should be calculated from session bytes only
        assert_eq!(snapshot.speed, 5); // (100-50) / 10
    }

    #[test]
    fn create_progress_snapshot_zero_time() {
        let snapshot = create_progress_snapshot("file.pak".to_string(), 50, 100, 0, 0.0);
        assert_eq!(snapshot.speed, 0);
        assert_eq!(snapshot.eta_secs, u64::MAX);
    }

    #[test]
    fn create_progress_snapshot_complete() {
        let snapshot = create_progress_snapshot("file.pak".to_string(), 100, 100, 0, 10.0);
        assert_eq!(snapshot.progress, 100.0);
        assert_eq!(snapshot.eta_secs, 0);
    }

    // ========================================================================
    // Tests for adjust_resume_offset
    // ========================================================================

    #[test]
    fn adjust_resume_offset_file_exists() {
        assert_eq!(adjust_resume_offset(100, true), 100);
    }

    #[test]
    fn adjust_resume_offset_file_missing() {
        assert_eq!(adjust_resume_offset(100, false), 0);
    }

    #[test]
    fn adjust_resume_offset_zero() {
        assert_eq!(adjust_resume_offset(0, true), 0);
        assert_eq!(adjust_resume_offset(0, false), 0);
    }

    // ========================================================================
    // Tests for should_use_parallel_download
    // ========================================================================

    #[test]
    fn should_use_parallel_large_file() {
        let chunk_min = 16 * 1024 * 1024; // 16 MB
        assert!(should_use_parallel_download(chunk_min, chunk_min, true));
        assert!(should_use_parallel_download(chunk_min + 1, chunk_min, true));
    }

    #[test]
    fn should_use_parallel_small_file() {
        let chunk_min = 16 * 1024 * 1024;
        assert!(!should_use_parallel_download(
            chunk_min - 1,
            chunk_min,
            true
        ));
        assert!(!should_use_parallel_download(1024, chunk_min, true));
    }

    #[test]
    fn should_use_parallel_disabled() {
        let chunk_min = 16 * 1024 * 1024;
        assert!(!should_use_parallel_download(
            chunk_min * 2,
            chunk_min,
            false
        ));
    }

    // ========================================================================
    // Tests for calculate_chunk_ranges
    // ========================================================================

    #[test]
    fn calculate_chunk_ranges_empty() {
        assert!(calculate_chunk_ranges(0, 1024, 4).is_empty());
        assert!(calculate_chunk_ranges(1024, 0, 4).is_empty());
    }

    #[test]
    fn calculate_chunk_ranges_single_chunk() {
        let ranges = calculate_chunk_ranges(100, 1000, 4);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], (0, 99));
    }

    #[test]
    fn calculate_chunk_ranges_multiple_chunks() {
        let ranges = calculate_chunk_ranges(100, 25, 10);
        assert_eq!(ranges.len(), 4);
        assert_eq!(ranges[0], (0, 24));
        assert_eq!(ranges[1], (25, 49));
        assert_eq!(ranges[2], (50, 74));
        assert_eq!(ranges[3], (75, 99));
    }

    #[test]
    fn calculate_chunk_ranges_capped_by_max_parts() {
        let ranges = calculate_chunk_ranges(1000, 100, 3);
        assert_eq!(ranges.len(), 3);
    }

    #[test]
    fn calculate_chunk_ranges_last_chunk_smaller() {
        let ranges = calculate_chunk_ranges(110, 50, 10);
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0], (0, 49));
        assert_eq!(ranges[1], (50, 99));
        assert_eq!(ranges[2], (100, 109)); // Last chunk is smaller
    }

    #[test]
    fn calculate_chunk_ranges_exact_fit() {
        let ranges = calculate_chunk_ranges(100, 25, 10);
        assert_eq!(ranges.len(), 4);
        // All chunks should be exactly 25 bytes (0-24, 25-49, 50-74, 75-99)
        for (i, (start, end)) in ranges.iter().enumerate() {
            assert_eq!(*start, (i as u64) * 25);
            assert_eq!(*end, (i as u64) * 25 + 24);
        }
    }

    // ========================================================================
    // Tests for RetryPolicy
    // ========================================================================

    #[test]
    fn retry_policy_default_values() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_retries, MAX_RETRIES);
        assert_eq!(policy.base_delay_ms, 500);
        assert_eq!(policy.max_delay_ms, MAX_RETRY_DELAY_MS);
        assert!(policy.retry_stream_errors);
    }

    #[test]
    fn retry_policy_aggressive() {
        let policy = RetryPolicy::aggressive();
        assert_eq!(policy.max_retries, 8);
        assert_eq!(policy.base_delay_ms, 250);
        assert_eq!(policy.max_delay_ms, 15_000);
        assert!(policy.retry_stream_errors);
    }

    #[test]
    fn retry_policy_conservative() {
        let policy = RetryPolicy::conservative();
        assert_eq!(policy.max_retries, 3);
        assert_eq!(policy.base_delay_ms, 1000);
        assert_eq!(policy.max_delay_ms, 60_000);
        assert!(policy.retry_stream_errors);
    }

    #[test]
    fn retry_policy_delay_exponential_growth() {
        let policy = RetryPolicy::default();
        // Base delay is 500ms, should double each attempt
        assert_eq!(policy.delay_for_attempt(0), 500);
        assert_eq!(policy.delay_for_attempt(1), 1000);
        assert_eq!(policy.delay_for_attempt(2), 2000);
        assert_eq!(policy.delay_for_attempt(3), 4000);
    }

    #[test]
    fn retry_policy_delay_caps_at_max() {
        let policy = RetryPolicy {
            max_retries: 10,
            base_delay_ms: 1000,
            max_delay_ms: 5000,
            retry_stream_errors: true,
        };
        // Should cap at max_delay_ms
        assert_eq!(policy.delay_for_attempt(10), 5000);
        assert_eq!(policy.delay_for_attempt(20), 5000);
    }

    #[test]
    fn retry_policy_should_retry_boundaries() {
        let policy = RetryPolicy {
            max_retries: 3,
            base_delay_ms: 500,
            max_delay_ms: 10_000,
            retry_stream_errors: true,
        };
        assert!(policy.should_retry(0));
        assert!(policy.should_retry(1));
        assert!(policy.should_retry(2));
        assert!(!policy.should_retry(3));
        assert!(!policy.should_retry(4));
    }

    // ========================================================================
    // Tests for classify_error
    // ========================================================================

    #[test]
    fn classify_error_cancelled() {
        assert_eq!(classify_error("cancelled"), ErrorClassification::Cancelled);
        assert_eq!(
            classify_error("Cancelled by user"),
            ErrorClassification::Cancelled
        );
    }

    #[test]
    fn classify_error_server_unreachable() {
        assert_eq!(
            classify_error("DNS lookup failed"),
            ErrorClassification::ServerUnreachable
        );
        assert_eq!(
            classify_error("no route to host"),
            ErrorClassification::ServerUnreachable
        );
        assert_eq!(
            classify_error("Network unreachable"),
            ErrorClassification::ServerUnreachable
        );
        assert_eq!(
            classify_error("host unreachable"),
            ErrorClassification::ServerUnreachable
        );
        assert_eq!(
            classify_error("name resolution failed"),
            ErrorClassification::ServerUnreachable
        );
        assert_eq!(
            classify_error("connection refused"),
            ErrorClassification::ServerUnreachable
        );
    }

    #[test]
    fn classify_error_permanent() {
        assert_eq!(
            classify_error("404 not found"),
            ErrorClassification::Permanent
        );
        assert_eq!(
            classify_error("File not found"),
            ErrorClassification::Permanent
        );
        assert_eq!(classify_error("401"), ErrorClassification::Permanent);
        assert_eq!(
            classify_error("Unauthorized"),
            ErrorClassification::Permanent
        );
        assert_eq!(
            classify_error("403 Forbidden"),
            ErrorClassification::Permanent
        );
        // Hash mismatch = corruption, should be transient (delete + retry)
        assert_eq!(
            classify_error("Hash mismatch detected"),
            ErrorClassification::Transient
        );
        assert_eq!(
            classify_error("Invalid signature"),
            ErrorClassification::Permanent
        );
    }

    #[test]
    fn classify_error_transient() {
        // Timeouts
        assert_eq!(
            classify_error("connection timeout"),
            ErrorClassification::Transient
        );
        assert_eq!(
            classify_error("Request timed out"),
            ErrorClassification::Transient
        );

        // Connection issues
        assert_eq!(
            classify_error("connection reset by peer"),
            ErrorClassification::Transient
        );
        assert_eq!(
            classify_error("connection closed"),
            ErrorClassification::Transient
        );
        assert_eq!(
            classify_error("broken pipe"),
            ErrorClassification::Transient
        );

        // Server errors
        assert_eq!(
            classify_error("500 internal server error"),
            ErrorClassification::Transient
        );
        assert_eq!(
            classify_error("502 bad gateway"),
            ErrorClassification::Transient
        );
        assert_eq!(
            classify_error("503 service unavailable"),
            ErrorClassification::Transient
        );
        assert_eq!(
            classify_error("504 gateway timeout"),
            ErrorClassification::Transient
        );

        // Rate limiting
        assert_eq!(
            classify_error("429 too many requests"),
            ErrorClassification::Transient
        );

        // Stream errors
        assert_eq!(
            classify_error("unexpected EOF"),
            ErrorClassification::Transient
        );
        assert_eq!(
            classify_error("incomplete read"),
            ErrorClassification::Transient
        );
        assert_eq!(
            classify_error("transfer aborted"),
            ErrorClassification::Transient
        );
        assert_eq!(
            classify_error("stream interrupted"),
            ErrorClassification::Transient
        );

        // Generic network
        assert_eq!(
            classify_error("network error"),
            ErrorClassification::Transient
        );
        assert_eq!(
            classify_error("temporarily unavailable"),
            ErrorClassification::Transient
        );
    }

    #[test]
    fn classify_error_unknown_defaults_transient() {
        // Unknown errors should default to transient (optimistic retry)
        assert_eq!(
            classify_error("some weird error"),
            ErrorClassification::Transient
        );
    }

    #[test]
    fn classify_error_case_insensitive() {
        assert_eq!(classify_error("TIMEOUT"), ErrorClassification::Transient);
        assert_eq!(classify_error("TimeOut"), ErrorClassification::Transient);
        assert_eq!(classify_error("CANCELLED"), ErrorClassification::Cancelled);
    }

    // ========================================================================
    // Tests for should_retry_download
    // ========================================================================

    #[test]
    fn should_retry_download_transient_errors() {
        let policy = RetryPolicy::default();
        assert!(should_retry_download("timeout", 0, &policy));
        assert!(should_retry_download("timeout", 1, &policy));
        assert!(should_retry_download("500 error", 2, &policy));
    }

    #[test]
    fn should_retry_download_permanent_errors() {
        let policy = RetryPolicy::default();
        assert!(!should_retry_download("404 not found", 0, &policy));
        assert!(!should_retry_download("401 unauthorized", 1, &policy));
    }

    #[test]
    fn should_retry_download_hash_mismatch() {
        // Hash mismatch = corruption during download, should retry (delete + redownload)
        let policy = RetryPolicy::default();
        assert!(should_retry_download("hash mismatch", 0, &policy));
        assert!(should_retry_download("checksum failed", 0, &policy));
    }

    #[test]
    fn should_retry_download_cancelled_errors() {
        let policy = RetryPolicy::default();
        assert!(!should_retry_download("cancelled", 0, &policy));
        assert!(!should_retry_download("cancelled by user", 1, &policy));
    }

    #[test]
    fn should_retry_download_unreachable_limited() {
        let policy = RetryPolicy::default();
        // Unreachable errors retry max twice (attempts 0 and 1)
        assert!(should_retry_download("DNS failure", 0, &policy));
        assert!(should_retry_download("DNS failure", 1, &policy));
        assert!(!should_retry_download("DNS failure", 2, &policy));
    }

    #[test]
    fn should_retry_download_respects_max_retries() {
        let policy = RetryPolicy {
            max_retries: 2,
            base_delay_ms: 500,
            max_delay_ms: 10_000,
            retry_stream_errors: true,
        };
        assert!(should_retry_download("timeout", 0, &policy));
        assert!(should_retry_download("timeout", 1, &policy));
        assert!(!should_retry_download("timeout", 2, &policy));
        assert!(!should_retry_download("timeout", 3, &policy));
    }

    // ========================================================================
    // Tests for calculate_adaptive_timeout
    // ========================================================================

    #[test]
    fn calculate_adaptive_timeout_basic() {
        // File: 1000 bytes, Speed: 100 bps, Min: 5s
        // Transfer time: 1000/100 = 10s
        // With 50% buffer: 10 + 5 = 15s
        let timeout = calculate_adaptive_timeout(1000, 100, 5);
        assert_eq!(timeout, 15);
    }

    #[test]
    fn calculate_adaptive_timeout_respects_minimum() {
        // File: 100 bytes, Speed: 100 bps, Min: 10s
        // Transfer time: 100/100 = 1s
        // With 50% buffer: 1 + 0.5 = 1.5s
        // Should return min of 10s
        let timeout = calculate_adaptive_timeout(100, 100, 10);
        assert_eq!(timeout, 10);
    }

    #[test]
    fn calculate_adaptive_timeout_zero_speed() {
        // Zero speed should return minimum timeout
        let timeout = calculate_adaptive_timeout(1000, 0, 30);
        assert_eq!(timeout, 30);
    }

    #[test]
    fn calculate_adaptive_timeout_large_file() {
        // 10 MB file at 1 MB/s
        let mb = 1024 * 1024;
        let file_size = 10 * mb;
        let speed = mb; // 1 MB/s
        let timeout = calculate_adaptive_timeout(file_size, speed, 5);
        // Transfer time: 10s, with 50% buffer: 15s
        assert_eq!(timeout, 15);
    }

    // ========================================================================
    // Tests for estimate_speed
    // ========================================================================

    #[test]
    fn estimate_speed_basic() {
        let speed = estimate_speed(1000, 10.0, 500);
        assert_eq!(speed, 100); // 1000 bytes / 10 seconds
    }

    #[test]
    fn estimate_speed_insufficient_time() {
        // Less than 1 second should return default
        let speed = estimate_speed(100, 0.5, 500);
        assert_eq!(speed, 500);
    }

    #[test]
    fn estimate_speed_zero_bytes() {
        let speed = estimate_speed(0, 10.0, 500);
        assert_eq!(speed, 500);
    }

    #[test]
    fn estimate_speed_zero_time() {
        let speed = estimate_speed(1000, 0.0, 500);
        assert_eq!(speed, 500);
    }

    #[test]
    fn estimate_speed_large_values() {
        let gb = 1024u64 * 1024 * 1024;
        let speed = estimate_speed(gb, 100.0, 1000);
        assert_eq!(speed, gb / 100);
    }

    // ========================================================================
    // Tests for DownloadHealth
    // ========================================================================

    #[test]
    fn download_health_default() {
        let health = DownloadHealth::default();
        assert_eq!(health.successful_chunks, 0);
        assert_eq!(health.retried_chunks, 0);
        assert_eq!(health.success_streak, 0);
        assert_eq!(health.max_consecutive_errors, 0);
        assert_eq!(health.consecutive_errors, 0);
    }

    #[test]
    fn download_health_new() {
        let health = DownloadHealth::new();
        assert_eq!(health.successful_chunks, 0);
    }

    #[test]
    fn download_health_record_success() {
        let mut health = DownloadHealth::new();
        health.record_success();
        assert_eq!(health.successful_chunks, 1);
        assert_eq!(health.success_streak, 1);
        assert_eq!(health.consecutive_errors, 0);

        health.record_success();
        assert_eq!(health.successful_chunks, 2);
        assert_eq!(health.success_streak, 2);
    }

    #[test]
    fn download_health_record_retry() {
        let mut health = DownloadHealth::new();
        health.record_retry();
        assert_eq!(health.retried_chunks, 1);
        assert_eq!(health.consecutive_errors, 1);
        assert_eq!(health.max_consecutive_errors, 1);
        assert_eq!(health.success_streak, 0);
    }

    #[test]
    fn download_health_consecutive_errors_tracking() {
        let mut health = DownloadHealth::new();
        health.record_retry();
        health.record_retry();
        health.record_retry();
        assert_eq!(health.consecutive_errors, 3);
        assert_eq!(health.max_consecutive_errors, 3);

        // Success resets consecutive count
        health.record_success();
        assert_eq!(health.consecutive_errors, 0);
        assert_eq!(health.max_consecutive_errors, 3); // Max persists
    }

    #[test]
    fn download_health_score_perfect() {
        let mut health = DownloadHealth::new();
        health.successful_chunks = 100;
        health.retried_chunks = 0;
        assert_eq!(health.health_score(), 1.0);
    }

    #[test]
    fn download_health_score_half() {
        let mut health = DownloadHealth::new();
        health.successful_chunks = 50;
        health.retried_chunks = 50;
        assert_eq!(health.health_score(), 0.5);
    }

    #[test]
    fn download_health_score_poor() {
        let mut health = DownloadHealth::new();
        health.successful_chunks = 25;
        health.retried_chunks = 75;
        assert_eq!(health.health_score(), 0.25);
    }

    #[test]
    fn download_health_score_no_data() {
        let health = DownloadHealth::new();
        // With no data, assume perfect health
        assert_eq!(health.health_score(), 1.0);
    }

    #[test]
    fn download_health_is_healthy_good() {
        let mut health = DownloadHealth::new();
        health.successful_chunks = 80;
        health.retried_chunks = 20;
        health.consecutive_errors = 2;
        assert!(health.is_healthy());
    }

    #[test]
    fn download_health_is_healthy_too_many_retries() {
        let mut health = DownloadHealth::new();
        health.successful_chunks = 40;
        health.retried_chunks = 60; // 40% success rate
        health.consecutive_errors = 2;
        assert!(!health.is_healthy()); // Below 50% threshold
    }

    #[test]
    fn download_health_is_healthy_too_many_consecutive_errors() {
        let mut health = DownloadHealth::new();
        health.successful_chunks = 90;
        health.retried_chunks = 10;
        health.consecutive_errors = 5; // Hit threshold
        assert!(!health.is_healthy());
    }

    #[test]
    fn download_health_is_healthy_boundary_50_percent() {
        let mut health = DownloadHealth::new();
        health.successful_chunks = 50;
        health.retried_chunks = 50;
        health.consecutive_errors = 0;
        // Exactly 50% - is_healthy requires > 0.5
        assert!(!health.is_healthy());
    }

    #[test]
    fn download_health_is_healthy_boundary_errors() {
        let mut health = DownloadHealth::new();
        health.successful_chunks = 90;
        health.retried_chunks = 10;
        health.consecutive_errors = 4;
        assert!(health.is_healthy()); // 4 errors is under threshold of 5
    }
}
