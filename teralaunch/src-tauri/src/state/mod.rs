//! Global state management for the launcher.
//!
//! This module provides centralized, thread-safe access to global application state
//! including authentication information and download progress tracking.

pub mod auth_state;
pub mod download_state;

// Re-export commonly used functions for convenience
pub use auth_state::{clear_auth_info, read_auth_info, set_auth_info};
pub use download_state::{
    add_downloaded_bytes, cancel_download, clear_hash_cache, get_current_file_name,
    get_downloaded_bytes, is_download_cancelled, set_current_file_name, set_download_cancelled,
    set_downloaded_bytes, sub_downloaded_bytes,
};
