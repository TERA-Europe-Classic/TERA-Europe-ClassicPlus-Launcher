//! Deep link state management for OAuth callback handling.
//!
//! When the launcher is opened via a `teraclassic://` deep link (e.g., after
//! OAuth consent in the browser), the URL is passed as a CLI argument by the OS.
//! This module stores that URL so the frontend can retrieve and process it.

use lazy_static::lazy_static;
use std::sync::RwLock;

lazy_static! {
    /// Stores a pending deep link URL received via CLI args on startup.
    /// Once read by the frontend, it is consumed (set to None).
    static ref PENDING_DEEP_LINK: RwLock<Option<String>> = RwLock::new(None);
}

/// Stores a deep link URL for later retrieval by the frontend.
pub fn set_pending_deep_link(url: String) {
    let mut guard = PENDING_DEEP_LINK.write().unwrap_or_else(|e| e.into_inner());
    *guard = Some(url);
}

/// Retrieves and consumes the pending deep link URL.
/// Returns `None` if no deep link is pending.
pub fn take_pending_deep_link() -> Option<String> {
    let mut guard = PENDING_DEEP_LINK.write().unwrap_or_else(|e| e.into_inner());
    guard.take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_take_deep_link() {
        // Clear any existing state
        let _ = take_pending_deep_link();

        set_pending_deep_link("teraclassic://auth?token=abc123&provider=google".to_string());

        let result = take_pending_deep_link();
        assert_eq!(
            result,
            Some("teraclassic://auth?token=abc123&provider=google".to_string())
        );

        // Second take should return None (consumed)
        let result2 = take_pending_deep_link();
        assert_eq!(result2, None);
    }

    #[test]
    fn test_take_without_set_returns_none() {
        // Clear any existing state
        let _ = take_pending_deep_link();

        let result = take_pending_deep_link();
        assert_eq!(result, None);
    }
}
