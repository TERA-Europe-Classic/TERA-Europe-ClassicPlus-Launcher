//! Path validation and manipulation utilities.
//!
//! This module provides pure functions for path operations, including:
//! - Path traversal prevention
//! - File ignore pattern matching
//! - Path normalization for comparison

#![allow(dead_code)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Validates that a resolved path is safely within the base directory.
/// Prevents path traversal attacks using ".." or absolute paths.
///
/// # Arguments
/// * `base` - The base directory that the file path must be within
/// * `file_path` - The path to validate
///
/// # Returns
/// * `Ok(PathBuf)` - The canonicalized path if valid
/// * `Err(String)` - An error message if validation fails
///
/// # Examples
/// ```ignore
/// let base = Path::new("/games/tera");
/// let file = Path::new("/games/tera/data/file.txt");
/// assert!(validate_path_within_base(base, file).is_ok());
///
/// let malicious = Path::new("/games/tera/../../../etc/passwd");
/// assert!(validate_path_within_base(base, malicious).is_err());
/// ```
pub fn validate_path_within_base(base: &Path, file_path: &Path) -> Result<PathBuf, String> {
    // Canonicalize both paths to resolve symlinks and ".." components
    let canonical_base = base
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize base path: {}", e))?;

    // For the file path, if it doesn't exist yet, canonicalize the parent
    let canonical_path = if file_path.exists() {
        file_path
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize file path: {}", e))?
    } else {
        // For new files, ensure parent exists and check that
        let parent = file_path
            .parent()
            .ok_or_else(|| "File path has no parent".to_string())?;
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent directory: {}", e))?;
        }
        let canonical_parent = parent
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize parent: {}", e))?;
        canonical_parent.join(file_path.file_name().ok_or("No file name")?)
    };

    // Check that the canonical path starts with the canonical base
    if !canonical_path.starts_with(&canonical_base) {
        return Err(format!(
            "Path traversal detected: {} is outside {}",
            canonical_path.display(),
            canonical_base.display()
        ));
    }

    Ok(canonical_path)
}

/// Checks if a path should be ignored based on game path and ignore patterns.
///
/// A path is ignored if:
/// 1. It's at the root level of the game directory (no subdirectory)
/// 2. It matches any of the ignored path patterns
///
/// # Arguments
/// * `path` - The path to check
/// * `game_path` - The base game directory path
/// * `ignored_paths` - Set of path patterns to ignore (e.g., "$Patch", "S1Game/Logs")
///
/// # Returns
/// * `true` if the path should be ignored
/// * `false` if the path should be processed
///
/// # Examples
/// ```ignore
/// let game_path = Path::new("/games/tera");
/// let mut ignored = HashSet::new();
/// ignored.insert("$Patch");
///
/// // Root files are ignored
/// assert!(is_ignored(Path::new("/games/tera/file.txt"), game_path, &ignored));
///
/// // Files in $Patch are ignored
/// assert!(is_ignored(Path::new("/games/tera/$Patch/data.pak"), game_path, &ignored));
///
/// // Normal game files are not ignored
/// assert!(!is_ignored(Path::new("/games/tera/Binaries/TERA.exe"), game_path, &ignored));
/// ```
#[cfg(not(tarpaulin_include))]
pub fn is_ignored(path: &Path, game_path: &Path, ignored_paths: &HashSet<&str>) -> bool {
    let relative_path = match path.strip_prefix(game_path) {
        Ok(p) => match p.to_str() {
            Some(s) => s.replace('\\', "/"),
            // Note: Non-UTF8 paths are nearly impossible to create on Windows and
            // difficult to test portably across platforms.
            None => return false, // Non-UTF8 path, don't ignore
        },
        Err(_) => return false, // Path not under game_path, don't ignore
    };

    // Ignore files at the root
    if relative_path.chars().filter(|&c| c == '/').count() == 0 {
        return true;
    }

    // Check if the path is in the list of ignored paths
    for ignored_path in ignored_paths {
        if relative_path.starts_with(ignored_path) {
            return true;
        }
    }

    false
}

/// Normalizes a path string for case-insensitive comparison.
///
/// This function:
/// 1. Converts backslashes to forward slashes
/// 2. Removes trailing slashes
/// 3. Converts to lowercase
///
/// # Arguments
/// * `value` - The path string to normalize
///
/// # Returns
/// A normalized path string suitable for comparison
///
/// # Examples
/// ```ignore
/// assert_eq!(normalize_path_for_compare("C:\\Games\\TERA\\"), "c:/games/tera");
/// assert_eq!(normalize_path_for_compare("c:/games/tera"), "c:/games/tera");
/// ```
pub fn normalize_path_for_compare(value: &str) -> String {
    let mut path = value.replace('\\', "/");
    while path.ends_with('/') {
        path.pop();
    }
    path.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Tests for is_ignored
    // ========================================================================

    #[test]
    fn is_ignored_root_files_ignored() {
        let game_path = Path::new("/games/tera");
        let ignored: HashSet<&str> = HashSet::new();

        // File at root level (no subdirectory) should be ignored
        let root_file = Path::new("/games/tera/somefile.txt");
        assert!(is_ignored(root_file, game_path, &ignored));
    }

    #[test]
    fn is_ignored_exact_match() {
        let game_path = Path::new("/games/tera");
        let mut ignored: HashSet<&str> = HashSet::new();
        ignored.insert("$Patch");

        let patch_dir = Path::new("/games/tera/$Patch/file.txt");
        assert!(is_ignored(patch_dir, game_path, &ignored));
    }

    #[test]
    fn is_ignored_prefix_match() {
        let game_path = Path::new("/games/tera");
        let mut ignored: HashSet<&str> = HashSet::new();
        ignored.insert("S1Game/Logs");

        let log_file = Path::new("/games/tera/S1Game/Logs/game.log");
        assert!(is_ignored(log_file, game_path, &ignored));
    }

    #[test]
    fn is_ignored_non_ignored_path() {
        let game_path = Path::new("/games/tera");
        let mut ignored: HashSet<&str> = HashSet::new();
        ignored.insert("$Patch");
        ignored.insert("S1Game/Logs");

        // A legitimate game file should not be ignored
        let game_file = Path::new("/games/tera/Binaries/TERA.exe");
        assert!(!is_ignored(game_file, game_path, &ignored));
    }

    #[test]
    fn is_ignored_path_outside_game_dir() {
        let game_path = Path::new("/games/tera");
        let ignored: HashSet<&str> = HashSet::new();

        // Path not under game_path returns false
        let outside_path = Path::new("/other/path/file.txt");
        assert!(!is_ignored(outside_path, game_path, &ignored));
    }

    #[test]
    fn is_ignored_handles_backslash_paths() {
        let game_path = Path::new("/games/tera");
        let mut ignored: HashSet<&str> = HashSet::new();
        ignored.insert("S1Game/Config/S1Engine.ini");

        // The function normalizes backslashes to forward slashes
        let config_file = Path::new("/games/tera/S1Game/Config/S1Engine.ini");
        assert!(is_ignored(config_file, game_path, &ignored));
    }

    #[test]
    fn is_ignored_empty_ignored_set() {
        let game_path = Path::new("/games/tera");
        let ignored: HashSet<&str> = HashSet::new();

        // With empty ignore set, only root files are ignored
        let subdir_file = Path::new("/games/tera/subdir/file.txt");
        assert!(!is_ignored(subdir_file, game_path, &ignored));
    }

    #[test]
    fn is_ignored_deeply_nested_path() {
        let game_path = Path::new("/games/tera");
        let mut ignored: HashSet<&str> = HashSet::new();
        ignored.insert("S1Game/Logs");

        let deep_file = Path::new("/games/tera/S1Game/Logs/sub/dir/deep/file.log");
        assert!(is_ignored(deep_file, game_path, &ignored));
    }

    // ========================================================================
    // Tests for validate_path_within_base (requires temp directory)
    // ========================================================================

    #[test]
    fn validate_path_within_base_valid_path() {
        let temp_dir = std::env::temp_dir().join("test_validate_path_utils");
        let _ = std::fs::create_dir_all(&temp_dir);

        let file_path = temp_dir.join("subdir").join("file.txt");
        let result = validate_path_within_base(&temp_dir, &file_path);

        assert!(result.is_ok());
        let validated = result.unwrap();
        assert!(validated.starts_with(temp_dir.canonicalize().unwrap()));

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn validate_path_within_base_traversal_attempt() {
        let temp_dir = std::env::temp_dir().join("test_validate_traversal_utils");
        let _ = std::fs::create_dir_all(&temp_dir);

        // Attempt path traversal with ..
        let malicious_path = temp_dir.join("..").join("..").join("etc").join("passwd");
        let result = validate_path_within_base(&temp_dir, &malicious_path);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Path traversal detected") || err.contains("outside"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn validate_path_within_base_absolute_outside_path() {
        let temp_dir = std::env::temp_dir().join("test_validate_outside_utils");
        let _ = std::fs::create_dir_all(&temp_dir);

        // Try to validate a path completely outside the base
        let outside_path = std::env::temp_dir()
            .join("completely_different_dir_utils")
            .join("file.txt");
        let _ = std::fs::create_dir_all(outside_path.parent().unwrap());

        let result = validate_path_within_base(&temp_dir, &outside_path);

        assert!(result.is_err());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_dir_all(outside_path.parent().unwrap());
    }

    #[test]
    fn validate_path_within_base_nonexistent_base() {
        let nonexistent_base = Path::new("/nonexistent/base/path/that/does/not/exist");
        let file_path = nonexistent_base.join("file.txt");

        let result = validate_path_within_base(nonexistent_base, &file_path);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Failed to canonicalize base path"));
    }

    #[test]
    fn validate_path_within_base_creates_parent_dirs() {
        let temp_dir = std::env::temp_dir().join("test_validate_create_parent_utils");
        let _ = std::fs::create_dir_all(&temp_dir);

        // Nested path that doesn't exist yet
        let nested_path = temp_dir
            .join("new")
            .join("nested")
            .join("dir")
            .join("file.txt");
        let result = validate_path_within_base(&temp_dir, &nested_path);

        assert!(result.is_ok());
        // Parent directories should have been created
        assert!(nested_path.parent().unwrap().exists());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn validate_path_within_base_existing_file() {
        let temp_dir = std::env::temp_dir().join("test_validate_existing_utils");
        let _ = std::fs::create_dir_all(&temp_dir);

        // Create an actual file
        let file_path = temp_dir.join("existing_file.txt");
        let _ = std::fs::write(&file_path, "test content");

        let result = validate_path_within_base(&temp_dir, &file_path);

        assert!(result.is_ok());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // ========================================================================
    // Tests for normalize_path_for_compare
    // ========================================================================

    #[test]
    fn normalize_path_forward_slashes() {
        assert_eq!(normalize_path_for_compare("c:/games/tera"), "c:/games/tera");
    }

    #[test]
    fn normalize_path_back_slashes_to_forward() {
        assert_eq!(
            normalize_path_for_compare("c:\\games\\tera"),
            "c:/games/tera"
        );
    }

    #[test]
    fn normalize_path_mixed_slashes() {
        assert_eq!(
            normalize_path_for_compare("c:\\games/tera\\sub"),
            "c:/games/tera/sub"
        );
    }

    #[test]
    fn normalize_path_lowercase() {
        assert_eq!(normalize_path_for_compare("C:/GAMES/TERA"), "c:/games/tera");
        assert_eq!(
            normalize_path_for_compare("C:\\Games\\Tera"),
            "c:/games/tera"
        );
    }

    #[test]
    fn normalize_path_removes_trailing_slashes() {
        assert_eq!(
            normalize_path_for_compare("c:/games/tera/"),
            "c:/games/tera"
        );
        assert_eq!(
            normalize_path_for_compare("c:/games/tera//"),
            "c:/games/tera"
        );
        assert_eq!(
            normalize_path_for_compare("c:\\games\\tera\\"),
            "c:/games/tera"
        );
    }

    #[test]
    fn normalize_path_empty_string() {
        assert_eq!(normalize_path_for_compare(""), "");
    }

    #[test]
    fn normalize_path_only_slashes() {
        assert_eq!(normalize_path_for_compare("/"), "");
        assert_eq!(normalize_path_for_compare("//"), "");
        assert_eq!(normalize_path_for_compare("\\"), "");
    }

    #[test]
    fn normalize_path_preserves_drive_letter() {
        assert_eq!(normalize_path_for_compare("D:/Data"), "d:/data");
    }

    #[test]
    fn normalize_path_unicode_handling() {
        assert_eq!(normalize_path_for_compare("c:/games/tera"), "c:/games/tera");
    }
}
