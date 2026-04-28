//! Global state management for the launcher.
//!
//! This module provides centralized, thread-safe access to global application state
//! including authentication information and download progress tracking.

pub mod auth_state;
pub mod deep_link_state;
pub mod download_state;
pub mod mods_state;

// Re-export commonly used functions for convenience
pub use auth_state::{
    clear_auth_client, clear_auth_info, clear_website_auth_client, get_auth_client,
    get_website_auth_client, read_auth_info, set_auth_client, set_auth_info,
    set_website_auth_client,
};
pub use deep_link_state::set_pending_deep_link;
pub use download_state::{
    add_downloaded_bytes, cancel_download, clear_hash_cache, get_current_file_name,
    get_download_generation, get_downloaded_bytes, increment_download_generation,
    is_download_cancelled, is_download_complete, reset_download_state, set_current_file_name,
    set_download_cancelled, set_download_complete, set_download_in_progress, set_downloaded_bytes,
    sub_downloaded_bytes, try_start_download,
};
