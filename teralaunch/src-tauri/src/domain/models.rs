use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Information about a file that needs to be downloaded/updated
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileInfo {
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub url: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub existing_size: u64,
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

/// Global authentication information stored after login
#[derive(Default)]
pub struct GlobalAuthInfo {
    pub character_count: String,
    pub user_no: i32,
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
    }

    #[test]
    fn file_info_clone() {
        let original = FileInfo {
            path: "test.pak".to_string(),
            hash: "hash123".to_string(),
            size: 512,
            url: "http://example.com".to_string(),
            existing_size: 256,
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
}
