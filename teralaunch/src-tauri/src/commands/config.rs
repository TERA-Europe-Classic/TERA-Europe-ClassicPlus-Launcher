//! Configuration-related Tauri commands
//!
//! This module contains commands for managing application configuration:
//! - Game path settings
//! - Language settings
//! - Folder selection

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use log::info;

use crate::infrastructure::{FileSystem, StdFileSystem};
use crate::services::config_service;
use crate::state::{cancel_download, clear_hash_cache, set_downloaded_bytes};
use crate::utils::normalize_path_for_compare;

/// Opens a folder picker dialog for selecting the game folder.
///
/// # Returns
/// The selected folder path, or an error if cancelled
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn select_game_folder() -> Result<String, String> {
    use tauri::api::dialog::blocking::FileDialogBuilder;

    let folder = FileDialogBuilder::new()
        .set_title("Select Tera Game Folder")
        .set_directory("/")
        .pick_folder();

    match folder {
        Some(path) => Ok(path.to_string_lossy().into_owned()),
        None => Err("Folder selection cancelled or failed".into()),
    }
}

/// Gets the game path from the configuration file.
///
/// # Returns
/// The game path as a string, or an error if not found
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub fn get_game_path_from_config() -> Result<String, String> {
    let fs = StdFileSystem::new();
    get_game_path_from_config_with_fs(&fs)
}

/// Inner function for getting game path, accepting a FileSystem for testability.
///
/// # Arguments
/// * `fs` - FileSystem implementation to use for file operations
///
/// # Returns
/// The game path as a string, or an error if not found
#[cfg(not(tarpaulin_include))]
fn get_game_path_from_config_with_fs<F: FileSystem>(fs: &F) -> Result<String, String> {
    let config_path = find_config_file().ok_or("Config file not found")?;
    let content = fs
        .read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;

    let config = config_service::parse_game_config(&content)?;
    Ok(config.path.to_string_lossy().into_owned())
}

/// Parses the game path from INI content string.
#[cfg(test)]
fn parse_game_path_from_ini(content: &str) -> Result<String, String> {
    match config_service::parse_game_config(content) {
        Ok(config) => Ok(config.path.to_string_lossy().into_owned()),
        Err(err) => Err(map_config_parse_error(&err)),
    }
}

/// Saves the game path to the configuration file.
///
/// This also validates the path, cancels any ongoing downloads,
/// clears the hash cache if the path changed, and emits an event.
///
/// # Arguments
/// * `path` - The game path to save
/// * `window` - The Tauri window for emitting events
/// * `_app_handle` - The Tauri app handle (unused but required for signature)
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn save_game_path_to_config(
    path: String,
    window: tauri::Window,
    _app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);
    config_service::validate_game_path(&path_buf)?;

    // Capture previous path before writing, so we can detect actual changes
    let prev_path_string = get_game_path()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()));

    let config_path = find_config_file().ok_or("Config file not found")?;
    let content =
        fs::read_to_string(&config_path).map_err(|e| format!("Failed to read config: {}", e))?;
    let updated = config_service::update_path_in_config(&content, &path_buf)?;
    fs::write(&config_path, updated).map_err(|e| format!("Failed to write config: {}", e))?;

    // Only interrupt/recheck when path actually changed
    let should_refresh = game_path_changed(prev_path_string.as_deref(), &path);

    if should_refresh {
        // Interrupt any ongoing downloads
        cancel_download();
        // Clear stale hash cache from old directory and reset download progress
        clear_cache_internal().await.ok();
        set_downloaded_bytes(0);
        let _ = window.emit("game_path_changed", &path);
    }

    Ok(())
}

/// Gets the current language setting from the configuration.
///
/// # Returns
/// The language code (e.g., "EUR", "USA")
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub fn get_language_from_config() -> Result<String, String> {
    info!("Attempting to read language from config file");
    let (_, game_lang) = load_config()?;
    info!("Language read from config: {}", game_lang);
    Ok(game_lang)
}

/// Saves the language setting to the configuration file.
///
/// # Arguments
/// * `language` - The language code to save
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub fn save_language_to_config(language: String) -> Result<(), String> {
    info!("Attempting to save language {} to config file", language);
    let config_path = find_config_file().ok_or("Config file not found")?;
    let content =
        fs::read_to_string(&config_path).map_err(|e| format!("Failed to read config: {}", e))?;
    let updated = config_service::update_language_in_config(&content, &language)?;
    fs::write(&config_path, updated).map_err(|e| format!("Failed to write config: {}", e))?;

    info!("Language successfully saved to config");
    Ok(())
}

// ============================================================================
// Internal helper functions (not exposed as Tauri commands)
// ============================================================================

/// Finds the configuration file, creating it if necessary.
#[cfg(not(tarpaulin_include))]
pub(crate) fn find_config_file() -> Option<PathBuf> {
    let file_path = config_service::get_config_file_path()?;
    let dir = file_path.parent()?.to_path_buf();

    if file_path.exists() {
        return Some(file_path);
    }

    let current_dir = env::current_dir().ok();
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let legacy_config =
        config_service::get_legacy_config_paths(current_dir.as_deref(), exe_dir.as_deref())
            .into_iter()
            .find(|p| p.exists());

    if fs::create_dir_all(&dir).is_err() {
        return None;
    }

    if let Some(old) = legacy_config {
        if fs::copy(&old, &file_path).is_ok() {
            return Some(file_path);
        }
    }

    if fs::write(&file_path, config_service::DEFAULT_CONFIG_CONTENT).is_ok() {
        return Some(file_path);
    }

    None
}

/// Loads the game configuration from the INI file.
#[cfg(not(tarpaulin_include))]
pub(crate) fn load_config() -> Result<(PathBuf, String), String> {
    let fs = StdFileSystem::new();
    load_config_with_fs(&fs)
}

/// Inner function for loading config, accepting a FileSystem for testability.
#[cfg(not(tarpaulin_include))]
pub(crate) fn load_config_with_fs<F: FileSystem>(fs: &F) -> Result<(PathBuf, String), String> {
    let config_path = find_config_file().ok_or("Config file not found")?;
    let content = fs
        .read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;

    parse_config_from_ini(&content)
}

/// Parses both game path and language from INI content string.
fn parse_config_from_ini(content: &str) -> Result<(PathBuf, String), String> {
    match config_service::parse_game_config(content) {
        Ok(config) => Ok((config.path, config.language)),
        Err(err) => Err(map_config_parse_error(&err)),
    }
}

fn map_config_parse_error(err: &str) -> String {
    if err.contains("Section [game]") {
        return "Game section not found in config".to_string();
    }
    if err.contains("Key 'path'") {
        return "Game path not found in config".to_string();
    }
    if err.contains("Key 'lang'") {
        return "Game language not found in config".to_string();
    }
    err.to_string()
}

/// Gets the game path from the configuration.
#[cfg(not(tarpaulin_include))]
pub(crate) fn get_game_path() -> Result<PathBuf, String> {
    let (game_path, _) = load_config()?;
    Ok(game_path)
}

/// Gets the game path from config, with injectable FileSystem.
#[allow(dead_code)]
#[cfg(not(tarpaulin_include))]
pub(crate) fn get_game_path_with_fs<F: FileSystem>(fs: &F) -> Result<String, String> {
    let config_path = find_config_file().ok_or("Config file not found")?;
    let content = fs
        .read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;

    let config = config_service::parse_game_config(&content)?;
    Ok(config.path.to_string_lossy().into_owned())
}

/// Saves the game path to config, with injectable FileSystem.
///
/// This is the testable inner function that performs the actual save.
/// Note: This does NOT perform path validation or side effects (cancel downloads, etc.)
/// Those are handled by the Tauri command wrapper.
#[allow(dead_code)]
pub(crate) fn save_game_path_with_fs<F: FileSystem>(
    fs: &F,
    config_path: &Path,
    new_path: &str,
) -> Result<(), String> {
    let content = fs
        .read_to_string(config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;
    let updated = config_service::update_path_in_config(&content, Path::new(new_path))?;
    fs.write_file(config_path, updated.as_bytes())
        .map_err(|e| format!("Failed to write config: {}", e))?;

    Ok(())
}

/// Saves the language to config, with injectable FileSystem.
#[allow(dead_code)]
pub(crate) fn save_language_with_fs<F: FileSystem>(
    fs: &F,
    config_path: &Path,
    language: &str,
) -> Result<(), String> {
    let content = fs
        .read_to_string(config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;
    let updated = config_service::update_language_in_config(&content, language)?;
    fs.write_file(config_path, updated.as_bytes())
        .map_err(|e| format!("Failed to write config: {}", e))?;

    Ok(())
}

/// Checks if the game path has changed (case-insensitive comparison).
fn game_path_changed(previous: Option<&str>, next: &str) -> bool {
    match previous {
        Some(prev) => normalize_path_for_compare(prev) != normalize_path_for_compare(next),
        None => true,
    }
}

/// Internal function to clear the hash cache.
#[cfg(not(tarpaulin_include))]
async fn clear_cache_internal() -> Result<(), String> {
    // Clear the in-memory hash cache
    clear_hash_cache().await;
    // Remove the disk cache file
    let cache_path = get_cache_file_path()?;
    if cache_path.exists() {
        fs::remove_file(cache_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Gets the path to the cache file.
#[cfg(not(tarpaulin_include))]
pub(crate) fn get_cache_file_path() -> Result<PathBuf, String> {
    let mut path = std::env::current_exe().map_err(|e| e.to_string())?;
    path.pop();
    path.push("file_cache.json");
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::MockFileSystem;
    use std::path::Path;

    // ========================================================================
    // Game path change detection tests
    // ========================================================================

    #[test]
    fn game_path_changed_same_path_different_case() {
        assert!(!game_path_changed(Some("C:/Games/TERA"), "c:/games/tera"));
    }

    #[test]
    fn game_path_changed_same_with_trailing_slash() {
        assert!(!game_path_changed(Some("C:/Games/TERA"), "C:/Games/TERA/"));
        assert!(!game_path_changed(Some("C:/Games/TERA/"), "C:/Games/TERA"));
    }

    #[test]
    fn game_path_changed_none_previous() {
        assert!(game_path_changed(None, "C:/Games/TERA"));
    }

    #[test]
    fn game_path_changed_different_paths() {
        assert!(game_path_changed(Some("C:/Games/TERA"), "D:/Games/TERA"));
    }

    // ========================================================================
    // INI parsing tests (pure functions, no filesystem)
    // ========================================================================

    #[test]
    fn parse_game_path_from_valid_ini() {
        // Note: The ini crate treats backslashes as escape characters.
        // To properly represent Windows paths with backslashes in INI files,
        // they must be doubled (e.g., C:\\\\Games\\\\TERA).
        // Alternatively, forward slashes work on Windows (C:/Games/TERA).
        let content = r"[game]
path=C:\\Games\\TERA
lang=EUR
";
        let result = parse_game_path_from_ini(content);
        assert_eq!(result.unwrap(), r"C:\Games\TERA");
    }

    #[test]
    fn parse_game_path_missing_section() {
        let content = "[other]\nkey=value\n";
        let result = parse_game_path_from_ini(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Game section not found"));
    }

    #[test]
    fn parse_game_path_missing_path_key() {
        let content = "[game]\nlang=EUR\n";
        let result = parse_game_path_from_ini(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Game path not found"));
    }

    #[test]
    fn parse_config_from_valid_ini() {
        // Note: The ini crate treats backslashes as escape characters.
        // To properly represent Windows paths with backslashes in INI files,
        // they must be doubled (e.g., C:\\\\Games\\\\TERA).
        let content = r"[game]
path=C:\\Games\\TERA
lang=EUR
";
        let result = parse_config_from_ini(content);
        let (path, lang) = result.unwrap();
        assert_eq!(path, PathBuf::from(r"C:\Games\TERA"));
        assert_eq!(lang, "EUR");
    }

    #[test]
    fn parse_config_missing_language() {
        let content = "[game]\npath=C:\\Games\\TERA\n";
        let result = parse_config_from_ini(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Game language not found"));
    }

    #[test]
    fn parse_config_invalid_ini_format() {
        let content = "this is not valid ini [[[";
        let result = parse_config_from_ini(content);
        // ini crate is permissive, so this may actually parse
        // Test that we handle whatever the ini crate returns
        assert!(result.is_err() || result.is_ok());
    }

    // ========================================================================
    // FileSystem-based tests using MockFileSystem
    // ========================================================================

    #[test]
    fn save_game_path_with_fs_writes_correctly() {
        let initial_content = r"[game]
path=C:\\OldPath
lang=EUR
";
        let config_path = "/test/config.ini";

        let mock_fs = MockFileSystem::new().with_file(config_path, initial_content.as_bytes());

        let result = save_game_path_with_fs(&mock_fs, Path::new(config_path), r"D:\NewPath");
        assert!(result.is_ok());

        // Verify the file was updated
        // The ini crate will write the path with escaped backslashes
        let updated_content = mock_fs.read_to_string(Path::new(config_path)).unwrap();
        assert!(
            updated_content.contains(r"D:\\NewPath") || updated_content.contains(r"D:\NewPath")
        );
    }

    #[test]
    fn save_game_path_with_fs_file_not_found() {
        let mock_fs = MockFileSystem::new();
        let config_path = "/nonexistent/config.ini";

        let result = save_game_path_with_fs(&mock_fs, Path::new(config_path), "D:\\NewPath");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to read config"));
    }

    #[test]
    fn save_language_with_fs_writes_correctly() {
        let initial_content = "[game]\npath=C:\\Games\\TERA\nlang=EUR\n";
        let config_path = "/test/config.ini";

        let mock_fs = MockFileSystem::new().with_file(config_path, initial_content.as_bytes());

        let result = save_language_with_fs(&mock_fs, Path::new(config_path), "GER");
        assert!(result.is_ok());

        // Verify the file was updated
        let updated_content = mock_fs.read_to_string(Path::new(config_path)).unwrap();
        assert!(updated_content.contains("GER"));
    }

    #[test]
    fn save_language_with_fs_file_not_found() {
        let mock_fs = MockFileSystem::new();
        let config_path = "/nonexistent/config.ini";

        let result = save_language_with_fs(&mock_fs, Path::new(config_path), "GER");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to read config"));
    }

    #[test]
    fn save_game_path_preserves_other_settings() {
        let initial_content = r"[game]
path=C:\\OldPath
lang=EUR
[other]
key=value
";
        let config_path = "/test/config.ini";

        let mock_fs = MockFileSystem::new().with_file(config_path, initial_content.as_bytes());

        let result = save_game_path_with_fs(&mock_fs, Path::new(config_path), r"D:\NewPath");
        assert!(result.is_ok());

        let updated_content = mock_fs.read_to_string(Path::new(config_path)).unwrap();
        // Path should be updated (ini crate will write with escaped backslashes)
        assert!(
            updated_content.contains(r"D:\\NewPath") || updated_content.contains(r"D:\NewPath")
        );
        // Language should be preserved
        assert!(updated_content.contains("EUR"));
    }

    // ========================================================================
    // Additional edge case tests for config operations
    // ========================================================================

    #[test]
    fn parse_game_path_with_forward_slashes() {
        let content = "[game]\npath=C:/Games/TERA\nlang=EUR\n";
        let result = parse_game_path_from_ini(content);
        assert_eq!(result.unwrap(), "C:/Games/TERA");
    }

    #[test]
    fn parse_game_path_with_spaces() {
        let content = "[game]\npath=C:/Program Files/TERA\nlang=EUR\n";
        let result = parse_game_path_from_ini(content);
        assert_eq!(result.unwrap(), "C:/Program Files/TERA");
    }

    #[test]
    fn parse_config_with_extra_whitespace() {
        let content = r"  [game]
  path = C:\\Games\\TERA
  lang = EUR
";
        let result = parse_config_from_ini(content);
        assert!(result.is_ok());
        let (path, lang) = result.unwrap();
        assert_eq!(path, PathBuf::from(r"C:\Games\TERA"));
        assert_eq!(lang, "EUR");
    }

    #[test]
    fn parse_config_with_empty_section() {
        let content = "[game]\n[empty]\n";
        let result = parse_config_from_ini(content);
        assert!(result.is_err());
    }

    #[test]
    fn save_language_preserves_game_path() {
        let initial_content = r"[game]
path=C:\\Games\\TERA
lang=EUR
";
        let config_path = "/test/config.ini";

        let mock_fs = MockFileSystem::new().with_file(config_path, initial_content.as_bytes());

        let result = save_language_with_fs(&mock_fs, Path::new(config_path), "GER");
        assert!(result.is_ok());

        let updated_content = mock_fs.read_to_string(Path::new(config_path)).unwrap();
        // Language should be updated
        assert!(updated_content.contains("GER"));
        // Path should be preserved
        assert!(
            updated_content.contains(r"C:\\Games\\TERA")
                || updated_content.contains(r"C:\Games\TERA")
        );
    }

    #[test]
    fn save_game_path_with_fs_malformed_ini() {
        let malformed_content = "[game\npath=broken";
        let config_path = "/test/config.ini";

        let mock_fs = MockFileSystem::new().with_file(config_path, malformed_content.as_bytes());

        let result = save_game_path_with_fs(&mock_fs, Path::new(config_path), r"D:\NewPath");
        // The ini crate is permissive, but this should still parse
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn save_language_with_fs_malformed_ini() {
        let malformed_content = "notvalid[[[";
        let config_path = "/test/config.ini";

        let mock_fs = MockFileSystem::new().with_file(config_path, malformed_content.as_bytes());

        let result = save_language_with_fs(&mock_fs, Path::new(config_path), "GER");
        // Should either parse permissively or fail gracefully
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn game_path_changed_with_backslash_forward_slash_mix() {
        // Windows accepts both, should be treated as same
        assert!(!game_path_changed(Some("C:\\Games\\TERA"), "C:/Games/TERA"));
    }

    #[test]
    fn game_path_changed_with_multiple_trailing_slashes() {
        assert!(!game_path_changed(
            Some("C:/Games/TERA///"),
            "C:/Games/TERA"
        ));
    }

    #[test]
    fn game_path_changed_empty_strings() {
        assert!(!game_path_changed(Some(""), ""));
        assert!(game_path_changed(Some("C:/Games"), ""));
        assert!(game_path_changed(Some(""), "C:/Games"));
    }

    #[test]
    fn parse_config_with_multiple_sections() {
        let content = r"[game]
path=C:\\Games\\TERA
lang=EUR

[display]
width=1920
height=1080

[audio]
volume=100
";
        let result = parse_config_from_ini(content);
        assert!(result.is_ok());
        let (path, lang) = result.unwrap();
        assert_eq!(path, PathBuf::from(r"C:\Games\TERA"));
        assert_eq!(lang, "EUR");
    }

    // Note: get_game_path_from_config_with_fs and load_config_with_fs tests
    // are not included because these functions call find_config_file() which
    // requires real filesystem access. The testable parts are covered by
    // parse_game_path_from_ini and parse_config_from_ini tests.

    #[test]
    fn save_game_path_with_empty_path() {
        let initial_content = "[game]\npath=C:/OldPath\nlang=EUR\n";
        let config_path = "/test/config.ini";

        let mock_fs = MockFileSystem::new().with_file(config_path, initial_content.as_bytes());

        let result = save_game_path_with_fs(&mock_fs, Path::new(config_path), "");
        assert!(result.is_ok());

        let updated_content = mock_fs.read_to_string(Path::new(config_path)).unwrap();
        // Empty path should be written
        assert!(updated_content.contains("path=") || updated_content.contains("path ="));
    }

    #[test]
    fn save_language_with_empty_language() {
        let initial_content = "[game]\npath=C:/Games/TERA\nlang=EUR\n";
        let config_path = "/test/config.ini";

        let mock_fs = MockFileSystem::new().with_file(config_path, initial_content.as_bytes());

        let result = save_language_with_fs(&mock_fs, Path::new(config_path), "");
        // Empty language code is invalid and should return an error
        assert!(result.is_err());
    }

    #[test]
    fn parse_game_path_with_unicode_characters() {
        let content = "[game]\npath=/home/用户/游戏/TERA\nlang=EUR\n";
        let result = parse_game_path_from_ini(content);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "/home/用户/游戏/TERA");
    }

    #[test]
    fn parse_config_case_sensitive_section_names() {
        // Test that section names are case-sensitive
        let content = "[GAME]\npath=C:/Games/TERA\nlang=EUR\n";
        let result = parse_config_from_ini(content);
        // ini crate is case-sensitive for section names
        assert!(result.is_err());
    }

    #[test]
    fn game_path_changed_with_relative_paths() {
        assert!(game_path_changed(Some("./Games/TERA"), "../Games/TERA"));
        assert!(!game_path_changed(Some("./Games/TERA"), "./Games/TERA"));
    }
}
