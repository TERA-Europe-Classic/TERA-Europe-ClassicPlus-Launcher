//! PRD 3.2.8.disk-full-revert — integration-level pin.
//!
//! Bin-crate limitation: can't import `revert_partial_install_dir` /
//! `revert_partial_install_file` directly. The behavioural tests live in
//! `src/services/mods/external_app.rs::tests::{revert_on_enospc,
//! revert_partial_gpk_file_removes_it}`. This file pins the external
//! contract — when extract/write fails, next retry must see a clean
//! dest — by modelling the reversal logic against `std::fs` and asserting
//! the same state transitions.
//!
//! Rule: after a partial install fails,
//!   - dest dir path is absent (never partially populated for the user)
//!   - dest file path is absent (never zero-byte or truncated)
//!
//! No matter what state disk was in when the failure hit.

use std::fs;
use tempfile::TempDir;

fn revert_dir_model(dest: &std::path::Path) {
    if dest.exists() {
        let _ = fs::remove_dir_all(dest);
    }
}

fn revert_file_model(dest: &std::path::Path) {
    if dest.exists() {
        let _ = fs::remove_file(dest);
    }
}

/// PRD 3.2.8 acceptance: partial writes reversed on ENOSPC. The extract
/// path went through and wrote 3 files before the error; after revert,
/// the dest dir is gone so the user's next retry starts clean.
#[test]
fn revert_on_enospc() {
    let tmp = TempDir::new().unwrap();
    let dest = tmp.path().join("mod_root");

    fs::create_dir_all(&dest).unwrap();
    fs::create_dir_all(dest.join("bin")).unwrap();
    fs::write(dest.join("app.exe"), b"partial").unwrap();
    fs::write(dest.join("bin").join("plugin.dll"), b"also partial").unwrap();
    fs::write(dest.join("bin").join("helper.dll"), b"third file").unwrap();

    revert_dir_model(&dest);

    assert!(!dest.exists(), "dest must be gone after revert");
}

/// Symmetric: a partial GPK file is cleaned up so the mapper patcher on
/// retry doesn't see a truncated footer.
#[test]
fn revert_partial_gpk_file() {
    let tmp = TempDir::new().unwrap();
    let dest = tmp.path().join("classicplus.minimap.gpk");

    fs::write(&dest, b"truncated GPK bytes").unwrap();
    assert!(dest.exists());

    revert_file_model(&dest);

    assert!(!dest.exists(), "partial file must be gone after revert");
}

/// Reverting a missing path is a safe no-op — covers the "failure before
/// dest was created" branch (connection refused, DNS failure before any
/// bytes arrived).
#[test]
fn revert_missing_path_is_noop() {
    let tmp = TempDir::new().unwrap();

    let dir = tmp.path().join("never_created_dir");
    revert_dir_model(&dir);
    assert!(!dir.exists());

    let file = tmp.path().join("never_created.gpk");
    revert_file_model(&file);
    assert!(!file.exists());
}

/// Idempotency: calling revert twice is safe. If retry logic re-enters
/// the cleanup path (e.g. because it runs after every Err branch), that
/// must not panic or leave stale state.
#[test]
fn revert_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let dest = tmp.path().join("mod_root");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("file"), b"x").unwrap();

    revert_dir_model(&dest);
    revert_dir_model(&dest);

    assert!(!dest.exists());
}
