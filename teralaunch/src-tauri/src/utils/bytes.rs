//! Byte and size calculation utilities.
//!
//! This module provides pure functions for byte/size operations, including:
//! - Resume offset calculations for partial downloads
//! - Human-readable byte formatting
//!
//! Note: `is_zero` is defined in `domain::models` as it's used by serde
//! for the `FileInfo` struct.

/// Calculates the resume offset for a partially downloaded file.
///
/// Returns the byte position from which to resume downloading:
/// - Returns the existing size if it represents a valid partial download
/// - Returns 0 if the file is missing, already complete, or corrupted
///
/// # Arguments
/// * `existing_size` - Current size of the local file
/// * `total_size` - Expected total size of the file
///
/// # Returns
/// The byte offset from which to resume downloading
///
/// # Examples
/// ```ignore
/// // Partial download - resume from existing position
/// assert_eq!(resume_offset(1024, 4096), 1024);
///
/// // No local file - start from beginning
/// assert_eq!(resume_offset(0, 4096), 0);
///
/// // Already complete - re-download from beginning
/// assert_eq!(resume_offset(4096, 4096), 0);
///
/// // Corrupted (larger than expected) - re-download
/// assert_eq!(resume_offset(8192, 4096), 0);
/// ```
pub fn resume_offset(existing_size: u64, total_size: u64) -> u64 {
    if existing_size == 0 || total_size == 0 || existing_size >= total_size {
        0
    } else {
        existing_size
    }
}

/// Formats a byte count into a human-readable string.
///
/// Automatically selects the appropriate unit (B, KB, MB, GB) and
/// formats with 2 decimal places.
///
/// # Arguments
/// * `bytes` - The byte count to format
///
/// # Returns
/// A formatted string with unit suffix
///
/// # Examples
/// ```ignore
/// assert_eq!(format_bytes(0), "0.00 B");
/// assert_eq!(format_bytes(1024), "1.00 KB");
/// assert_eq!(format_bytes(1048576), "1.00 MB");
/// assert_eq!(format_bytes(1073741824), "1.00 GB");
/// ```
#[allow(dead_code)]
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_index])
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Tests for resume_offset
    // ========================================================================

    #[test]
    fn resume_offset_returns_existing_when_partial() {
        assert_eq!(resume_offset(1024, 4096), 1024);
    }

    #[test]
    fn resume_offset_returns_zero_when_missing_or_full() {
        assert_eq!(resume_offset(0, 4096), 0);
        assert_eq!(resume_offset(4096, 4096), 0);
    }

    #[test]
    fn resume_offset_returns_zero_when_existing_exceeds_total() {
        assert_eq!(resume_offset(8192, 4096), 0);
    }

    #[test]
    fn resume_offset_zero_total_size() {
        assert_eq!(resume_offset(100, 0), 0);
        assert_eq!(resume_offset(0, 0), 0);
    }

    #[test]
    fn resume_offset_one_byte_partial() {
        // Just one byte remaining
        assert_eq!(resume_offset(4095, 4096), 4095);
    }

    #[test]
    fn resume_offset_one_byte_existing() {
        assert_eq!(resume_offset(1, 4096), 1);
    }

    // ========================================================================
    // Tests for format_bytes
    // ========================================================================

    #[test]
    fn format_bytes_zero() {
        assert_eq!(format_bytes(0), "0.00 B");
    }

    #[test]
    fn format_bytes_bytes_range() {
        assert_eq!(format_bytes(1), "1.00 B");
        assert_eq!(format_bytes(512), "512.00 B");
        assert_eq!(format_bytes(1023), "1023.00 B");
    }

    #[test]
    fn format_bytes_kb_range() {
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048575), "1024.00 KB");
    }

    #[test]
    fn format_bytes_mb_range() {
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1572864), "1.50 MB");
        assert_eq!(format_bytes(1073741823), "1024.00 MB");
    }

    #[test]
    fn format_bytes_gb_range() {
        assert_eq!(format_bytes(1073741824), "1.00 GB");
        assert_eq!(format_bytes(1610612736), "1.50 GB");
        // Very large value stays in GB
        assert_eq!(format_bytes(10737418240), "10.00 GB");
    }

    #[test]
    fn format_bytes_boundary_cases() {
        // Exactly at KB boundary
        assert_eq!(format_bytes(1024), "1.00 KB");
        // Exactly at MB boundary
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        // Exactly at GB boundary
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn format_bytes_large_values() {
        // Multiple GB
        assert_eq!(format_bytes(5 * 1024 * 1024 * 1024), "5.00 GB");
        // Very large (stays in GB since we don't have TB)
        assert_eq!(format_bytes(1024u64 * 1024 * 1024 * 1024), "1024.00 GB");
    }

    #[test]
    fn format_bytes_decimal_precision() {
        // 1.5 KB
        assert_eq!(format_bytes(1536), "1.50 KB");
        // 2.75 MB
        assert_eq!(format_bytes(2883584), "2.75 MB");
    }
}
