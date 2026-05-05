use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Information about a file that needs to be downloaded/updated
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileHashAlgorithm {
    Sha256,
    Md5,
}

impl Default for FileHashAlgorithm {
    fn default() -> Self {
        Self::Sha256
    }
}

/// How a downloaded file should be installed after transfer.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileInstallMode {
    Direct,
    LzmaCab,
}

impl Default for FileInstallMode {
    fn default() -> Self {
        Self::Direct
    }
}

/// Information about a file that needs to be downloaded/updated.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[derive(Default)]
pub struct FileInfo {
    /// Final game-relative path after installation.
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub url: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub existing_size: u64,
    /// Optional game-relative staging path used when the download is not the final file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_path: Option<String>,
    /// Expected size of the final installed file when it differs from downloaded bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_size: Option<u64>,
    /// Hash algorithm for verifying the final installed file.
    #[serde(default)]
    pub hash_algorithm: FileHashAlgorithm,
    /// Installation behavior for the downloaded payload.
    #[serde(default)]
    pub install_mode: FileInstallMode,
}


/// Helper function for serde skip_serializing_if
fn is_zero(v: &u64) -> bool {
    *v == 0
}

/// Cached file information for hash verification
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedFileInfo {
    pub hash: String,
    pub last_modified: SystemTime,
}

/// Global authentication information stored after login.
///
/// `auth_key` is session-sensitive — it authenticates every subsequent Portal
/// API call until logout. The struct derives `ZeroizeOnDrop` so the auth_key
/// buffer is overwritten when the struct drops (e.g. on logout reset or
/// process exit). Non-sensitive fields (`user_name`, `user_no`,
/// `character_count`) are `#[zeroize(skip)]` because wiping them serves no
/// security purpose.
#[derive(Default, Zeroize, ZeroizeOnDrop)]
pub struct GlobalAuthInfo {
    #[zeroize(skip)]
    pub character_count: String,
    #[zeroize(skip)]
    pub user_no: i32,
    #[zeroize(skip)]
    pub user_name: String,
    pub auth_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn is_zero_true_for_zero() {
        assert!(is_zero(&0));
    }

    #[test]
    fn is_zero_false_for_non_zero() {
        assert!(!is_zero(&1));
        assert!(!is_zero(&u64::MAX));
    }

    #[test]
    fn file_info_skips_zero_existing_size() {
        let info = FileInfo {
            path: "test.pak".to_string(),
            hash: "abc123".to_string(),
            size: 1000,
            url: "http://example.com/test.pak".to_string(),
            existing_size: 0,
            ..FileInfo::default()
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(!json.contains("existing_size"));
    }

    #[test]
    fn file_info_includes_non_zero_existing_size() {
        let info = FileInfo {
            path: "test.pak".to_string(),
            hash: "abc123".to_string(),
            size: 1000,
            url: "http://example.com/test.pak".to_string(),
            existing_size: 500,
            ..FileInfo::default()
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("existing_size"));
        assert!(json.contains("500"));
    }

    #[test]
    fn file_info_serialization_roundtrip() {
        let original = FileInfo {
            path: "data/test.pak".to_string(),
            hash: "deadbeef".to_string(),
            size: 2048,
            url: "https://cdn.example.com/files/test.pak".to_string(),
            existing_size: 1024,
            ..FileInfo::default()
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: FileInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(original.path, deserialized.path);
        assert_eq!(original.hash, deserialized.hash);
        assert_eq!(original.size, deserialized.size);
        assert_eq!(original.url, deserialized.url);
        assert_eq!(original.existing_size, deserialized.existing_size);
    }

    #[test]
    fn file_info_deserialization_defaults_existing_size() {
        let json = r#"{
            "path": "test.pak",
            "hash": "abc123",
            "size": 1000,
            "url": "http://example.com/test.pak"
        }"#;

        let info: FileInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.existing_size, 0);
        assert_eq!(info.hash_algorithm, FileHashAlgorithm::Sha256);
        assert_eq!(info.install_mode, FileInstallMode::Direct);
        assert_eq!(info.download_path, None);
        assert_eq!(info.output_size, None);
    }

    #[test]
    fn file_info_roundtrips_v100_install_metadata() {
        let original = FileInfo {
            path: "S1Game/S1Data/DataCenter_Final_EUR.dat".to_string(),
            hash: "dc193ac520efac09b9faefb7f46f2405".to_string(),
            size: 61_905_708,
            url: "http://157.90.107.2:8090/public/patch/patch/1-1.cab".to_string(),
            download_path: Some("$Patch/v100/patch/1-1.cab".to_string()),
            output_size: Some(61_076_240),
            hash_algorithm: FileHashAlgorithm::Md5,
            install_mode: FileInstallMode::LzmaCab,
            ..FileInfo::default()
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: FileInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.path, original.path);
        assert_eq!(deserialized.hash_algorithm, FileHashAlgorithm::Md5);
        assert_eq!(deserialized.install_mode, FileInstallMode::LzmaCab);
        assert_eq!(deserialized.download_path, original.download_path);
        assert_eq!(deserialized.output_size, original.output_size);
    }

    #[test]
    fn file_info_clone() {
        let original = FileInfo {
            path: "test.pak".to_string(),
            hash: "hash123".to_string(),
            size: 512,
            url: "http://example.com".to_string(),
            existing_size: 256,
            ..FileInfo::default()
        };

        let cloned = original.clone();
        assert_eq!(original.path, cloned.path);
        assert_eq!(original.hash, cloned.hash);
        assert_eq!(original.size, cloned.size);
        assert_eq!(original.url, cloned.url);
        assert_eq!(original.existing_size, cloned.existing_size);
    }

    #[test]
    fn cached_file_info_creation() {
        let now = SystemTime::now();
        let info = CachedFileInfo {
            hash: "test_hash".to_string(),
            last_modified: now,
        };

        assert_eq!(info.hash, "test_hash");
        assert_eq!(info.last_modified, now);
    }

    #[test]
    fn cached_file_info_clone() {
        let now = SystemTime::now();
        let original = CachedFileInfo {
            hash: "cached_hash".to_string(),
            last_modified: now,
        };

        let cloned = original.clone();
        assert_eq!(original.hash, cloned.hash);
        assert_eq!(original.last_modified, cloned.last_modified);
    }

    #[test]
    fn cached_file_info_serialization() {
        let now = SystemTime::now();
        let info = CachedFileInfo {
            hash: "serialized_hash".to_string(),
            last_modified: now,
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: CachedFileInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(info.hash, deserialized.hash);
        assert_eq!(info.last_modified, deserialized.last_modified);
    }

    #[test]
    fn global_auth_info_default() {
        let auth_info = GlobalAuthInfo::default();

        assert_eq!(auth_info.character_count, "");
        assert_eq!(auth_info.user_no, 0);
        assert_eq!(auth_info.user_name, "");
        assert_eq!(auth_info.auth_key, "");
    }

    #[test]
    fn global_auth_info_custom_values() {
        let auth_info = GlobalAuthInfo {
            character_count: "5".to_string(),
            user_no: 12345,
            user_name: "test_user".to_string(),
            auth_key: "secret_key_123".to_string(),
        };

        assert_eq!(auth_info.character_count, "5");
        assert_eq!(auth_info.user_no, 12345);
        assert_eq!(auth_info.user_name, "test_user");
        assert_eq!(auth_info.auth_key, "secret_key_123");
    }

    // --- PRD 3.1.7.zeroize-audit ------------------------------------------

    #[test]
    fn global_auth_info_zeroize_clears_auth_key() {
        let mut info = GlobalAuthInfo {
            character_count: "5".to_string(),
            user_no: 42,
            user_name: "keeper".to_string(),
            auth_key: "super-secret-auth-key".to_string(),
        };
        info.zeroize();
        // auth_key zeroed.
        assert!(
            info.auth_key.is_empty(),
            "auth_key must be empty after zeroize"
        );
        // Non-sensitive fields preserved (skipped by derive).
        assert_eq!(info.user_name, "keeper");
        assert_eq!(info.user_no, 42);
        assert_eq!(info.character_count, "5");
    }

    #[test]
    fn global_auth_info_implements_zeroize_on_drop() {
        // Compile-time bound: derived ZeroizeOnDrop guarantees Drop zeroes
        // auth_key. If the derive is removed, this won't compile.
        fn assert_zod<T: ZeroizeOnDrop>() {}
        assert_zod::<GlobalAuthInfo>();
    }
}
