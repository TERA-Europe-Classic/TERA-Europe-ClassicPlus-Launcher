//! Game launching and state management Tauri commands
//!
//! This module contains commands for:
//! - Launching the game
//! - Checking game running status
//! - Resetting launch state

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use log::{error, info};
use tauri::Emitter;

use crate::commands::config::load_config;
use crate::domain::GlobalAuthInfo;
use crate::services::game_service;
use crate::state::read_auth_info;
use crate::GameState;
use teralib::{reset_global_state, run_game};

// ============================================================================
// Inner testable functions
// ============================================================================

/// Result of validating auth info for game launch.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedAuthInfo {
    pub account_name: String,
    pub characters_count: String,
    pub ticket: String,
}

/// Validates that auth info is complete and returns extracted values.
///
/// # Arguments
/// * `auth_info` - The global auth info to validate
///
/// # Returns
/// Validated auth info with extracted fields, or an error message
pub fn validate_auth_info(auth_info: &GlobalAuthInfo) -> Result<ValidatedAuthInfo, String> {
    if auth_info.user_no == 0 {
        return Err("User not authenticated (user_no is 0)".to_string());
    }
    if auth_info.auth_key.is_empty() {
        return Err("Auth key is missing".to_string());
    }

    Ok(ValidatedAuthInfo {
        account_name: auth_info.user_no.to_string(),
        characters_count: auth_info.character_count.clone(),
        ticket: auth_info.auth_key.clone(),
    })
}

/// Validates the game path and returns the full executable path.
///
/// # Arguments
/// * `game_path` - The base game installation directory
///
/// # Returns
/// The full path to TERA.exe if valid, or an error message
pub fn validate_game_path(game_path: &Path) -> Result<PathBuf, String> {
    let executable_path = game_service::get_executable_path(game_path);
    let validation = game_service::validate_game_installation(game_path);
    if !validation.is_valid() {
        return Err(format!(
            "Game executable not found at: {:?}",
            executable_path
        ));
    }
    Ok(executable_path)
}

/// Converts a path to a string suitable for passing to run_game.
///
/// # Arguments
/// * `path` - The path to convert
///
/// # Returns
/// The path as a string, or an error if the path contains invalid UTF-8
pub fn path_to_string(path: &Path) -> Result<String, String> {
    path.to_str()
        .ok_or_else(|| "Invalid path to game executable".to_string())
        .map(|s| s.to_string())
}

/// Computes the combined game status from individual status flags.
///
/// # Arguments
/// * `is_running` - Whether the game process is currently running
/// * `is_launching` - Whether the game is currently being launched
///
/// # Returns
/// `true` if the game is either running or launching
pub fn compute_game_status(is_running: bool, is_launching: bool) -> bool {
    let state = game_service::GameState::from_flags(is_launching, is_running);
    !state.can_launch()
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// Launches the TERA game with the current user's credentials.
///
/// This command:
/// 1. Checks if the game is already launching/running
/// 2. Reads auth info and game config
/// 3. Launches the game executable
/// 4. Emits events for game status changes
///
/// # Arguments
/// * `app_handle` - The Tauri app handle for emitting events
/// * `state` - The game state containing launch status
///
/// # Returns
/// Success message or error description
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn handle_launch_game(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, GameState>,
) -> Result<String, String> {
    info!("Total time: {:?}", 3);
    let is_running = *state.status_receiver.lock().await.borrow();

    let (account_name, characters_count, ticket, auth_key, user_no, _user_name) = {
        let auth_info = read_auth_info();
        let account_name = auth_info.user_no.to_string();
        let characters_count = auth_info.character_count.clone();
        let ticket = auth_info.auth_key.clone();
        let auth_key = auth_info.auth_key.clone();
        let user_no = auth_info.user_no;
        let user_name = auth_info.user_name.clone();
        (
            account_name,
            characters_count,
            ticket,
            auth_key,
            user_no,
            user_name,
        )
    };

    // Multi-client support: Per-account tracking is handled by frontend.
    // Game running state is tracked by teralib via PID-based credentials map.
    game_service::validate_launch_preconditions(false, is_running, &auth_key)?;
    if user_no == 0 {
        return Err("User not authenticated (user_no is 0)".to_string());
    }

    // Note: Auth key refresh is handled by the frontend which performs a fresh login
    // before every game launch. The ticket stored in auth state is already fresh.

    let (game_path, game_lang) = load_config()?;
    let executable_path = game_service::get_executable_path(&game_path);
    let launch_params = game_service::build_launch_params(
        &game_path,
        account_name.clone(),
        characters_count.clone(),
        ticket.clone(),
        game_lang.clone(),
    )
    .map_err(|_| format!("Game executable not found at: {:?}", executable_path))?;

    let full_game_path_str = path_to_string(&launch_params.executable_path)?;

    let app_handle_clone = app_handle.clone();
    let user_no_for_event = user_no; // Capture for game_ended event

    tokio::task::spawn(async move {
        // Emit the game_status_changed event at the start of the launch
        if let Err(e) = app_handle_clone.emit("game_status_changed", true) {
            error!("Failed to emit game_status_changed event: {:?}", e);
        }

        // Spawn any enabled external-app mods (Shinra Meter, TCC) with
        // auto-launch on. Intentionally fire-and-forget — a failing overlay
        // must never block the game launch itself.
        crate::commands::mods::spawn_auto_launch_external_apps();

        info!("run_game reached");

        match run_game(
            &account_name,
            &characters_count,
            &ticket,
            &game_lang,
            &full_game_path_str,
        )
        .await
        {
            Ok(exit_status) => {
                let result = format!("Game exited with status: {:?}", exit_status);
                if let Err(e) = app_handle_clone.emit("game_status", &result) {
                    error!("Failed to emit game_status event: {:?}", e);
                }
                info!("{}", result);
            }
            Err(e) => {
                let error = format!("Error launching game: {:?}", e);
                if let Err(emit_err) = app_handle_clone.emit("game_status", &error) {
                    error!("Failed to emit game_status event: {:?}", emit_err);
                }
                error!("{}", error);
            }
        }

        info!(
            "Emitting game_ended event for user_no: {}",
            user_no_for_event
        );
        if let Err(e) = app_handle_clone.emit("game_ended", user_no_for_event) {
            error!("Failed to emit game_ended event: {:?}", e);
        }

        // Overlay lifecycle (PRD 3.2.12 / 3.2.13, fix.overlay-lifecycle-
        // wiring). If another TERA client is still running, leave the
        // external-app overlays (Shinra / TCC) attached to it — only tear
        // them down when the last client has exited. The pure predicate
        // `decide_overlay_action` carries the policy; the wiring lives
        // here so the policy decision is colocated with the close event.
        use crate::services::mods::external_app::{decide_overlay_action, OverlayLifecycleAction};
        let remaining_clients = teralib::get_running_game_count();
        if decide_overlay_action(remaining_clients) == OverlayLifecycleAction::Terminate {
            crate::commands::mods::stop_auto_launched_external_apps();
        }

        // Game status is tracked by teralib via PID map - it sends updates via watch channel
        // Check if all games have finished
        if !teralib::is_game_running() {
            if let Err(e) = app_handle_clone.emit("game_status_changed", false) {
                error!("Failed to emit game_status_changed event: {:?}", e);
            }
            info!("All game instances finished");
        }
    });

    Ok("Game launch initiated".to_string())
}

/// Checks if any game is currently running.
/// Uses teralib's PID-based tracking as the source of truth.
///
/// # Returns
/// `true` if any game is running, `false` otherwise
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn get_game_status(_state: tauri::State<'_, GameState>) -> Result<bool, String> {
    Ok(teralib::is_game_running())
}

/// Returns the number of currently running game instances.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn get_running_game_count(_state: tauri::State<'_, GameState>) -> Result<usize, String> {
    Ok(teralib::get_running_game_count())
}

/// Resets the game state.
/// Used to recover from stuck states.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn reset_launch_state(_state: tauri::State<'_, GameState>) -> Result<(), String> {
    reset_global_state();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ========================================================================
    // Auth Info Validation Tests
    // ========================================================================

    #[test]
    fn validate_auth_info_success() {
        let auth_info = GlobalAuthInfo {
            user_no: 12345,
            user_name: "TestUser".to_string(),
            auth_key: "test-auth-key-abc123".to_string(),
            character_count: "5".to_string(),
        };

        let result = validate_auth_info(&auth_info);
        assert!(result.is_ok());

        let validated = result.unwrap();
        assert_eq!(validated.account_name, "12345");
        assert_eq!(validated.characters_count, "5");
        assert_eq!(validated.ticket, "test-auth-key-abc123");
    }

    #[test]
    fn validate_auth_info_zero_user_no() {
        let auth_info = GlobalAuthInfo {
            user_no: 0,
            user_name: "TestUser".to_string(),
            auth_key: "test-auth-key".to_string(),
            character_count: "5".to_string(),
        };

        let result = validate_auth_info(&auth_info);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "User not authenticated (user_no is 0)");
    }

    #[test]
    fn validate_auth_info_empty_auth_key() {
        let auth_info = GlobalAuthInfo {
            user_no: 12345,
            user_name: "TestUser".to_string(),
            auth_key: "".to_string(),
            character_count: "5".to_string(),
        };

        let result = validate_auth_info(&auth_info);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Auth key is missing");
    }

    #[test]
    fn validate_auth_info_default_fails() {
        let auth_info = GlobalAuthInfo::default();

        let result = validate_auth_info(&auth_info);
        assert!(result.is_err());
        // Default has user_no = 0, so that error takes precedence
        assert_eq!(result.unwrap_err(), "User not authenticated (user_no is 0)");
    }

    #[test]
    fn validate_auth_info_empty_character_count_allowed() {
        // Empty character count should be allowed (new account with no chars)
        let auth_info = GlobalAuthInfo {
            user_no: 12345,
            user_name: "TestUser".to_string(),
            auth_key: "valid-key".to_string(),
            character_count: "".to_string(),
        };

        let result = validate_auth_info(&auth_info);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().characters_count, "");
    }

    #[test]
    fn validate_auth_info_empty_username_allowed() {
        // Empty username should be allowed (we use user_no for launch)
        let auth_info = GlobalAuthInfo {
            user_no: 12345,
            user_name: "".to_string(),
            auth_key: "valid-key".to_string(),
            character_count: "5".to_string(),
        };

        let result = validate_auth_info(&auth_info);
        assert!(result.is_ok());
    }

    // ========================================================================
    // Game Path Validation Tests
    // ========================================================================

    #[test]
    fn validate_game_path_success() {
        // Create a temporary directory structure with TERA.exe
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let binaries_dir = temp_dir.path().join("Binaries");
        fs::create_dir_all(&binaries_dir).expect("Failed to create Binaries dir");

        let tera_exe = binaries_dir.join("TERA.exe");
        fs::write(&tera_exe, "dummy executable").expect("Failed to create TERA.exe");

        let result = validate_game_path(temp_dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), tera_exe);
    }

    #[test]
    fn validate_game_path_missing_executable() {
        // Create a directory without TERA.exe
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let binaries_dir = temp_dir.path().join("Binaries");
        fs::create_dir_all(&binaries_dir).expect("Failed to create Binaries dir");

        let result = validate_game_path(temp_dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Game executable not found"));
    }

    #[test]
    fn validate_game_path_missing_binaries_dir() {
        // Create a directory without Binaries folder
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        let result = validate_game_path(temp_dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Game executable not found"));
    }

    #[test]
    fn validate_game_path_nonexistent_base() {
        let nonexistent = PathBuf::from("/nonexistent/game/path");

        let result = validate_game_path(&nonexistent);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Game executable not found"));
    }

    // ========================================================================
    // Path to String Conversion Tests
    // ========================================================================

    #[test]
    fn path_to_string_valid_path() {
        let path = PathBuf::from("/some/valid/path/TERA.exe");
        let result = path_to_string(&path);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "/some/valid/path/TERA.exe");
    }

    #[test]
    fn path_to_string_windows_path() {
        let path = PathBuf::from("C:\\Games\\TERA\\Binaries\\TERA.exe");
        let result = path_to_string(&path);
        assert!(result.is_ok());
        // On Windows, this preserves backslashes
        assert!(result.unwrap().contains("TERA.exe"));
    }

    #[test]
    fn path_to_string_with_spaces() {
        let path = PathBuf::from("/path/with spaces/to/TERA.exe");
        let result = path_to_string(&path);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("with spaces"));
    }

    // ========================================================================
    // Game Status Computation Tests
    // ========================================================================

    #[test]
    fn compute_game_status_both_false() {
        assert!(!compute_game_status(false, false));
    }

    #[test]
    fn compute_game_status_only_running() {
        assert!(compute_game_status(true, false));
    }

    #[test]
    fn compute_game_status_only_launching() {
        assert!(compute_game_status(false, true));
    }

    #[test]
    fn compute_game_status_both_true() {
        assert!(compute_game_status(true, true));
    }

    // ========================================================================
    // ValidatedAuthInfo Struct Tests
    // ========================================================================

    #[test]
    fn validated_auth_info_equality() {
        let info1 = ValidatedAuthInfo {
            account_name: "123".to_string(),
            characters_count: "5".to_string(),
            ticket: "abc".to_string(),
        };
        let info2 = ValidatedAuthInfo {
            account_name: "123".to_string(),
            characters_count: "5".to_string(),
            ticket: "abc".to_string(),
        };
        assert_eq!(info1, info2);
    }

    #[test]
    fn validated_auth_info_inequality() {
        let info1 = ValidatedAuthInfo {
            account_name: "123".to_string(),
            characters_count: "5".to_string(),
            ticket: "abc".to_string(),
        };
        let info2 = ValidatedAuthInfo {
            account_name: "456".to_string(),
            characters_count: "5".to_string(),
            ticket: "abc".to_string(),
        };
        assert_ne!(info1, info2);
    }

    #[test]
    fn validated_auth_info_clone() {
        let info = ValidatedAuthInfo {
            account_name: "123".to_string(),
            characters_count: "5".to_string(),
            ticket: "abc".to_string(),
        };
        let cloned = info.clone();
        assert_eq!(info, cloned);
    }

    #[test]
    fn validated_auth_info_debug() {
        let info = ValidatedAuthInfo {
            account_name: "123".to_string(),
            characters_count: "5".to_string(),
            ticket: "abc".to_string(),
        };
        let debug_str = format!("{:?}", info);
        assert!(debug_str.contains("ValidatedAuthInfo"));
        assert!(debug_str.contains("123"));
    }

    // ========================================================================
    // Integration-Style Tests (Combining Validations)
    // ========================================================================

    #[test]
    fn full_validation_flow_success() {
        // Create a realistic game directory
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let binaries_dir = temp_dir.path().join("Binaries");
        fs::create_dir_all(&binaries_dir).expect("Failed to create Binaries dir");
        fs::write(binaries_dir.join("TERA.exe"), "exe").expect("Failed to create TERA.exe");

        // Create valid auth info
        let auth_info = GlobalAuthInfo {
            user_no: 99999,
            user_name: "IntegrationTestUser".to_string(),
            auth_key: "integration-test-key".to_string(),
            character_count: "10".to_string(),
        };

        // Validate auth
        let validated_auth = validate_auth_info(&auth_info);
        assert!(validated_auth.is_ok());

        // Validate game path
        let game_exe = validate_game_path(temp_dir.path());
        assert!(game_exe.is_ok());

        // Convert to string
        let exe_str = path_to_string(&game_exe.unwrap());
        assert!(exe_str.is_ok());
        assert!(exe_str.unwrap().contains("TERA.exe"));
    }

    #[test]
    fn full_validation_flow_auth_fails() {
        // Create valid game directory
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let binaries_dir = temp_dir.path().join("Binaries");
        fs::create_dir_all(&binaries_dir).expect("Failed to create Binaries dir");
        fs::write(binaries_dir.join("TERA.exe"), "exe").expect("Failed to create TERA.exe");

        // Create invalid auth info (not logged in)
        let auth_info = GlobalAuthInfo::default();

        // Auth validation should fail first
        let validated_auth = validate_auth_info(&auth_info);
        assert!(validated_auth.is_err());

        // Game path would be valid but we don't get there
        let game_exe = validate_game_path(temp_dir.path());
        assert!(game_exe.is_ok());
    }

    #[test]
    fn full_validation_flow_game_path_fails() {
        // No game directory
        let invalid_path = PathBuf::from("/this/path/does/not/exist");

        // Create valid auth info
        let auth_info = GlobalAuthInfo {
            user_no: 12345,
            user_name: "User".to_string(),
            auth_key: "key".to_string(),
            character_count: "1".to_string(),
        };

        // Auth validation succeeds
        let validated_auth = validate_auth_info(&auth_info);
        assert!(validated_auth.is_ok());

        // Game path validation fails
        let game_exe = validate_game_path(&invalid_path);
        assert!(game_exe.is_err());
    }
}
