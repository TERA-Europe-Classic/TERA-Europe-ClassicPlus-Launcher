//! Shared fixtures for integration tests.
//!
//! Integration tests in `teralaunch/src-tauri/tests/` live in separate binary
//! crates; Cargo auto-discovers each top-level `tests/*.rs`. This module is
//! included as `mod common;` from each test binary that needs it.

use tempfile::TempDir;

pub fn two_plus_two() -> i32 {
    2 + 2
}

pub fn scratch_dir() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}
