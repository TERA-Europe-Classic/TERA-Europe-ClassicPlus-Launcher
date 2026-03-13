//! Hash service for file verification.
//!
//! This module provides pure functions for hash operations:
//! - SHA-256 hash calculation
//! - Hash comparison and verification
//! - Batch hash operations

#![allow(dead_code)]
// BUFFER_SIZE is imported by the base branch and used only in test builds;
// allow unused_imports to avoid false positives when merging.
#![allow(unused_imports)]

use sha2::{Digest, Sha256};
use std::io::Read;

use crate::domain::{BUFFER_SIZE, HASH_BUFFER_SIZE};

/// Calculates SHA-256 hash from a reader.
///
/// This is the core hash function that works with any Read source,
/// making it easy to test without actual files.
///
/// # Arguments
/// * `reader` - Any type implementing Read trait
///
/// # Returns
/// * `Ok(String)` - The hex-encoded SHA-256 hash
/// * `Err(String)` - Error message if reading fails
///
/// # Examples
/// ```ignore
/// let data = b"Hello, World!";
/// let hash = calculate_hash_from_reader(&data[..])?;
/// assert_eq!(hash.len(), 64); // SHA-256 produces 64 hex chars
/// ```
#[cfg(not(tarpaulin_include))]
pub fn calculate_hash_from_reader<R: Read>(mut reader: R) -> Result<String, String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; HASH_BUFFER_SIZE];

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| format!("Failed to read: {}", e))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Calculates SHA-256 hash from a byte slice.
///
/// Convenience function for hashing in-memory data.
///
/// # Arguments
/// * `data` - The bytes to hash
///
/// # Returns
/// The hex-encoded SHA-256 hash
///
/// # Examples
/// ```ignore
/// let hash = calculate_hash_from_bytes(b"Hello, World!");
/// assert_eq!(hash.len(), 64);
/// ```
pub fn calculate_hash_from_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Compares two hash strings for equality.
///
/// Performs case-insensitive comparison.
///
/// # Arguments
/// * `hash1` - First hash string
/// * `hash2` - Second hash string
///
/// # Returns
/// `true` if the hashes match, `false` otherwise
///
/// # Examples
/// ```ignore
/// assert!(hashes_match("abc123", "ABC123"));
/// assert!(!hashes_match("abc123", "xyz789"));
/// ```
pub fn hashes_match(hash1: &str, hash2: &str) -> bool {
    hash1.eq_ignore_ascii_case(hash2)
}

/// Verifies that data matches an expected hash.
///
/// # Arguments
/// * `data` - The data to verify
/// * `expected_hash` - The expected SHA-256 hash
///
/// # Returns
/// * `Ok(true)` - If the hash matches
/// * `Ok(false)` - If the hash doesn't match
/// * `Err(String)` - If hash calculation fails
///
/// # Examples
/// ```ignore
/// let data = b"test data";
/// let hash = calculate_hash_from_bytes(data);
/// assert!(verify_hash(data, &hash)?);
/// ```
pub fn verify_hash(data: &[u8], expected_hash: &str) -> Result<bool, String> {
    let actual_hash = calculate_hash_from_bytes(data);
    Ok(hashes_match(&actual_hash, expected_hash))
}

/// Result of comparing a file's hash against expected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HashCheckResult {
    /// File matches expected hash
    Match,
    /// File hash differs from expected
    Mismatch { expected: String, actual: String },
    /// File doesn't exist
    Missing,
    /// Hash calculation failed
    Error(String),
}

impl HashCheckResult {
    /// Returns true if the hash check was successful (file matches).
    pub fn is_match(&self) -> bool {
        matches!(self, HashCheckResult::Match)
    }

    /// Returns true if the file needs to be downloaded/updated.
    pub fn needs_update(&self) -> bool {
        !matches!(self, HashCheckResult::Match)
    }
}

/// Checks if a file needs updating based on hash comparison.
///
/// This is a pure function version that takes the file content
/// as a reader, making it testable without filesystem access.
///
/// # Arguments
/// * `reader` - Optional reader for existing file content
/// * `expected_hash` - The expected SHA-256 hash
///
/// # Returns
/// The result of the hash check
pub fn check_file_hash<R: Read>(reader: Option<R>, expected_hash: &str) -> HashCheckResult {
    match reader {
        None => HashCheckResult::Missing,
        Some(r) => match calculate_hash_from_reader(r) {
            Ok(actual) => {
                if hashes_match(&actual, expected_hash) {
                    HashCheckResult::Match
                } else {
                    HashCheckResult::Mismatch {
                        expected: expected_hash.to_string(),
                        actual,
                    }
                }
            }
            Err(e) => HashCheckResult::Error(e),
        },
    }
}

/// Batch hash check statistics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HashCheckStats {
    /// Number of files that match
    pub matched: usize,
    /// Number of files with mismatched hash
    pub mismatched: usize,
    /// Number of missing files
    pub missing: usize,
    /// Number of files with errors
    pub errors: usize,
}

impl HashCheckStats {
    /// Returns total number of files checked.
    pub fn total(&self) -> usize {
        self.matched + self.mismatched + self.missing + self.errors
    }

    /// Returns number of files needing updates.
    pub fn needs_update(&self) -> usize {
        self.mismatched + self.missing
    }

    /// Returns true if all files match.
    pub fn all_match(&self) -> bool {
        self.mismatched == 0 && self.missing == 0 && self.errors == 0
    }
}

/// Aggregates multiple hash check results into statistics.
///
/// # Arguments
/// * `results` - Iterator of hash check results
///
/// # Returns
/// Aggregated statistics
pub fn aggregate_hash_results<'a>(
    results: impl Iterator<Item = &'a HashCheckResult>,
) -> HashCheckStats {
    let mut stats = HashCheckStats::default();

    for result in results {
        match result {
            HashCheckResult::Match => stats.matched += 1,
            HashCheckResult::Mismatch { .. } => stats.mismatched += 1,
            HashCheckResult::Missing => stats.missing += 1,
            HashCheckResult::Error(_) => stats.errors += 1,
        }
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // Known SHA-256 hashes for test data
    const EMPTY_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const HELLO_HASH: &str = "185f8db32271fe25f561a6fc938b2e264306ec304eda518007d1764826381969";

    // ========================================================================
    // Tests for calculate_hash_from_reader
    // ========================================================================

    #[test]
    fn calculate_hash_from_reader_empty() {
        let data: &[u8] = b"";
        let result = calculate_hash_from_reader(Cursor::new(data));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), EMPTY_HASH);
    }

    #[test]
    fn calculate_hash_from_reader_hello() {
        let data = b"Hello";
        let result = calculate_hash_from_reader(Cursor::new(data));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), HELLO_HASH);
    }

    #[test]
    fn calculate_hash_from_reader_large_data() {
        // Create data larger than buffer size
        let data = vec![0u8; BUFFER_SIZE * 3 + 100];
        let result = calculate_hash_from_reader(Cursor::new(&data));
        assert!(result.is_ok());
        // Just verify it produces a valid 64-char hex hash
        let hash = result.unwrap();
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn calculate_hash_from_reader_consistent() {
        let data = b"test data for consistency check";
        let hash1 = calculate_hash_from_reader(Cursor::new(data)).unwrap();
        let hash2 = calculate_hash_from_reader(Cursor::new(data)).unwrap();
        assert_eq!(hash1, hash2);
    }

    // ========================================================================
    // Tests for calculate_hash_from_bytes
    // ========================================================================

    #[test]
    fn calculate_hash_from_bytes_empty() {
        assert_eq!(calculate_hash_from_bytes(b""), EMPTY_HASH);
    }

    #[test]
    fn calculate_hash_from_bytes_hello() {
        assert_eq!(calculate_hash_from_bytes(b"Hello"), HELLO_HASH);
    }

    #[test]
    fn calculate_hash_from_bytes_binary() {
        // Test with binary data
        let data: Vec<u8> = (0u8..=255).collect();
        let hash = calculate_hash_from_bytes(&data);
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn calculate_hash_from_bytes_unicode() {
        let hash = calculate_hash_from_bytes("Hello, World!".as_bytes());
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ========================================================================
    // Tests for hashes_match
    // ========================================================================

    #[test]
    fn hashes_match_identical() {
        assert!(hashes_match("abc123def456", "abc123def456"));
    }

    #[test]
    fn hashes_match_case_insensitive() {
        assert!(hashes_match("ABC123DEF456", "abc123def456"));
        assert!(hashes_match("abc123DEF456", "ABC123def456"));
    }

    #[test]
    fn hashes_match_different() {
        assert!(!hashes_match("abc123", "xyz789"));
    }

    #[test]
    fn hashes_match_empty() {
        assert!(hashes_match("", ""));
    }

    #[test]
    fn hashes_match_different_lengths() {
        assert!(!hashes_match("abc", "abcd"));
    }

    // ========================================================================
    // Tests for verify_hash
    // ========================================================================

    #[test]
    fn verify_hash_match() {
        let data = b"Hello";
        let result = verify_hash(data, HELLO_HASH);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn verify_hash_mismatch() {
        let data = b"Hello";
        let result = verify_hash(
            data,
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn verify_hash_case_insensitive() {
        let data = b"Hello";
        let uppercase_hash = HELLO_HASH.to_uppercase();
        let result = verify_hash(data, &uppercase_hash);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    // ========================================================================
    // Tests for HashCheckResult
    // ========================================================================

    #[test]
    fn hash_check_result_is_match() {
        assert!(HashCheckResult::Match.is_match());
        assert!(!HashCheckResult::Missing.is_match());
        assert!(!HashCheckResult::Mismatch {
            expected: String::new(),
            actual: String::new()
        }
        .is_match());
        assert!(!HashCheckResult::Error(String::new()).is_match());
    }

    #[test]
    fn hash_check_result_needs_update() {
        assert!(!HashCheckResult::Match.needs_update());
        assert!(HashCheckResult::Missing.needs_update());
        assert!(HashCheckResult::Mismatch {
            expected: String::new(),
            actual: String::new()
        }
        .needs_update());
        assert!(HashCheckResult::Error(String::new()).needs_update());
    }

    // ========================================================================
    // Tests for check_file_hash
    // ========================================================================

    #[test]
    fn check_file_hash_missing() {
        let result: HashCheckResult = check_file_hash::<Cursor<&[u8]>>(None, "somehash");
        assert_eq!(result, HashCheckResult::Missing);
    }

    #[test]
    fn check_file_hash_match() {
        let data = b"Hello";
        let result = check_file_hash(Some(Cursor::new(data)), HELLO_HASH);
        assert_eq!(result, HashCheckResult::Match);
    }

    #[test]
    fn check_file_hash_mismatch() {
        let data = b"Hello";
        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let result = check_file_hash(Some(Cursor::new(data)), wrong_hash);
        match result {
            HashCheckResult::Mismatch { expected, actual } => {
                assert_eq!(expected, wrong_hash);
                assert_eq!(actual, HELLO_HASH);
            }
            _ => panic!("Expected Mismatch result"),
        }
    }

    // ========================================================================
    // Tests for HashCheckStats
    // ========================================================================

    #[test]
    fn hash_check_stats_default() {
        let stats = HashCheckStats::default();
        assert_eq!(stats.total(), 0);
        assert_eq!(stats.needs_update(), 0);
        assert!(stats.all_match());
    }

    #[test]
    fn hash_check_stats_total() {
        let stats = HashCheckStats {
            matched: 5,
            mismatched: 3,
            missing: 2,
            errors: 1,
        };
        assert_eq!(stats.total(), 11);
    }

    #[test]
    fn hash_check_stats_needs_update() {
        let stats = HashCheckStats {
            matched: 5,
            mismatched: 3,
            missing: 2,
            errors: 0,
        };
        assert_eq!(stats.needs_update(), 5);
    }

    #[test]
    fn hash_check_stats_all_match_true() {
        let stats = HashCheckStats {
            matched: 10,
            mismatched: 0,
            missing: 0,
            errors: 0,
        };
        assert!(stats.all_match());
    }

    #[test]
    fn hash_check_stats_all_match_false_mismatched() {
        let stats = HashCheckStats {
            matched: 10,
            mismatched: 1,
            missing: 0,
            errors: 0,
        };
        assert!(!stats.all_match());
    }

    #[test]
    fn hash_check_stats_all_match_false_missing() {
        let stats = HashCheckStats {
            matched: 10,
            mismatched: 0,
            missing: 1,
            errors: 0,
        };
        assert!(!stats.all_match());
    }

    #[test]
    fn hash_check_stats_all_match_false_errors() {
        let stats = HashCheckStats {
            matched: 10,
            mismatched: 0,
            missing: 0,
            errors: 1,
        };
        assert!(!stats.all_match());
    }

    // ========================================================================
    // Tests for aggregate_hash_results
    // ========================================================================

    #[test]
    fn aggregate_hash_results_empty() {
        let results: Vec<HashCheckResult> = vec![];
        let stats = aggregate_hash_results(results.iter());
        assert_eq!(stats.total(), 0);
    }

    #[test]
    fn aggregate_hash_results_all_match() {
        let results = [
            HashCheckResult::Match,
            HashCheckResult::Match,
            HashCheckResult::Match,
        ];
        let stats = aggregate_hash_results(results.iter());
        assert_eq!(stats.matched, 3);
        assert_eq!(stats.mismatched, 0);
        assert_eq!(stats.missing, 0);
        assert_eq!(stats.errors, 0);
        assert!(stats.all_match());
    }

    #[test]
    fn aggregate_hash_results_mixed() {
        let results = [
            HashCheckResult::Match,
            HashCheckResult::Match,
            HashCheckResult::Mismatch {
                expected: "a".to_string(),
                actual: "b".to_string(),
            },
            HashCheckResult::Missing,
            HashCheckResult::Missing,
            HashCheckResult::Error("test error".to_string()),
        ];
        let stats = aggregate_hash_results(results.iter());
        assert_eq!(stats.matched, 2);
        assert_eq!(stats.mismatched, 1);
        assert_eq!(stats.missing, 2);
        assert_eq!(stats.errors, 1);
        assert_eq!(stats.total(), 6);
        assert_eq!(stats.needs_update(), 3);
        assert!(!stats.all_match());
    }

    #[test]
    fn aggregate_hash_results_only_missing() {
        let results = [HashCheckResult::Missing, HashCheckResult::Missing];
        let stats = aggregate_hash_results(results.iter());
        assert_eq!(stats.matched, 0);
        assert_eq!(stats.missing, 2);
        assert!(!stats.all_match());
    }

    // ========================================================================
    // Hash determinism tests
    // ========================================================================

    #[test]
    fn hash_is_deterministic() {
        let data = b"The quick brown fox jumps over the lazy dog";
        let hash1 = calculate_hash_from_bytes(data);
        let hash2 = calculate_hash_from_bytes(data);
        let hash3 = calculate_hash_from_reader(Cursor::new(data)).unwrap();

        assert_eq!(hash1, hash2);
        assert_eq!(hash2, hash3);
    }

    #[test]
    fn different_data_different_hash() {
        let hash1 = calculate_hash_from_bytes(b"data1");
        let hash2 = calculate_hash_from_bytes(b"data2");
        assert_ne!(hash1, hash2);
    }

    // ========================================================================
    // Tests for error paths using failing reader
    // ========================================================================

    /// A reader that fails after reading a specified number of bytes.
    struct FailingReader {
        data: Vec<u8>,
        position: usize,
        fail_after: usize,
    }

    impl FailingReader {
        fn new(data: &[u8], fail_after: usize) -> Self {
            Self {
                data: data.to_vec(),
                position: 0,
                fail_after,
            }
        }
    }

    impl std::io::Read for FailingReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.position >= self.fail_after {
                return Err(std::io::Error::other("Simulated read failure"));
            }

            let remaining = self.data.len().saturating_sub(self.position);
            let to_read = buf
                .len()
                .min(remaining)
                .min(self.fail_after - self.position);

            if to_read == 0 {
                return Ok(0);
            }

            buf[..to_read].copy_from_slice(&self.data[self.position..self.position + to_read]);
            self.position += to_read;
            Ok(to_read)
        }
    }

    #[test]
    fn calculate_hash_from_reader_fails_on_read_error() {
        let reader = FailingReader::new(b"some data", 0); // Fail immediately
        let result = calculate_hash_from_reader(reader);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Failed to read"));
        assert!(err.contains("Simulated read failure"));
    }

    #[test]
    fn calculate_hash_from_reader_fails_mid_read() {
        // Create data larger than buffer size to ensure partial read before failure
        let data = vec![0u8; 1024];
        let reader = FailingReader::new(&data, 512); // Fail after 512 bytes
        let result = calculate_hash_from_reader(reader);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Failed to read"));
    }

    #[test]
    fn check_file_hash_returns_error_on_read_failure() {
        let reader = FailingReader::new(b"data", 0); // Fail immediately
        let result = check_file_hash(Some(reader), "somehash");

        match result {
            HashCheckResult::Error(e) => {
                assert!(e.contains("Failed to read"));
            }
            _ => panic!("Expected Error result, got {:?}", result),
        }
    }

    #[test]
    fn check_file_hash_error_needs_update() {
        let reader = FailingReader::new(b"data", 0);
        let result = check_file_hash(Some(reader), "somehash");

        assert!(result.needs_update());
        assert!(!result.is_match());
    }
}
