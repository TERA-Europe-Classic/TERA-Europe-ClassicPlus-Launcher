//! Filesystem abstraction for testability.
//!
//! This module provides a trait for filesystem operations, allowing the application
//! to use mock implementations in tests while using std::fs in production.

use std::path::Path;

/// Metadata about a file.
#[derive(Debug, Clone)]
pub struct FileMetadata {
    /// Size of the file in bytes
    pub size: u64,
    /// Last modified time as Unix timestamp (seconds since epoch)
    pub modified_secs: u64,
    /// Whether this is a directory
    pub is_dir: bool,
    /// Whether this is a file
    pub is_file: bool,
}

/// Trait for filesystem operations, allowing mocking in tests.
pub trait FileSystem: Send + Sync {
    /// Read the entire contents of a file into a byte vector.
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, String>;

    /// Write data to a file, creating it if it doesn't exist.
    fn write_file(&self, path: &Path, data: &[u8]) -> Result<(), String>;

    /// Check if a path exists.
    fn exists(&self, path: &Path) -> bool;

    /// Create a directory and all parent directories.
    fn create_dir_all(&self, path: &Path) -> Result<(), String>;

    /// Remove a file.
    fn remove_file(&self, path: &Path) -> Result<(), String>;

    /// Get metadata about a file.
    fn metadata(&self, path: &Path) -> Result<FileMetadata, String>;

    /// Read a file as a UTF-8 string.
    fn read_to_string(&self, path: &Path) -> Result<String, String> {
        let bytes = self.read_file(path)?;
        String::from_utf8(bytes).map_err(|e| format!("Invalid UTF-8: {}", e))
    }

    /// Write a string to a file.
    fn write_string(&self, path: &Path, content: &str) -> Result<(), String> {
        self.write_file(path, content.as_bytes())
    }
}

/// Default filesystem implementation using std::fs.
pub struct StdFileSystem;

impl StdFileSystem {
    /// Create a new StdFileSystem instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for StdFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem for StdFileSystem {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, String> {
        std::fs::read(path).map_err(|e| format!("Failed to read file {:?}: {}", path, e))
    }

    fn write_file(&self, path: &Path, data: &[u8]) -> Result<(), String> {
        std::fs::write(path, data).map_err(|e| format!("Failed to write file {:?}: {}", path, e))
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), String> {
        std::fs::create_dir_all(path)
            .map_err(|e| format!("Failed to create directory {:?}: {}", path, e))
    }

    fn remove_file(&self, path: &Path) -> Result<(), String> {
        std::fs::remove_file(path).map_err(|e| format!("Failed to remove file {:?}: {}", path, e))
    }

    fn metadata(&self, path: &Path) -> Result<FileMetadata, String> {
        let meta = std::fs::metadata(path)
            .map_err(|e| format!("Failed to get metadata for {:?}: {}", path, e))?;

        let modified_secs = meta
            .modified()
            .map_err(|e| format!("Failed to get modified time: {}", e))?
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("Time conversion error: {}", e))?
            .as_secs();

        Ok(FileMetadata {
            size: meta.len(),
            modified_secs,
            is_dir: meta.is_dir(),
            is_file: meta.is_file(),
        })
    }
}

// ============================================================================
// Mock filesystem for testing (available in test builds)
// ============================================================================

/// Mock filesystem for testing.
///
/// This implementation stores files and directories in memory,
/// allowing tests to simulate filesystem operations without touching disk.
#[cfg(test)]
pub struct MockFileSystem {
    files: std::sync::RwLock<std::collections::HashMap<String, Vec<u8>>>,
    dirs: std::sync::RwLock<std::collections::HashSet<String>>,
    metadata_errors: std::sync::RwLock<std::collections::HashSet<String>>,
}

#[cfg(test)]
impl MockFileSystem {
    /// Create a new empty mock filesystem.
    pub fn new() -> Self {
        Self {
            files: std::sync::RwLock::new(std::collections::HashMap::new()),
            dirs: std::sync::RwLock::new(std::collections::HashSet::new()),
            metadata_errors: std::sync::RwLock::new(std::collections::HashSet::new()),
        }
    }

    /// Add a file with the given content. Builder pattern.
    pub fn with_file(self, path: &str, content: &[u8]) -> Self {
        self.files
            .write()
            .unwrap()
            .insert(path.to_string(), content.to_vec());
        self
    }

    /// Add a directory. Builder pattern.
    pub fn with_dir(self, path: &str) -> Self {
        self.dirs.write().unwrap().insert(path.to_string());
        self
    }

    /// Mark a path to return an error when metadata() is called.
    /// The path will still return true for exists() but Err for metadata().
    pub fn with_metadata_error(self, path: &str) -> Self {
        self.metadata_errors
            .write()
            .unwrap()
            .insert(path.to_string());
        self
    }
}

#[cfg(test)]
impl Default for MockFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl FileSystem for MockFileSystem {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, String> {
        let path_str = path.to_string_lossy().to_string();
        self.files
            .read()
            .unwrap()
            .get(&path_str)
            .cloned()
            .ok_or_else(|| format!("File not found: {}", path_str))
    }

    fn write_file(&self, path: &Path, data: &[u8]) -> Result<(), String> {
        let path_str = path.to_string_lossy().to_string();
        self.files.write().unwrap().insert(path_str, data.to_vec());
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy().to_string();
        self.files.read().unwrap().contains_key(&path_str)
            || self.dirs.read().unwrap().contains(&path_str)
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), String> {
        let path_str = path.to_string_lossy().to_string();
        self.dirs.write().unwrap().insert(path_str);
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<(), String> {
        let path_str = path.to_string_lossy().to_string();
        self.files
            .write()
            .unwrap()
            .remove(&path_str)
            .map(|_| ())
            .ok_or_else(|| format!("File not found: {}", path_str))
    }

    fn metadata(&self, path: &Path) -> Result<FileMetadata, String> {
        let path_str = path.to_string_lossy().to_string();

        // Check if this path should return an error
        if self.metadata_errors.read().unwrap().contains(&path_str) {
            return Err(format!("Simulated metadata error for: {}", path_str));
        }

        if let Some(data) = self.files.read().unwrap().get(&path_str) {
            Ok(FileMetadata {
                size: data.len() as u64,
                modified_secs: 0,
                is_dir: false,
                is_file: true,
            })
        } else if self.dirs.read().unwrap().contains(&path_str) {
            Ok(FileMetadata {
                size: 0,
                modified_secs: 0,
                is_dir: true,
                is_file: false,
            })
        } else {
            Err(format!("Path not found: {}", path_str))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_filesystem_read_write() {
        let fs = MockFileSystem::new();
        let path = Path::new("/test/file.txt");

        fs.write_file(path, b"Hello, World!").unwrap();
        let content = fs.read_file(path).unwrap();
        assert_eq!(content, b"Hello, World!");
    }

    #[test]
    fn mock_filesystem_exists() {
        let fs = MockFileSystem::new().with_file("/test/file.txt", b"content");
        assert!(fs.exists(Path::new("/test/file.txt")));
        assert!(!fs.exists(Path::new("/nonexistent")));
    }

    #[test]
    fn mock_filesystem_create_dir() {
        let fs = MockFileSystem::new();
        let path = Path::new("/test/dir");

        assert!(!fs.exists(path));
        fs.create_dir_all(path).unwrap();
        assert!(fs.exists(path));
    }

    #[test]
    fn mock_filesystem_remove_file() {
        let fs = MockFileSystem::new().with_file("/test/file.txt", b"content");
        let path = Path::new("/test/file.txt");

        assert!(fs.exists(path));
        fs.remove_file(path).unwrap();
        assert!(!fs.exists(path));
    }

    #[test]
    fn mock_filesystem_metadata() {
        let fs = MockFileSystem::new().with_file("/test/file.txt", b"Hello!");
        let meta = fs.metadata(Path::new("/test/file.txt")).unwrap();

        assert_eq!(meta.size, 6);
        assert!(meta.is_file);
        assert!(!meta.is_dir);
    }

    #[test]
    fn read_to_string_works() {
        let fs = MockFileSystem::new().with_file("/test.txt", b"Hello!");
        let content = fs.read_to_string(Path::new("/test.txt")).unwrap();
        assert_eq!(content, "Hello!");
    }

    #[test]
    fn write_string_works() {
        let fs = MockFileSystem::new();
        fs.write_string(Path::new("/test.txt"), "Hello!").unwrap();
        let content = fs.read_file(Path::new("/test.txt")).unwrap();
        assert_eq!(content, b"Hello!");
    }
}

// ============================================================================
// Real filesystem tests using StdFileSystem and tempfile
// ============================================================================

#[cfg(test)]
mod std_filesystem_tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn std_filesystem_new() {
        let fs = StdFileSystem::new();
        // Should be constructable without errors
        assert!(fs.exists(Path::new(".")));
    }

    #[test]
    fn std_filesystem_default() {
        let fs = StdFileSystem {};
        // Should be constructable via Default trait
        assert!(fs.exists(Path::new(".")));
    }

    #[test]
    fn read_file_works() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, b"Hello, World!").unwrap();

        let fs = StdFileSystem::new();
        let content = fs.read_file(&file_path).unwrap();
        assert_eq!(content, b"Hello, World!");
    }

    #[test]
    fn write_file_works() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        let fs = StdFileSystem::new();
        fs.write_file(&file_path, b"Test content").unwrap();

        let content = fs::read(&file_path).unwrap();
        assert_eq!(content, b"Test content");
    }

    #[test]
    fn exists_returns_true_for_existing_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("existing.txt");
        fs::write(&file_path, b"exists").unwrap();

        let fs = StdFileSystem::new();
        assert!(fs.exists(&file_path));
    }

    #[test]
    fn exists_returns_false_for_non_existing_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("nonexistent.txt");

        let fs = StdFileSystem::new();
        assert!(!fs.exists(&file_path));
    }

    #[test]
    fn exists_returns_true_for_directory() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().join("subdir");
        fs::create_dir(&dir_path).unwrap();

        let fs = StdFileSystem::new();
        assert!(fs.exists(&dir_path));
    }

    #[test]
    fn create_dir_all_creates_nested_directories() {
        let dir = tempdir().unwrap();
        let nested_path = dir.path().join("a").join("b").join("c");

        let fs = StdFileSystem::new();
        fs.create_dir_all(&nested_path).unwrap();

        assert!(fs.exists(&nested_path));
        let meta = fs.metadata(&nested_path).unwrap();
        assert!(meta.is_dir);
    }

    #[test]
    fn remove_file_deletes_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("to_delete.txt");
        fs::write(&file_path, b"delete me").unwrap();

        let fs = StdFileSystem::new();
        assert!(fs.exists(&file_path));

        fs.remove_file(&file_path).unwrap();
        assert!(!fs.exists(&file_path));
    }

    #[test]
    fn metadata_returns_correct_file_size() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("sized.txt");
        let content = b"12345";
        fs::write(&file_path, content).unwrap();

        let fs = StdFileSystem::new();
        let meta = fs.metadata(&file_path).unwrap();

        assert_eq!(meta.size, 5);
        assert!(meta.is_file);
        assert!(!meta.is_dir);
    }

    #[test]
    fn metadata_identifies_file_correctly() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        fs::write(&file_path, b"content").unwrap();

        let fs = StdFileSystem::new();
        let meta = fs.metadata(&file_path).unwrap();

        assert!(meta.is_file);
        assert!(!meta.is_dir);
    }

    #[test]
    fn metadata_identifies_directory_correctly() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().join("subdir");
        fs::create_dir(&dir_path).unwrap();

        let fs = StdFileSystem::new();
        let meta = fs.metadata(&dir_path).unwrap();

        assert!(meta.is_dir);
        assert!(!meta.is_file);
        assert_eq!(meta.size, 0); // Directories typically have 0 or small size
    }

    #[test]
    fn metadata_has_valid_modified_time() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("timed.txt");
        fs::write(&file_path, b"time").unwrap();

        let fs = StdFileSystem::new();
        let meta = fs.metadata(&file_path).unwrap();

        // Modified time should be a reasonable Unix timestamp (after 2020)
        assert!(meta.modified_secs > 1_600_000_000);
    }

    #[test]
    fn read_to_string_works() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("utf8.txt");
        fs::write(&file_path, "Hello, UTF-8! 🚀").unwrap();

        let fs = StdFileSystem::new();
        let content = fs.read_to_string(&file_path).unwrap();

        assert_eq!(content, "Hello, UTF-8! 🚀");
    }

    #[test]
    fn write_string_works() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("written.txt");

        let fs = StdFileSystem::new();
        fs.write_string(&file_path, "Written content").unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Written content");
    }

    #[test]
    fn read_file_error_for_non_existent() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("does_not_exist.txt");

        let fs = StdFileSystem::new();
        let result = fs.read_file(&file_path);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Failed to read file"));
    }

    #[test]
    fn remove_file_error_for_non_existent() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("not_there.txt");

        let fs = StdFileSystem::new();
        let result = fs.remove_file(&file_path);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Failed to remove file"));
    }

    #[test]
    fn metadata_error_for_non_existent() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("missing.txt");

        let fs = StdFileSystem::new();
        let result = fs.metadata(&file_path);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Failed to get metadata"));
    }

    #[test]
    fn read_to_string_error_on_invalid_utf8() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("invalid.txt");
        // Write invalid UTF-8 bytes
        fs::write(&file_path, [0xFF, 0xFE, 0xFD]).unwrap();

        let fs = StdFileSystem::new();
        let result = fs.read_to_string(&file_path);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Invalid UTF-8"));
    }

    #[test]
    fn write_and_read_binary_data() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("binary.dat");
        let binary_data = vec![0x00, 0x01, 0xFF, 0xAB, 0xCD, 0xEF];

        let fs = StdFileSystem::new();
        fs.write_file(&file_path, &binary_data).unwrap();

        let read_data = fs.read_file(&file_path).unwrap();
        assert_eq!(read_data, binary_data);
    }

    #[test]
    fn overwrite_existing_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("overwrite.txt");

        let fs = StdFileSystem::new();
        fs.write_string(&file_path, "First content").unwrap();
        assert_eq!(fs.read_to_string(&file_path).unwrap(), "First content");

        fs.write_string(&file_path, "Second content").unwrap();
        assert_eq!(fs.read_to_string(&file_path).unwrap(), "Second content");
    }
}
