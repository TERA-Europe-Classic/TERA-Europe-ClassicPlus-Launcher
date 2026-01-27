//! Download service for file download orchestration.
//!
//! This module provides pure functions for download management:
//! - Download progress calculation
//! - URL correction
//! - Batch download orchestration logic

#![allow(dead_code)]

use crate::domain::FileInfo;
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
}
