//! PRD 3.1.9 — refuse updater-driven downgrades.
//!
//! The v2 updater plugin's `check().await` returns a `Result<Option<Update>>`.
//! When the server (or an attacker who can reach the endpoint) advertises a
//! version that is not strictly newer than the currently-running binary,
//! `should_accept_update` returns `false` and the launcher logs + skips the
//! install. This blocks the canonical downgrade-to-known-vulnerable attack
//! against signed updaters.
//!
//! Policy:
//! - strict-less or equal current → refuse (downgrade or replay).
//! - pre-release semantics follow semver: `0.2.0-rc.1` < `0.2.0`.
//! - either side unparseable → refuse (conservative default; signals either
//!   a corrupted manifest or a server/client mismatch we can't safely decide).

use semver::Version;

/// Returns `true` iff `remote` is a strictly newer semver than `current`.
pub fn should_accept_update(current: &str, remote: &str) -> bool {
    let (Ok(c), Ok(r)) = (Version::parse(current), Version::parse(remote)) else {
        return false;
    };
    r > c
}

#[cfg(test)]
mod tests {
    use super::should_accept_update;

    #[test]
    fn newer_patch_accepted() {
        assert!(should_accept_update("0.1.12", "0.1.13"));
    }

    #[test]
    fn newer_minor_accepted() {
        assert!(should_accept_update("0.1.12", "0.2.0"));
    }

    #[test]
    fn newer_major_accepted() {
        assert!(should_accept_update("0.1.12", "1.0.0"));
    }

    #[test]
    fn equal_refused() {
        // Replay of the same version is a downgrade in the "force a
        // reinstall" sense — refuse by policy.
        assert!(!should_accept_update("0.1.12", "0.1.12"));
    }

    #[test]
    fn older_patch_refused() {
        assert!(!should_accept_update("0.1.12", "0.1.11"));
    }

    #[test]
    fn older_minor_refused() {
        assert!(!should_accept_update("0.2.0", "0.1.99"));
    }

    #[test]
    fn older_major_refused() {
        assert!(!should_accept_update("1.0.0", "0.9.9"));
    }

    #[test]
    fn prerelease_of_same_version_refused() {
        // semver: 0.2.0-rc.1 < 0.2.0. Accepting it would be a downgrade.
        assert!(!should_accept_update("0.2.0", "0.2.0-rc.1"));
    }

    #[test]
    fn stable_over_prerelease_accepted() {
        // User on rc.1, server ships final 0.2.0 → accept.
        assert!(should_accept_update("0.2.0-rc.1", "0.2.0"));
    }

    #[test]
    fn invalid_current_refused() {
        assert!(!should_accept_update("not-a-version", "0.2.0"));
    }

    #[test]
    fn invalid_remote_refused() {
        // Defensive: a malformed manifest version string must not bypass.
        assert!(!should_accept_update("0.1.12", "1.0.0-"));
    }

    #[test]
    fn empty_strings_refused() {
        assert!(!should_accept_update("", ""));
        assert!(!should_accept_update("0.1.12", ""));
        assert!(!should_accept_update("", "0.2.0"));
    }
}
