//! Smoke test harness for `teralaunch/src-tauri`.
//!
//! Proves the integration-test directory compiles and runs. Every P0/P1
//! integration test authored under `docs/PRD/fix-plan.md` lives alongside this
//! file. Shared fixtures go in `tests/common/mod.rs`.

mod common;

#[test]
fn smoke_runs() {
    assert_eq!(common::two_plus_two(), 4);
}

#[test]
fn tempdir_fixture_works() {
    let dir = common::scratch_dir();
    assert!(dir.path().exists());
}
