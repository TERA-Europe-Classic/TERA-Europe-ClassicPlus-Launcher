//! Game service for game launching and management.
//!
//! This module provides pure functions for game operations:
//! - Game path validation
//! - Executable verification
//! - Launch argument preparation

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// The name of the game executable
pub const GAME_EXECUTABLE: &str = "TERA.exe";

/// The subdirectory containing the game executable
pub const BINARIES_DIR: &str = "Binaries";

/// Result of validating a game installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameValidation {
    /// Game installation is valid
    Valid,
    /// Game path doesn't exist
    PathNotFound,
    /// Game path is not a directory
    NotDirectory,
    /// Binaries directory is missing
    BinariesMissing,
    /// Game executable is missing
    ExecutableMissing,
}

impl GameValidation {
    /// Returns true if the game installation is valid.
    pub fn is_valid(&self) -> bool {
        matches!(self, GameValidation::Valid)
    }

    /// Returns a human-readable error message.
    pub fn error_message(&self) -> Option<String> {
        match self {
            GameValidation::Valid => None,
            GameValidation::PathNotFound => Some("Game path does not exist".to_string()),
            GameValidation::NotDirectory => Some("Game path is not a directory".to_string()),
            GameValidation::BinariesMissing => {
                Some("Binaries directory not found in game folder".to_string())
            }
            GameValidation::ExecutableMissing => {
                Some(format!("{} not found in Binaries folder", GAME_EXECUTABLE))
            }
        }
    }
}

/// Validates a game installation path.
///
/// Checks that the path exists, is a directory, and contains the
/// required game executable.
///
/// # Arguments
/// * `game_path` - Path to the game installation directory
///
/// # Returns
/// Validation result
///
/// # Examples
/// ```ignore
/// let result = validate_game_installation(Path::new("C:/Games/TERA"));
/// if result.is_valid() {
///     println!("Game installation is valid");
/// } else {
///     println!("Error: {}", result.error_message().unwrap());
/// }
/// ```
pub fn validate_game_installation(game_path: &Path) -> GameValidation {
    if !game_path.exists() {
        return GameValidation::PathNotFound;
    }

    if !game_path.is_dir() {
        return GameValidation::NotDirectory;
    }

    let binaries_path = game_path.join(BINARIES_DIR);
    if !binaries_path.exists() {
        return GameValidation::BinariesMissing;
    }

    let executable_path = binaries_path.join(GAME_EXECUTABLE);
    if !executable_path.exists() {
        return GameValidation::ExecutableMissing;
    }

    GameValidation::Valid
}

/// Gets the full path to the game executable.
///
/// # Arguments
/// * `game_path` - Base game installation path
///
/// # Returns
/// Full path to TERA.exe
pub fn get_executable_path(game_path: &Path) -> PathBuf {
    game_path.join(BINARIES_DIR).join(GAME_EXECUTABLE)
}

/// Launch parameters for the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchParams {
    pub executable_path: PathBuf,
    pub account_name: String,
    pub character_count: String,
    pub ticket: String,
    pub language: String,
}

/// Builds launch parameters from authentication info.
///
/// # Arguments
/// * `game_path` - Path to game installation
/// * `account_name` - User account identifier
/// * `character_count` - Number of characters
/// * `ticket` - Authentication ticket
/// * `language` - Game language setting
///
/// # Returns
/// * `Ok(LaunchParams)` - Valid launch parameters
/// * `Err(String)` - Error if game installation is invalid
pub fn build_launch_params(
    game_path: &Path,
    account_name: String,
    character_count: String,
    ticket: String,
    language: String,
) -> Result<LaunchParams, String> {
    let validation = validate_game_installation(game_path);
    if !validation.is_valid() {
        return Err(validation.error_message().unwrap());
    }

    Ok(LaunchParams {
        executable_path: get_executable_path(game_path),
        account_name,
        character_count,
        ticket,
        language,
    })
}

/// Checks if the game is likely already running.
///
/// This is a pure function that takes the process list as input
/// rather than querying the system directly.
///
/// # Arguments
/// * `running_processes` - List of currently running process names
///
/// # Returns
/// `true` if TERA.exe is in the process list
pub fn is_game_running(running_processes: &[&str]) -> bool {
    running_processes
        .iter()
        .any(|p| p.eq_ignore_ascii_case(GAME_EXECUTABLE))
}

/// Validates launch preconditions.
///
/// Checks all requirements before launching the game.
///
/// # Arguments
/// * `is_already_launching` - Whether a launch is already in progress
/// * `is_already_running` - Whether the game is already running
/// * `auth_key` - The authentication key (must not be empty)
///
/// # Returns
/// * `Ok(())` - If all preconditions are met
/// * `Err(String)` - Error message describing what's wrong
pub fn validate_launch_preconditions(
    is_already_launching: bool,
    is_already_running: bool,
    auth_key: &str,
) -> Result<(), String> {
    if is_already_launching {
        return Err("Game is already launching".to_string());
    }

    if is_already_running {
        return Err("Game is already running".to_string());
    }

    if auth_key.is_empty() {
        return Err("Not authenticated. Please log in first.".to_string());
    }

    Ok(())
}

/// Game launch state for UI feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    /// Game is not running and not launching
    Idle,
    /// Game is in the process of launching
    Launching,
    /// Game is running
    Running,
}

impl GameState {
    /// Returns true if a new launch can be initiated.
    pub fn can_launch(&self) -> bool {
        matches!(self, GameState::Idle)
    }

    /// Combines launching and running flags into a state.
    pub fn from_flags(is_launching: bool, is_running: bool) -> Self {
        if is_running {
            GameState::Running
        } else if is_launching {
            GameState::Launching
        } else {
            GameState::Idle
        }
    }
}

/// Formats a game exit status for display.
///
/// # Arguments
/// * `exit_code` - The process exit code
///
/// # Returns
/// Human-readable status message
pub fn format_exit_status(exit_code: Option<i32>) -> String {
    match exit_code {
        Some(0) => "Game exited normally".to_string(),
        Some(code) => format!("Game exited with code: {}", code),
        None => "Game process terminated".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ========================================================================
    // Tests for GameValidation
    // ========================================================================

    #[test]
    fn game_validation_is_valid() {
        assert!(GameValidation::Valid.is_valid());
        assert!(!GameValidation::PathNotFound.is_valid());
        assert!(!GameValidation::NotDirectory.is_valid());
        assert!(!GameValidation::BinariesMissing.is_valid());
        assert!(!GameValidation::ExecutableMissing.is_valid());
    }

    #[test]
    fn game_validation_error_messages() {
        assert!(GameValidation::Valid.error_message().is_none());
        assert!(GameValidation::PathNotFound.error_message().is_some());
        assert!(GameValidation::NotDirectory.error_message().is_some());
        assert!(GameValidation::BinariesMissing.error_message().is_some());
        assert!(GameValidation::ExecutableMissing.error_message().is_some());
    }

    #[test]
    fn game_validation_error_message_content() {
        let msg = GameValidation::ExecutableMissing.error_message().unwrap();
        assert!(msg.contains(GAME_EXECUTABLE));
    }

    // ========================================================================
    // Tests for validate_game_installation
    // ========================================================================

    #[test]
    fn validate_game_installation_nonexistent() {
        let path = Path::new("/nonexistent/path/that/does/not/exist");
        let result = validate_game_installation(path);
        assert_eq!(result, GameValidation::PathNotFound);
    }

    #[test]
    fn validate_game_installation_file_not_dir() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_validate_game_file.txt");
        fs::write(&temp_file, "test").unwrap();

        let result = validate_game_installation(&temp_file);
        assert_eq!(result, GameValidation::NotDirectory);

        fs::remove_file(&temp_file).ok();
    }

    #[test]
    fn validate_game_installation_no_binaries() {
        let temp_dir = std::env::temp_dir().join("test_validate_game_no_bin");
        fs::create_dir_all(&temp_dir).unwrap();

        let result = validate_game_installation(&temp_dir);
        assert_eq!(result, GameValidation::BinariesMissing);

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn validate_game_installation_no_executable() {
        let temp_dir = std::env::temp_dir().join("test_validate_game_no_exe");
        let binaries_dir = temp_dir.join(BINARIES_DIR);
        fs::create_dir_all(&binaries_dir).unwrap();

        let result = validate_game_installation(&temp_dir);
        assert_eq!(result, GameValidation::ExecutableMissing);

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn validate_game_installation_valid() {
        let temp_dir = std::env::temp_dir().join("test_validate_game_valid");
        let binaries_dir = temp_dir.join(BINARIES_DIR);
        fs::create_dir_all(&binaries_dir).unwrap();
        fs::write(binaries_dir.join(GAME_EXECUTABLE), "dummy").unwrap();

        let result = validate_game_installation(&temp_dir);
        assert_eq!(result, GameValidation::Valid);
        assert!(result.is_valid());

        fs::remove_dir_all(&temp_dir).ok();
    }

    // ========================================================================
    // Tests for get_executable_path
    // ========================================================================

    #[test]
    fn get_executable_path_format() {
        let game_path = Path::new("/games/tera");
        let exe_path = get_executable_path(game_path);

        assert!(exe_path.to_string_lossy().contains(BINARIES_DIR));
        assert!(exe_path.to_string_lossy().contains(GAME_EXECUTABLE));
    }

    #[test]
    fn get_executable_path_windows() {
        let game_path = Path::new("C:/Games/TERA");
        let exe_path = get_executable_path(game_path);

        // Path should end with correct components
        assert!(
            exe_path.ends_with(Path::new("Binaries/TERA.exe"))
                || exe_path.ends_with(Path::new("Binaries\\TERA.exe"))
        );
    }

    // ========================================================================
    // Tests for build_launch_params
    // ========================================================================

    #[test]
    fn build_launch_params_invalid_path() {
        let path = Path::new("/nonexistent/path");
        let result = build_launch_params(
            path,
            "account".to_string(),
            "1".to_string(),
            "ticket".to_string(),
            "EUR".to_string(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn build_launch_params_valid() {
        let temp_dir = std::env::temp_dir().join("test_build_launch_params");
        let binaries_dir = temp_dir.join(BINARIES_DIR);
        fs::create_dir_all(&binaries_dir).unwrap();
        fs::write(binaries_dir.join(GAME_EXECUTABLE), "dummy").unwrap();

        let result = build_launch_params(
            &temp_dir,
            "account".to_string(),
            "3".to_string(),
            "ticket123".to_string(),
            "EUR".to_string(),
        );

        assert!(result.is_ok());
        let params = result.unwrap();
        assert_eq!(params.account_name, "account");
        assert_eq!(params.character_count, "3");
        assert_eq!(params.ticket, "ticket123");
        assert_eq!(params.language, "EUR");
        assert!(params
            .executable_path
            .to_string_lossy()
            .contains(GAME_EXECUTABLE));

        fs::remove_dir_all(&temp_dir).ok();
    }

    // ========================================================================
    // Tests for is_game_running
    // ========================================================================

    #[test]
    fn is_game_running_not_in_list() {
        let processes = vec!["explorer.exe", "chrome.exe", "code.exe"];
        assert!(!is_game_running(&processes));
    }

    #[test]
    fn is_game_running_in_list() {
        let processes = vec!["explorer.exe", "TERA.exe", "chrome.exe"];
        assert!(is_game_running(&processes));
    }

    #[test]
    fn is_game_running_case_insensitive() {
        let processes = vec!["tera.exe"];
        assert!(is_game_running(&processes));

        let processes = vec!["TERA.EXE"];
        assert!(is_game_running(&processes));

        let processes = vec!["TeRa.ExE"];
        assert!(is_game_running(&processes));
    }

    #[test]
    fn is_game_running_empty_list() {
        let processes: Vec<&str> = vec![];
        assert!(!is_game_running(&processes));
    }

    // ========================================================================
    // Tests for validate_launch_preconditions
    // ========================================================================

    #[test]
    fn validate_launch_preconditions_valid() {
        let result = validate_launch_preconditions(false, false, "auth_key");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_launch_preconditions_already_launching() {
        let result = validate_launch_preconditions(true, false, "auth_key");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already launching"));
    }

    #[test]
    fn validate_launch_preconditions_already_running() {
        let result = validate_launch_preconditions(false, true, "auth_key");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already running"));
    }

    #[test]
    fn validate_launch_preconditions_no_auth() {
        let result = validate_launch_preconditions(false, false, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("log in"));
    }

    #[test]
    fn validate_launch_preconditions_priority() {
        // Already launching takes priority over already running
        let result = validate_launch_preconditions(true, true, "auth_key");
        assert!(result.unwrap_err().contains("already launching"));
    }

    // ========================================================================
    // Tests for GameState
    // ========================================================================

    #[test]
    fn game_state_can_launch() {
        assert!(GameState::Idle.can_launch());
        assert!(!GameState::Launching.can_launch());
        assert!(!GameState::Running.can_launch());
    }

    #[test]
    fn game_state_from_flags() {
        assert_eq!(GameState::from_flags(false, false), GameState::Idle);
        assert_eq!(GameState::from_flags(true, false), GameState::Launching);
        assert_eq!(GameState::from_flags(false, true), GameState::Running);
        // Running takes priority over launching
        assert_eq!(GameState::from_flags(true, true), GameState::Running);
    }

    // ========================================================================
    // Tests for format_exit_status
    // ========================================================================

    #[test]
    fn format_exit_status_normal() {
        let status = format_exit_status(Some(0));
        assert!(status.contains("normally"));
    }

    #[test]
    fn format_exit_status_error_code() {
        let status = format_exit_status(Some(1));
        assert!(status.contains("1"));

        let status = format_exit_status(Some(-1));
        assert!(status.contains("-1"));
    }

    #[test]
    fn format_exit_status_none() {
        let status = format_exit_status(None);
        assert!(status.contains("terminated"));
    }
}
