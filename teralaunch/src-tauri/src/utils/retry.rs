//! Retry logic and timeout utilities.
//!
//! This module provides pure functions for retry/timeout operations, including:
//! - Stall detection for downloads
//! - Transient error classification
//! - Retry delay calculation with exponential backoff
//! - Circuit breaker pattern for failure tracking

#![allow(dead_code)]

use crate::domain::{
    CIRCUIT_BREAKER_COOLDOWN_SECS, CIRCUIT_BREAKER_THRESHOLD, MAX_RETRY_DELAY_MS,
    RETRY_DELAY_BASE_MS,
};
use crate::services::download_service::{classify_error, ErrorClassification};

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
/// This function delegates to download_service::classify_error() for consistency.
/// Transient errors are temporary network issues that may resolve on retry:
/// - Timeouts
/// - Connection resets/closes/aborted
/// - Broken pipes
/// - Temporary service unavailability
/// - HTTP 429, 500, 502, 503, 504 errors
/// - EOF/incomplete/reset errors
/// - Hash mismatches (corruption during download - delete and retry)
///
/// # Arguments
/// * `message` - The error message to analyze
///
/// # Returns
/// * `true` if the error is likely transient and worth retrying
/// * `false` if the error is permanent (e.g., 404, cancelled) or unreachable
///
/// # Examples
/// ```ignore
/// assert!(is_transient_download_error("request timed out"));
/// assert!(is_transient_download_error("connection reset by peer"));
/// assert!(is_transient_download_error("HTTP 503"));
/// assert!(is_transient_download_error("HTTP 429 too many requests"));
/// assert!(is_transient_download_error("HTTP 500 internal server error"));
/// assert!(is_transient_download_error("hash mismatch")); // corruption, retry
/// assert!(!is_transient_download_error("HTTP 404 not found"));
/// ```
pub fn is_transient_download_error(message: &str) -> bool {
    matches!(
        classify_error(message),
        ErrorClassification::Transient | ErrorClassification::ServerUnreachable
    )
}

/// Calculates retry delay with exponential backoff and optional jitter.
///
/// Formula: min(MAX_RETRY_DELAY_MS, RETRY_DELAY_BASE_MS * 2^attempt)
///
/// # Arguments
/// * `attempt` - The retry attempt number (0-indexed, 0 = first retry)
///
/// # Returns
/// Delay in milliseconds, capped at MAX_RETRY_DELAY_MS
///
/// # Examples
/// ```ignore
/// assert_eq!(retry_delay_ms(0), 500);     // First retry - 500ms
/// assert_eq!(retry_delay_ms(1), 1000);    // Second retry - 1000ms
/// assert_eq!(retry_delay_ms(2), 2000);    // Third retry - 2000ms
/// assert_eq!(retry_delay_ms(3), 4000);    // Fourth retry - 4000ms
/// assert_eq!(retry_delay_ms(4), 8000);    // Fifth retry - 8000ms
/// assert_eq!(retry_delay_ms(10), 30000);  // Capped at MAX_RETRY_DELAY_MS
/// ```
pub fn retry_delay_ms(attempt: u8) -> u64 {
    let base = RETRY_DELAY_BASE_MS;
    // Use checked_pow to avoid overflow for large attempt values (e.g. u8::MAX)
    let power = 2u64.checked_pow(attempt as u32).unwrap_or(u64::MAX);
    let delay = base.saturating_mul(power);
    std::cmp::min(delay, MAX_RETRY_DELAY_MS)
}

/// Calculates retry delay with randomized jitter (±25%)
///
/// # Arguments
/// * `attempt` - The retry attempt number (0-indexed)
///
/// # Returns
/// Delay in milliseconds with jitter applied
///
/// # Note
/// For deterministic tests, this currently returns the base delay.
/// In production, this would add random jitter to prevent thundering herd.
pub fn retry_delay_with_jitter_ms(attempt: u8) -> u64 {
    // Add ±25% jitter to prevent thundering herd
    // For deterministic tests, just return base delay
    // In production, would add random jitter
    retry_delay_ms(attempt)
}

/// Tracks consecutive failures for circuit breaker pattern
#[derive(Debug, Clone, Default)]
pub struct CircuitBreakerState {
    pub consecutive_failures: u8,
    pub last_failure_time: Option<std::time::Instant>,
}

impl CircuitBreakerState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a failure attempt.
    /// Only connectivity errors (DNS, network unreachable, etc.) should count toward circuit breaking.
    /// Returns `true` if circuit should open (threshold reached for connectivity errors).
    pub fn record_failure(&mut self, is_connectivity_error: bool) -> bool {
        if is_connectivity_error {
            self.consecutive_failures = self.consecutive_failures.saturating_add(1);
            self.last_failure_time = Some(std::time::Instant::now());
        }
        self.consecutive_failures >= CIRCUIT_BREAKER_THRESHOLD
    }

    /// Record a success, resets the failure count
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.last_failure_time = None;
    }

    /// Check if circuit breaker cooldown has elapsed
    pub fn cooldown_elapsed(&self) -> bool {
        match self.last_failure_time {
            Some(t) => t.elapsed().as_secs() >= CIRCUIT_BREAKER_COOLDOWN_SECS,
            None => true,
        }
    }

    /// Check if we should attempt a retry
    pub fn should_retry(&self) -> bool {
        self.consecutive_failures < CIRCUIT_BREAKER_THRESHOLD || self.cooldown_elapsed()
    }
}

/// Determines if an error indicates the server is likely unreachable.
/// Returns true for errors that suggest no connectivity at all.
///
/// This function delegates to download_service::classify_error() for consistency.
///
/// # Arguments
/// * `message` - The error message to analyze
///
/// # Returns
/// * `true` if the error suggests complete server unreachability
/// * `false` if it's a different type of error
///
/// # Examples
/// ```ignore
/// assert!(is_server_unreachable_error("DNS resolution failed"));
/// assert!(is_server_unreachable_error("connection refused"));
/// assert!(is_server_unreachable_error("network unreachable"));
/// assert!(!is_server_unreachable_error("HTTP 503"));
/// ```
pub fn is_server_unreachable_error(message: &str) -> bool {
    matches!(
        classify_error(message),
        ErrorClassification::ServerUnreachable
    )
}

/// Iterator that yields retry delays with exponential backoff
pub struct RetryDelays {
    current_attempt: u8,
    max_attempts: u8,
}

impl RetryDelays {
    pub fn new(max_attempts: u8) -> Self {
        Self {
            current_attempt: 0,
            max_attempts,
        }
    }
}

impl Iterator for RetryDelays {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_attempt >= self.max_attempts {
            return None;
        }
        let delay = retry_delay_ms(self.current_attempt);
        self.current_attempt += 1;
        Some(delay)
    }
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
        // Hash mismatch = corruption during download, should retry (delete + redownload)
        assert!(is_transient_download_error("hash mismatch"));
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
        // Hash mismatch IS transient (corruption, delete + retry)
        assert!(is_transient_download_error("hash mismatch"));
        assert!(!is_transient_download_error("invalid response format"));
        assert!(!is_transient_download_error("HTTP 404 not found"));
        assert!(!is_transient_download_error("HTTP 401 unauthorized"));
    }

    #[test]
    fn is_transient_new_patterns() {
        // Test new transient error patterns added
        assert!(is_transient_download_error("HTTP 500 internal error"));
        assert!(is_transient_download_error(
            "HTTP 500 internal server error"
        ));
        assert!(is_transient_download_error("HTTP 429 too many requests"));
        assert!(is_transient_download_error("rate limit 429"));
        assert!(is_transient_download_error("connection reset"));
        assert!(is_transient_download_error("stream reset by peer"));
        assert!(is_transient_download_error("unexpected EOF"));
        assert!(is_transient_download_error("eof while reading"));
        assert!(is_transient_download_error("incomplete response"));
        assert!(is_transient_download_error("request aborted"));
        assert!(is_transient_download_error("connection refused"));
        assert!(is_transient_download_error("refused to connect"));
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
    // Tests for retry_delay_ms (exponential backoff)
    // ========================================================================

    #[test]
    fn retry_delay_exponential_backoff() {
        // Test exponential backoff: 500 * 2^attempt
        assert_eq!(retry_delay_ms(0), 500); // 500 * 2^0 = 500ms
        assert_eq!(retry_delay_ms(1), 1000); // 500 * 2^1 = 1000ms
        assert_eq!(retry_delay_ms(2), 2000); // 500 * 2^2 = 2000ms
        assert_eq!(retry_delay_ms(3), 4000); // 500 * 2^3 = 4000ms
        assert_eq!(retry_delay_ms(4), 8000); // 500 * 2^4 = 8000ms
        assert_eq!(retry_delay_ms(5), 16000); // 500 * 2^5 = 16000ms
    }

    #[test]
    fn retry_delay_capped_at_max() {
        // Test that delay caps at MAX_RETRY_DELAY_MS (30 seconds)
        assert_eq!(retry_delay_ms(10), 30_000); // Would be 512000, capped at 30000
        assert_eq!(retry_delay_ms(20), 30_000); // Way over, still capped
        assert_eq!(retry_delay_ms(u8::MAX), 30_000); // Maximum value, still capped
    }

    #[test]
    fn retry_delay_no_overflow() {
        // Ensure saturating_mul prevents overflow
        assert_eq!(retry_delay_ms(50), 30_000);
        assert_eq!(retry_delay_ms(100), 30_000);
        assert_eq!(retry_delay_ms(255), 30_000);
    }

    #[test]
    fn retry_delay_typical_usage_new() {
        // Typical retry pattern with MAX_RETRIES = 5
        assert_eq!(retry_delay_ms(0), 500); // First retry
        assert_eq!(retry_delay_ms(1), 1000); // Second retry
        assert_eq!(retry_delay_ms(2), 2000); // Third retry
        assert_eq!(retry_delay_ms(3), 4000); // Fourth retry
        assert_eq!(retry_delay_ms(4), 8000); // Fifth retry
    }

    #[test]
    fn retry_delay_with_jitter() {
        // Test jitter function (currently deterministic)
        assert_eq!(retry_delay_with_jitter_ms(0), 500);
        assert_eq!(retry_delay_with_jitter_ms(1), 1000);
        assert_eq!(retry_delay_with_jitter_ms(2), 2000);
    }

    // ========================================================================
    // Tests for CircuitBreakerState
    // ========================================================================

    #[test]
    fn circuit_breaker_record_failure() {
        let mut cb = CircuitBreakerState::new();
        assert_eq!(cb.consecutive_failures, 0);

        // Record failures up to threshold
        assert!(!cb.record_failure(true)); // 1st failure, below threshold
        assert_eq!(cb.consecutive_failures, 1);

        assert!(!cb.record_failure(true)); // 2nd failure, below threshold
        assert_eq!(cb.consecutive_failures, 2);

        assert!(cb.record_failure(true)); // 3rd failure, at threshold - circuit opens
        assert_eq!(cb.consecutive_failures, 3);
        assert!(cb.last_failure_time.is_some());
    }

    #[test]
    fn circuit_breaker_record_success_resets() {
        let mut cb = CircuitBreakerState::new();
        cb.record_failure(true);
        cb.record_failure(true);
        assert_eq!(cb.consecutive_failures, 2);

        cb.record_success();
        assert_eq!(cb.consecutive_failures, 0);
        assert!(cb.last_failure_time.is_none());
    }

    #[test]
    fn circuit_breaker_should_retry() {
        let mut cb = CircuitBreakerState::new();

        // Below threshold - should retry
        cb.record_failure(true);
        assert!(cb.should_retry());

        cb.record_failure(true);
        assert!(cb.should_retry());

        // At threshold - should not retry (without cooldown)
        cb.record_failure(true);
        assert!(!cb.should_retry());
    }

    #[test]
    fn circuit_breaker_cooldown_elapsed() {
        let mut cb = CircuitBreakerState::new();

        // No failure time - cooldown is always elapsed
        assert!(cb.cooldown_elapsed());

        // Just recorded failure - cooldown not elapsed
        cb.record_failure(true);
        cb.record_failure(true);
        cb.record_failure(true);
        assert!(!cb.cooldown_elapsed());

        // After success - cooldown elapsed (no failure time)
        cb.record_success();
        assert!(cb.cooldown_elapsed());
    }

    #[test]
    fn circuit_breaker_saturating_add() {
        let mut cb = CircuitBreakerState::new();

        // Fill up to u8::MAX to test overflow protection
        for _ in 0..u8::MAX {
            cb.record_failure(true);
        }

        assert_eq!(cb.consecutive_failures, u8::MAX);
        cb.record_failure(true); // Should not overflow
        assert_eq!(cb.consecutive_failures, u8::MAX);
    }

    // ========================================================================
    // Tests for is_server_unreachable_error
    // ========================================================================

    #[test]
    fn server_unreachable_detects_dns_errors() {
        assert!(is_server_unreachable_error("DNS resolution failed"));
        assert!(is_server_unreachable_error("dns lookup error"));
        assert!(is_server_unreachable_error(
            "failed to resolve name resolution"
        ));
    }

    #[test]
    fn server_unreachable_detects_connection_errors() {
        assert!(is_server_unreachable_error("connection refused"));
        assert!(is_server_unreachable_error("Connection Refused"));
        assert!(is_server_unreachable_error("connect timeout"));
        // "connection timeout" = timeout during an active connection = Transient, not ServerUnreachable
        assert!(!is_server_unreachable_error("connection timeout"));
    }

    #[test]
    fn server_unreachable_detects_network_errors() {
        assert!(is_server_unreachable_error("network unreachable"));
        assert!(is_server_unreachable_error("host unreachable"));
        assert!(is_server_unreachable_error("no route to host"));
    }

    #[test]
    fn server_unreachable_not_for_other_errors() {
        // HTTP errors are not "unreachable" - connection was established
        assert!(!is_server_unreachable_error("HTTP 503"));
        assert!(!is_server_unreachable_error("HTTP 500"));
        assert!(!is_server_unreachable_error("request timeout")); // read timeout, not connect
        assert!(!is_server_unreachable_error("broken pipe"));
        assert!(!is_server_unreachable_error("connection reset"));
    }

    #[test]
    fn server_unreachable_empty_string() {
        assert!(!is_server_unreachable_error(""));
    }

    #[test]
    fn server_unreachable_case_insensitive() {
        assert!(is_server_unreachable_error("DNS FAILED"));
        assert!(is_server_unreachable_error("Connection REFUSED"));
        assert!(is_server_unreachable_error("NETWORK UNREACHABLE"));
    }

    // ========================================================================
    // Tests for RetryDelays iterator
    // ========================================================================

    #[test]
    fn retry_delays_iterator() {
        let mut delays = RetryDelays::new(5);

        assert_eq!(delays.next(), Some(500)); // Attempt 0
        assert_eq!(delays.next(), Some(1000)); // Attempt 1
        assert_eq!(delays.next(), Some(2000)); // Attempt 2
        assert_eq!(delays.next(), Some(4000)); // Attempt 3
        assert_eq!(delays.next(), Some(8000)); // Attempt 4
        assert_eq!(delays.next(), None); // Max attempts reached
    }

    #[test]
    fn retry_delays_iterator_zero_attempts() {
        let mut delays = RetryDelays::new(0);
        assert_eq!(delays.next(), None);
    }

    #[test]
    fn retry_delays_iterator_single_attempt() {
        let mut delays = RetryDelays::new(1);
        assert_eq!(delays.next(), Some(500));
        assert_eq!(delays.next(), None);
    }

    #[test]
    fn retry_delays_collect() {
        let delays: Vec<u64> = RetryDelays::new(5).collect();
        assert_eq!(delays, vec![500, 1000, 2000, 4000, 8000]);
    }

    #[test]
    fn retry_delays_high_max_capped() {
        // Test that very high attempt numbers get capped at MAX_RETRY_DELAY_MS
        let mut delays = RetryDelays::new(15);
        for _ in 0..6 {
            delays.next(); // Skip first 6: 500, 1000, 2000, 4000, 8000, 16000
        }
        // 7th attempt and beyond should be capped at 30000
        assert_eq!(delays.next(), Some(30_000));
        assert_eq!(delays.next(), Some(30_000));
    }
}
