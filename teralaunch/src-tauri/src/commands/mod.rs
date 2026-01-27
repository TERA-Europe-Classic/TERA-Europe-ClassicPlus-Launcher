//! Tauri command handlers organized by domain
//!
//! This module provides the structure for organizing Tauri command handlers by functionality:
//!
//! - [`auth`] - Authentication commands (login, register, logout)
//! - [`config`] - Configuration commands (game path, language settings)
//! - [`download`] - Download commands (file downloads, progress tracking)
//! - [`game`] - Game launching commands (launch, status checks)
//! - [`hash`] - Hash verification commands (file checking, cache management)
//! - [`util`] - Utility commands (debug mode, logging, updates)

pub mod auth;
pub mod config;
pub mod download;
pub mod game;
pub mod hash;
pub mod util;

// Note: Commands are accessed via submodules (e.g., commands::auth::login)
// in tauri::generate_handler! macro which requires the original function paths.

/// Returns a list of all command names available.
///
/// This is useful for documentation and verification purposes.
#[allow(dead_code)]
pub fn list_commands() -> Vec<&'static str> {
    vec![
        // Auth
        "login",
        "register_new_account",
        "set_auth_info",
        "handle_logout",
        // Config
        "select_game_folder",
        "get_game_path_from_config",
        "save_game_path_to_config",
        "get_language_from_config",
        "save_language_to_config",
        // Download
        "download_all_files",
        "update_file",
        "cancel_downloads",
        "get_downloaded_bytes",
        // Game
        "handle_launch_game",
        "get_game_status",
        "reset_launch_state",
        // Hash
        "get_files_to_update",
        "check_update_required",
        "generate_hash_file",
        "clear_cache",
        // Util
        "is_debug",
        "set_logging",
        "update_launcher",
        "check_server_connection",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_commands_returns_expected_count() {
        let commands = list_commands();
        // 4 auth + 5 config + 4 download + 3 game + 4 hash + 4 util = 24 commands
        assert_eq!(commands.len(), 24);
    }

    #[test]
    fn list_commands_contains_expected_commands() {
        let commands = list_commands();
        assert!(commands.contains(&"login"));
        assert!(commands.contains(&"handle_launch_game"));
        assert!(commands.contains(&"download_all_files"));
        assert!(commands.contains(&"is_debug"));
    }
}
