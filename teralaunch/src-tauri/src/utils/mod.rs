//! Utility modules for the TERA launcher.
//!
//! This module provides pure utility functions organized by domain:
//!
//! - [`path`]: Path validation and manipulation
//! - [`bytes`]: Byte/size calculations and formatting
//! - [`retry`]: Retry logic and timeout helpers

pub mod bytes;
pub mod path;
pub mod retry;

// Re-export commonly used items for convenience
pub use bytes::resume_offset;
pub use path::{is_ignored, normalize_path_for_compare, validate_path_within_base};
pub use retry::{is_transient_download_error, retry_delay_ms, stall_exceeded};
