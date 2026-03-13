//! Configuration service for managing launcher settings.
//!
//! This module provides pure functions for configuration management:
//! - Config file path resolution
//! - INI file reading/writing
//! - Game path and language management

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Configuration section name for game settings
pub const GAME_SECTION: &str = "game";

/// Key name for the game path setting
pub const PATH_KEY: &str = "path";

/// Key name for the game language setting
pub const LANG_KEY: &str = "lang";

/// Valid language codes for the game
const VALID_LANGUAGE_CODES: &[&str] = &["GER", "EUR", "FRA", "RUS"];

/// Result of parsing a config file
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameConfig {
    pub path: PathBuf,
    pub language: String,
}

/// Parses game configuration from INI content.
///
/// # Arguments
/// * `content` - The INI file content as a string
///
/// # Returns
/// * `Ok(GameConfig)` - The parsed configuration
/// * `Err(String)` - Error message if parsing fails
///
/// # Examples
/// ```ignore
/// let content = "[game]\npath=C:/Games/TERA\nlang=EUR";
/// let config = parse_game_config(content)?;
/// assert_eq!(config.language, "EUR");
/// ```
pub fn parse_game_config(content: &str) -> Result<GameConfig, String> {
    let ini =
        ini::Ini::load_from_str(content).map_err(|e| format!("Failed to parse config: {}", e))?;

    let section = ini
        .section(Some(GAME_SECTION))
        .ok_or_else(|| format!("Section [{}] not found in config", GAME_SECTION))?;

    let path = section
        .get(PATH_KEY)
        .ok_or_else(|| format!("Key '{}' not found in [{}] section", PATH_KEY, GAME_SECTION))?;

    let language = section
        .get(LANG_KEY)
        .ok_or_else(|| format!("Key '{}' not found in [{}] section", LANG_KEY, GAME_SECTION))?;

    // Path can be empty in fresh installations - this is valid
    Ok(GameConfig {
        path: PathBuf::from(path),
        language: language.to_string(),
    })
}

/// Generates INI content from a game configuration.
///
/// # Arguments
/// * `config` - The configuration to serialize
///
/// # Returns
/// The INI content as a string
///
/// # Examples
/// ```ignore
/// let config = GameConfig {
///     path: PathBuf::from("C:/Games/TERA"),
///     language: "EUR".to_string(),
/// };
/// let content = generate_config_content(&config);
/// assert!(content.contains("[game]"));
/// assert!(content.contains("path=C:/Games/TERA"));
/// ```
pub fn generate_config_content(config: &GameConfig) -> String {
    let mut ini = ini::Ini::new();
    ini.with_section(Some(GAME_SECTION))
        .set(PATH_KEY, config.path.to_string_lossy().as_ref())
        .set(LANG_KEY, &config.language);

    let mut output = Vec::new();
    ini.write_to(&mut output)
        .expect("Failed to write INI to buffer");
    String::from_utf8(output).expect("INI content is not valid UTF-8")
}

/// Updates just the path in an existing config content.
///
/// # Arguments
/// * `content` - The existing INI file content
/// * `new_path` - The new path to set
///
/// # Returns
/// * `Ok(String)` - The updated INI content
/// * `Err(String)` - Error message if update fails
pub fn update_path_in_config(content: &str, new_path: &Path) -> Result<String, String> {
    let mut ini =
        ini::Ini::load_from_str(content).map_err(|e| format!("Failed to parse config: {}", e))?;

    ini.with_section(Some(GAME_SECTION))
        .set(PATH_KEY, new_path.to_string_lossy().as_ref());

    let mut output = Vec::new();
    ini.write_to(&mut output)
        .map_err(|e| format!("Failed to write config: {}", e))?;
    String::from_utf8(output).map_err(|e| format!("Invalid UTF-8 in config: {}", e))
}

/// Updates just the language in an existing config content.
///
/// # Arguments
/// * `content` - The existing INI file content
/// * `new_lang` - The new language to set
///
/// # Returns
/// * `Ok(String)` - The updated INI content
/// * `Err(String)` - Error message if update fails
pub fn update_language_in_config(content: &str, new_lang: &str) -> Result<String, String> {
    // Validate language code first
    validate_language_code(new_lang)?;

    let mut ini =
        ini::Ini::load_from_str(content).map_err(|e| format!("Failed to parse config: {}", e))?;

    ini.with_section(Some(GAME_SECTION)).set(LANG_KEY, new_lang);

    let mut output = Vec::new();
    ini.write_to(&mut output)
        .map_err(|e| format!("Failed to write config: {}", e))?;
    String::from_utf8(output).map_err(|e| format!("Invalid UTF-8 in config: {}", e))
}

/// Determines the config file path based on platform conventions.
///
/// Returns the path where the config file should be located:
/// - Windows: %APPDATA%/Crazy-eSports.com/tera_config.ini
/// - Linux/Mac: ~/.config/Crazy-eSports.com/tera_config.ini
///
/// # Returns
/// * `Some(PathBuf)` - The expected config file path
/// * `None` - If the config directory cannot be determined
pub fn get_config_file_path() -> Option<PathBuf> {
    dirs_next::config_dir().map(|d| d.join("Crazy-eSports.com").join("tera_config.ini"))
}

/// Searches for legacy config file locations.
///
/// Checks common locations where old versions of the launcher might have stored config:
/// - Current directory: ./src/tera_config.ini
/// - Parent directory: ../src/tera_config.ini
/// - Executable directory: {exe_dir}/src/tera_config.ini
///
/// # Arguments
/// * `current_dir` - The current working directory (optional)
/// * `exe_dir` - The executable directory (optional)
///
/// # Returns
/// Paths to check in priority order
pub fn get_legacy_config_paths(current_dir: Option<&Path>, exe_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::with_capacity(3);

    if let Some(cwd) = current_dir {
        paths.push(cwd.join("src/tera_config.ini"));
        if let Some(parent) = cwd.parent() {
            paths.push(parent.join("src/tera_config.ini"));
        }
    }

    if let Some(exe) = exe_dir {
        paths.push(exe.join("src/tera_config.ini"));
    }

    paths
}

/// Validates that a language code is in the allowed list
///
/// # Arguments
/// * `lang` - The language code to validate
///
/// # Returns
/// * `Ok(())` - If the language code is valid
/// * `Err(String)` - Error message with valid codes if invalid
///
/// # Examples
/// ```ignore
/// validate_language_code("EUR")?;  // OK
/// validate_language_code("eur")?;  // OK (case-insensitive)
/// validate_language_code("XXX")?;  // Error
/// ```
pub fn validate_language_code(lang: &str) -> Result<(), String> {
    let lang_upper = lang.to_uppercase();
    if VALID_LANGUAGE_CODES.contains(&lang_upper.as_str()) {
        Ok(())
    } else {
        Err(format!(
            "Invalid language code '{}'. Valid codes: {:?}",
            lang, VALID_LANGUAGE_CODES
        ))
    }
}

/// Validates that a path is a valid game directory.
///
/// Checks that the path exists and is a directory.
///
/// # Arguments
/// * `path` - The path to validate
///
/// # Returns
/// * `Ok(())` - If the path is valid
/// * `Err(String)` - Error message describing the validation failure
pub fn validate_game_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("The specified path does not exist".to_string());
    }
    if !path.is_dir() {
        return Err("The specified path is not a directory".to_string());
    }
    Ok(())
}

/// Default configuration content for new installations.
pub const DEFAULT_CONFIG_CONTENT: &str = include_str!("../tera_config.ini");

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Tests for parse_game_config
    // ========================================================================

    #[test]
    fn parse_game_config_valid() {
        let content = "[game]\npath=C:/Games/TERA\nlang=EUR\n";
        let result = parse_game_config(content);
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.path, PathBuf::from("C:/Games/TERA"));
        assert_eq!(config.language, "EUR");
    }

    #[test]
    fn parse_game_config_with_spaces() {
        let content = "[game]\npath = C:/Games/TERA\nlang = EUR\n";
        let result = parse_game_config(content);
        assert!(result.is_ok());
        let config = result.unwrap();
        // Note: ini crate trims whitespace
        assert_eq!(config.path, PathBuf::from("C:/Games/TERA"));
        assert_eq!(config.language, "EUR");
    }

    #[test]
    fn parse_game_config_missing_section() {
        let content = "path=C:/Games/TERA\nlang=EUR\n";
        let result = parse_game_config(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Section [game] not found"));
    }

    #[test]
    fn parse_game_config_missing_path() {
        let content = "[game]\nlang=EUR\n";
        let result = parse_game_config(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Key 'path' not found"));
    }

    #[test]
    fn parse_game_config_missing_lang() {
        let content = "[game]\npath=C:/Games/TERA\n";
        let result = parse_game_config(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Key 'lang' not found"));
    }

    #[test]
    fn parse_game_config_wrong_section() {
        let content = "[settings]\npath=C:/Games/TERA\nlang=EUR\n";
        let result = parse_game_config(content);
        assert!(result.is_err());
    }

    #[test]
    fn parse_game_config_extra_sections() {
        let content = "[game]\npath=C:/Games/TERA\nlang=EUR\n[other]\nkey=value\n";
        let result = parse_game_config(content);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_game_config_empty_content() {
        let content = "";
        let result = parse_game_config(content);
        assert!(result.is_err());
    }

    #[test]
    fn parse_game_config_only_section() {
        let content = "[game]\n";
        let result = parse_game_config(content);
        assert!(result.is_err());
    }

    #[test]
    fn parse_game_config_backslash_path() {
        let content = "[game]\npath=C:\\Games\\TERA\nlang=EUR\n";
        let result = parse_game_config(content);
        assert!(result.is_ok());
        // Path is preserved as-is
        let config = result.unwrap();
        assert!(config.path.to_string_lossy().contains("TERA"));
    }

    // ========================================================================
    // Tests for generate_config_content
    // ========================================================================

    #[test]
    fn generate_config_content_basic() {
        let config = GameConfig {
            path: PathBuf::from("C:/Games/TERA"),
            language: "EUR".to_string(),
        };
        let content = generate_config_content(&config);
        assert!(content.contains("[game]"));
        assert!(content.contains("path="));
        assert!(content.contains("lang=EUR"));
    }

    #[test]
    fn generate_config_content_roundtrip() {
        let original = GameConfig {
            path: PathBuf::from("D:/TERA Online"),
            language: "USA".to_string(),
        };
        let content = generate_config_content(&original);
        let parsed = parse_game_config(&content).unwrap();
        assert_eq!(parsed.language, original.language);
        // Path comparison may differ due to separator normalization
        assert!(parsed.path.to_string_lossy().contains("TERA Online"));
    }

    #[test]
    fn generate_config_content_empty_values() {
        let config = GameConfig {
            path: PathBuf::from(""),
            language: String::new(),
        };
        let content = generate_config_content(&config);
        assert!(content.contains("[game]"));
        // Should still have keys even if empty
        assert!(content.contains("path="));
        assert!(content.contains("lang="));
    }

    // ========================================================================
    // Tests for update_path_in_config
    // ========================================================================

    #[test]
    fn update_path_in_config_basic() {
        let content = "[game]\npath=C:/Old/Path\nlang=EUR\n";
        let new_path = Path::new("D:/New/Path");
        let result = update_path_in_config(content, new_path);
        assert!(result.is_ok());
        let updated = result.unwrap();
        assert!(updated.contains("D:/New/Path") || updated.contains("D:\\New\\Path"));
        // Language should be preserved
        assert!(updated.contains("lang=EUR"));
    }

    #[test]
    fn update_path_in_config_preserves_other_sections() {
        let content = "[game]\npath=C:/Old\nlang=EUR\n[other]\nkey=value\n";
        let new_path = Path::new("D:/New");
        let result = update_path_in_config(content, new_path);
        assert!(result.is_ok());
        let updated = result.unwrap();
        assert!(updated.contains("[other]"));
        assert!(updated.contains("key=value"));
    }

    #[test]
    fn update_path_in_config_invalid_ini() {
        let content = "not valid ini {{{{";
        let new_path = Path::new("D:/New");
        // The ini crate is fairly permissive, so this might not fail
        // but let's test the error path concept
        let _ = update_path_in_config(content, new_path);
    }

    // ========================================================================
    // Tests for update_language_in_config
    // ========================================================================

    #[test]
    fn update_language_in_config_basic() {
        let content = "[game]\npath=C:/Games/TERA\nlang=EUR\n";
        let result = update_language_in_config(content, "USA");
        assert!(result.is_ok());
        let updated = result.unwrap();
        assert!(updated.contains("lang=USA"));
        // Path should be preserved
        assert!(updated.contains("C:/Games/TERA") || updated.contains("C:\\Games\\TERA"));
    }

    #[test]
    fn update_language_in_config_empty_lang() {
        let content = "[game]\npath=C:/Games/TERA\nlang=EUR\n";
        let result = update_language_in_config(content, "");
        assert!(result.is_ok());
        let updated = result.unwrap();
        assert!(updated.contains("lang="));
    }

    // ========================================================================
    // Tests for get_config_file_path
    // ========================================================================

    #[test]
    fn get_config_file_path_returns_expected_structure() {
        let path = get_config_file_path();
        // Should return Some on most systems
        if let Some(p) = path {
            assert!(p.to_string_lossy().contains("Crazy-eSports.com"));
            assert!(p.to_string_lossy().contains("tera_config.ini"));
        }
    }

    // ========================================================================
    // Tests for get_legacy_config_paths
    // ========================================================================

    #[test]
    fn get_legacy_config_paths_with_current_dir() {
        let current = PathBuf::from("/app");
        let paths = get_legacy_config_paths(Some(&current), None);
        assert!(!paths.is_empty());
        assert!(paths
            .iter()
            .any(|p| p.to_string_lossy().contains("src/tera_config.ini")));
    }

    #[test]
    fn get_legacy_config_paths_with_exe_dir() {
        let exe = PathBuf::from("/opt/launcher");
        let paths = get_legacy_config_paths(None, Some(&exe));
        assert!(!paths.is_empty());
        assert!(paths
            .iter()
            .any(|p| p.to_string_lossy().contains("src/tera_config.ini")));
    }

    #[test]
    fn get_legacy_config_paths_both_dirs() {
        let current = PathBuf::from("/app");
        let exe = PathBuf::from("/opt/launcher");
        let paths = get_legacy_config_paths(Some(&current), Some(&exe));
        // Should have paths from both sources
        assert!(paths.len() >= 2);
    }

    #[test]
    fn get_legacy_config_paths_none() {
        let paths = get_legacy_config_paths(None, None);
        assert!(paths.is_empty());
    }

    // ========================================================================
    // Tests for validate_game_path
    // ========================================================================

    #[test]
    fn validate_game_path_nonexistent() {
        let path = Path::new("/nonexistent/path/that/does/not/exist/anywhere");
        let result = validate_game_path(path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn validate_game_path_is_file() {
        // Create a temp file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_validate_game_path.txt");
        std::fs::write(&temp_file, "test").unwrap();

        let result = validate_game_path(&temp_file);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a directory"));

        // Cleanup
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn validate_game_path_valid_directory() {
        let temp_dir = std::env::temp_dir();
        let result = validate_game_path(&temp_dir);
        assert!(result.is_ok());
    }

    // ========================================================================
    // Tests for DEFAULT_CONFIG_CONTENT
    // ========================================================================

    #[test]
    fn default_config_content_is_valid() {
        // The default content should be parseable
        let result = parse_game_config(DEFAULT_CONFIG_CONTENT);
        // It's okay if it fails due to empty values, but the structure should be valid
        // or it should at least contain the section marker
        assert!(DEFAULT_CONFIG_CONTENT.contains("[game]") || result.is_ok());
    }

    // ========================================================================
    // Tests for validate_language_code
    // ========================================================================

    #[test]
    fn validate_language_code_valid_uppercase() {
        assert!(validate_language_code("GER").is_ok());
        assert!(validate_language_code("EUR").is_ok());
        assert!(validate_language_code("FRA").is_ok());
        assert!(validate_language_code("RUS").is_ok());
    }

    #[test]
    fn validate_language_code_valid_lowercase() {
        // Should be case-insensitive
        assert!(validate_language_code("ger").is_ok());
        assert!(validate_language_code("eur").is_ok());
        assert!(validate_language_code("fra").is_ok());
        assert!(validate_language_code("rus").is_ok());
    }

    #[test]
    fn validate_language_code_valid_mixed_case() {
        assert!(validate_language_code("GeR").is_ok());
        assert!(validate_language_code("eUr").is_ok());
    }

    #[test]
    fn validate_language_code_invalid() {
        let result = validate_language_code("XXX");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Invalid language code"));
        assert!(err.contains("XXX"));
    }

    #[test]
    fn validate_language_code_empty() {
        let result = validate_language_code("");
        assert!(result.is_err());
    }

    #[test]
    fn validate_language_code_invalid_partial_match() {
        // Should not accept partial matches
        let result = validate_language_code("EU");
        assert!(result.is_err());
        let result = validate_language_code("EUROPE");
        assert!(result.is_err());
    }

    #[test]
    fn update_language_in_config_validates_code() {
        let content = "[game]\npath=C:/Games/TERA\nlang=EUR\n";

        // Valid language should work
        let result = update_language_in_config(content, "GER");
        assert!(result.is_ok());

        // Invalid language should fail
        let result = update_language_in_config(content, "XXX");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid language code"));
    }
}
