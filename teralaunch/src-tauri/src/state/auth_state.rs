//! Global authentication state management.
//!
//! This module provides thread-safe access to the global authentication
//! information used after user login.

use lazy_static::lazy_static;
use std::sync::RwLock;

use crate::domain::GlobalAuthInfo;

lazy_static! {
    static ref GLOBAL_AUTH_INFO: RwLock<GlobalAuthInfo> = RwLock::new(GlobalAuthInfo::default());
}

/// Returns a clone of the current global authentication info.
/// Returns `None` if the lock is poisoned.
#[allow(dead_code)]
pub fn get_auth_info() -> Option<GlobalAuthInfo> {
    GLOBAL_AUTH_INFO.read().ok().map(|guard| GlobalAuthInfo {
        auth_key: guard.auth_key.clone(),
        user_name: guard.user_name.clone(),
        user_no: guard.user_no,
        character_count: guard.character_count.clone(),
    })
}

/// Sets the global authentication info.
/// Recovers from poisoned lock by using `into_inner`.
pub fn set_auth_info(info: GlobalAuthInfo) {
    let mut guard = GLOBAL_AUTH_INFO.write().unwrap_or_else(|e| e.into_inner());
    guard.auth_key = info.auth_key;
    guard.user_name = info.user_name;
    guard.user_no = info.user_no;
    guard.character_count = info.character_count;
}

/// Clears the global authentication info (resets to default).
/// Recovers from poisoned lock by using `into_inner`.
pub fn clear_auth_info() {
    let mut guard = GLOBAL_AUTH_INFO.write().unwrap_or_else(|e| e.into_inner());
    *guard = GlobalAuthInfo::default();
}

/// Returns a read guard to the global authentication info.
/// Recovers from poisoned lock by using `into_inner`.
pub fn read_auth_info() -> impl std::ops::Deref<Target = GlobalAuthInfo> + 'static {
    GLOBAL_AUTH_INFO.read().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Tests modify global state, so they may interfere if run in parallel.
    // Use `cargo test -- --test-threads=1` if flaky.

    #[test]
    fn test_set_and_get_auth_info() {
        // Clear first to ensure clean state
        clear_auth_info();

        let info = GlobalAuthInfo {
            auth_key: "test_key".to_string(),
            user_name: "test_user".to_string(),
            user_no: 42,
            character_count: "3".to_string(),
        };
        set_auth_info(info);

        let retrieved = get_auth_info().expect("Should retrieve auth info");
        assert_eq!(retrieved.auth_key, "test_key");
        assert_eq!(retrieved.user_name, "test_user");
        assert_eq!(retrieved.user_no, 42);
        assert_eq!(retrieved.character_count, "3");
    }

    #[test]
    fn test_clear_auth_info() {
        let info = GlobalAuthInfo {
            auth_key: "to_clear".to_string(),
            user_name: "user".to_string(),
            user_no: 99,
            character_count: "5".to_string(),
        };
        set_auth_info(info);

        clear_auth_info();

        let retrieved = get_auth_info().expect("Should retrieve auth info");
        assert_eq!(retrieved.auth_key, "");
        assert_eq!(retrieved.user_name, "");
        assert_eq!(retrieved.user_no, 0);
        assert_eq!(retrieved.character_count, "");
    }

    #[test]
    fn test_read_auth_info_guard() {
        clear_auth_info();
        let info = GlobalAuthInfo {
            auth_key: "guard_key".to_string(),
            user_name: "guard_user".to_string(),
            user_no: 10,
            character_count: "1".to_string(),
        };
        set_auth_info(info);

        let guard = read_auth_info();
        assert_eq!(guard.auth_key, "guard_key");
        assert_eq!(guard.user_no, 10);
    }

    #[test]
    fn test_write_auth_info_guard() {
        clear_auth_info();

        {
            let mut guard = write_auth_info();
            guard.auth_key = "modified_key".to_string();
            guard.user_no = 77;
        }

        let retrieved = get_auth_info().expect("Should retrieve auth info");
        assert_eq!(retrieved.auth_key, "modified_key");
        assert_eq!(retrieved.user_no, 77);
    }

    #[test]
    fn test_default_auth_info() {
        clear_auth_info();

        let retrieved = get_auth_info().expect("Should retrieve auth info");
        assert_eq!(retrieved.auth_key, "");
        assert_eq!(retrieved.user_name, "");
        assert_eq!(retrieved.user_no, 0);
        assert_eq!(retrieved.character_count, "");
    }
}
