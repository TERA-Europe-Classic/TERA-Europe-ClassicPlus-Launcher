//! Retry logic and timeout utilities.
//!
//! This module provides pure functions for retry/timeout operations, including:
//! - Stall detection for downloads
//! - Transient error classification
//! - Retry delay calculation

use crate::domain::RETRY_DELAY_BASE_MS;

/// Determines if a download has stalled based on byte progress.
///
/// A download is considered stalled if:
/// 1. No bytes have been transferred (current == last)
/// 2. The idle time exceeds the threshold
///
/// # Arguments
/// * `last_bytes` - Byte count at the previous check
/// * `current_bytes` - Current byte count
/// * `idle_secs` - Seconds since last progress
/// * `threshold_secs` - Maximum allowed idle time before stall
///
/// # Returns
/// * `true` if the download is stalled
/// * `false` if progress is being made or within threshold
///
/// # Examples
/// ```ignore
/// // Stalled - no progress for 61 seconds with 60 second threshold
/// assert!(stall_exceeded(100, 100, 61, 60));
///
/// // Not stalled - within threshold
/// assert!(!stall_exceeded(100, 100, 30, 60));
///
/// // Not stalled - progress was made
/// assert!(!stall_exceeded(100, 120, 61, 60));
/// ```
pub fn stall_exceeded(
    last_bytes: u64,
    current_bytes: u64,
    idle_secs: u64,
    threshold_secs: u64,
) -> bool {
    if current_bytes != last_bytes {
        return false;
    }
    idle_secs >= threshold_secs
}

/// Checks if an error message indicates a transient (retriable) download error.
///
/// Transient errors are temporary network issues that may resolve on retry:
/// - Timeouts
/// - Connection resets/closes
/// - Broken pipes
/// - Temporary service unavailability
/// - DNS issues
/// - HTTP 502, 503, 504 errors
///
/// # Arguments
/// * `message` - The error message to analyze
///
/// # Returns
/// * `true` if the error is likely transient and worth retrying
/// * `false` if the error is permanent (e.g., 404, permission denied)
///
/// # Examples
/// ```ignore
/// assert!(is_transient_download_error("request timed out"));
/// assert!(is_transient_download_error("connection reset by peer"));
/// assert!(is_transient_download_error("HTTP 503"));
/// assert!(!is_transient_download_error("hash mismatch"));
/// assert!(!is_transient_download_error("HTTP 404 not found"));
/// ```
pub fn is_transient_download_error(message: &str) -> bool {
    let msg = message.to_lowercase();
    msg.contains("timed out")
        || msg.contains("timeout")
        || msg.contains("connection reset")
        || msg.contains("connection closed")
        || msg.contains("broken pipe")
        || msg.contains("temporarily")
        || msg.contains("network")
        || msg.contains("dns")
        || msg.contains("503")
        || msg.contains("502")
        || msg.contains("504")
}

/// Calculates the retry delay in milliseconds for a given attempt number.
///
/// Uses linear backoff: delay = RETRY_DELAY_BASE_MS * attempt
///
/// # Arguments
/// * `attempt` - The retry attempt number (0-indexed)
///
/// # Returns
/// The delay in milliseconds before the retry
///
/// # Examples
/// ```ignore
/// assert_eq!(retry_delay_ms(0), 0);     // First attempt - no delay
/// assert_eq!(retry_delay_ms(1), 500);   // Second attempt - 500ms
/// assert_eq!(retry_delay_ms(2), 1000);  // Third attempt - 1000ms
/// ```
pub fn retry_delay_ms(attempt: u8) -> u64 {
    RETRY_DELAY_BASE_MS.saturating_mul(attempt as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Tests for stall_exceeded
    // ========================================================================

    #[test]
    fn stall_exceeded_detects_no_progress() {
        assert!(stall_exceeded(100, 100, 61, 60));
        assert!(!stall_exceeded(100, 100, 30, 60));
        assert!(!stall_exceeded(100, 120, 61, 60));
    }

    #[test]
    fn stall_exceeded_exactly_at_threshold() {
        // idle_secs == threshold_secs should return true
        assert!(stall_exceeded(100, 100, 60, 60));
    }

    #[test]
    fn stall_exceeded_just_below_threshold() {
        assert!(!stall_exceeded(100, 100, 59, 60));
    }

    #[test]
    fn stall_exceeded_zero_threshold() {
        // With threshold of 0, any idle time >= 0 triggers stall
        assert!(stall_exceeded(100, 100, 0, 0));
    }

    #[test]
    fn stall_exceeded_progress_made_resets() {
        // Even with high idle time, if progress was made, no stall
        assert!(!stall_exceeded(100, 101, 1000, 60));
    }

    #[test]
    fn stall_exceeded_zero_bytes() {
        // Edge case: both bytes are 0
        assert!(stall_exceeded(0, 0, 61, 60));
        assert!(!stall_exceeded(0, 0, 30, 60));
    }

    #[test]
    fn stall_exceeded_large_values() {
        let large = u64::MAX / 2;
        assert!(stall_exceeded(large, large, 61, 60));
        assert!(!stall_exceeded(large, large + 1, 61, 60));
    }

    // ========================================================================
    // Tests for is_transient_download_error
    // ========================================================================

    #[test]
    fn transient_error_detection() {
        assert!(is_transient_download_error("request timed out"));
        assert!(is_transient_download_error("connection reset by peer"));
        assert!(is_transient_download_error("HTTP 503"));
        assert!(!is_transient_download_error("hash mismatch"));
    }

    #[test]
    fn is_transient_all_patterns() {
        // Test all transient patterns
        assert!(is_transient_download_error("request TIMED OUT"));
        assert!(is_transient_download_error("Connection Timeout occurred"));
        assert!(is_transient_download_error("connection reset by peer"));
        assert!(is_transient_download_error(
            "connection closed unexpectedly"
        ));
        assert!(is_transient_download_error("broken pipe error"));
        assert!(is_transient_download_error(
            "service temporarily unavailable"
        ));
        assert!(is_transient_download_error("network error"));
        assert!(is_transient_download_error("DNS resolution failed"));
        assert!(is_transient_download_error("HTTP error 503"));
        assert!(is_transient_download_error("error 502 bad gateway"));
        assert!(is_transient_download_error("gateway timeout 504"));
    }

    #[test]
    fn is_transient_non_transient_errors() {
        assert!(!is_transient_download_error("file not found"));
        assert!(!is_transient_download_error("permission denied"));
        assert!(!is_transient_download_error("hash mismatch"));
        assert!(!is_transient_download_error("invalid response format"));
        assert!(!is_transient_download_error("HTTP 404 not found"));
        assert!(!is_transient_download_error("HTTP 401 unauthorized"));
        assert!(!is_transient_download_error(
            "HTTP 500 internal server error"
        ));
    }

    #[test]
    fn is_transient_case_insensitive() {
        assert!(is_transient_download_error("TIMED OUT"));
        assert!(is_transient_download_error("Timed Out"));
        assert!(is_transient_download_error("CONNECTION RESET"));
        assert!(is_transient_download_error("NETWORK ERROR"));
    }

    #[test]
    fn is_transient_empty_string() {
        assert!(!is_transient_download_error(""));
    }

    #[test]
    fn is_transient_partial_matches() {
        // Ensure partial word matches work
        assert!(is_transient_download_error("timeout occurred"));
        assert!(is_transient_download_error(
            "the request timed out completely"
        ));
    }

    // ========================================================================
    // Tests for retry_delay_ms
    // ========================================================================

    #[test]
    fn retry_delay_grows_by_attempt() {
        assert_eq!(retry_delay_ms(0), 0);
        assert_eq!(retry_delay_ms(1), 500);
        assert_eq!(retry_delay_ms(2), 1000);
    }

    #[test]
    fn retry_delay_high_attempts() {
        // Test that it doesn't overflow
        assert_eq!(retry_delay_ms(10), 5000);
        assert_eq!(retry_delay_ms(255), 127500);
    }

    #[test]
    fn retry_delay_boundary_values() {
        assert_eq!(retry_delay_ms(0), 0);
        assert_eq!(retry_delay_ms(u8::MAX), 500 * 255);
    }

    #[test]
    fn retry_delay_typical_usage() {
        // Typical retry pattern with MAX_RETRIES = 2
        assert_eq!(retry_delay_ms(0), 0); // First attempt
        assert_eq!(retry_delay_ms(1), 500); // First retry
        assert_eq!(retry_delay_ms(2), 1000); // Second retry
    }
}
