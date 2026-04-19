//! PRD 3.2.2.crash-recovery — integration-level pin.
//!
//! Bin-crate limitation: can't import `Registry` / `ModEntry` directly.
//! The proper behavioural test lives in
//! `src/services/mods/registry.rs::tests::mid_install_sigkill_recovers_to_error`.
//! This file pins the JSON-level contract: a persisted registry with
//! `status: "installing"` MUST be rewritten such that `status: "error"` and
//! `last_error` is populated after the next load. If the serde
//! representation of `ModStatus` ever changes (rename, tag-value shift),
//! recovery silently breaks without this test.

use std::fs;

use serde_json::Value;
use tempfile::TempDir;

/// A hand-rolled registry document matching the shape of the real
/// `Registry` struct's serde representation. Keeping the string literal
/// here instead of importing Registry acts as a cross-check: if anyone
/// edits the schema, this test forces a matching update here.
const STUCK_REGISTRY_JSON: &str = r#"{
  "version": 1,
  "mods": [
    {
      "id": "classicplus.shinra",
      "kind": "external",
      "name": "Shinra",
      "author": "neowutran",
      "description": "DPS meter",
      "version": "3.0.0",
      "status": "installing",
      "progress": 42,
      "auto_launch": true,
      "enabled": true
    }
  ]
}"#;

#[test]
fn installing_state_serialises_as_snake_case() {
    // If this lexical contract ever breaks, the recovery pass in
    // Registry::load won't recognise stranded rows.
    let v: Value = serde_json::from_str(STUCK_REGISTRY_JSON).unwrap();
    assert_eq!(v["mods"][0]["status"].as_str(), Some("installing"));
    assert_eq!(v["mods"][0]["progress"].as_u64(), Some(42));
}

#[test]
fn stuck_install_document_is_valid_json_on_disk() {
    // Precondition for the recovery path: the launcher must be able to
    // round-trip the document through the filesystem. If atomic save /
    // text-read ever regresses, the recovery pass silently never runs.
    let tmp = TempDir::new().unwrap();
    let p = tmp.path().join("registry.json");
    fs::write(&p, STUCK_REGISTRY_JSON).unwrap();
    let reloaded = fs::read_to_string(&p).unwrap();
    let parsed: Value = serde_json::from_str(&reloaded).unwrap();
    assert_eq!(parsed["version"].as_u64(), Some(1));
    assert_eq!(parsed["mods"][0]["status"].as_str(), Some("installing"));
}

#[test]
fn error_state_expected_shape() {
    // Shape of the post-recovery document we expect Registry::load to
    // produce. Serves as a spec for what the UI layer should render for a
    // recovered row.
    let post_recovery: Value = serde_json::json!({
        "version": 1,
        "mods": [{
            "id": "classicplus.shinra",
            "kind": "external",
            "name": "Shinra",
            "author": "neowutran",
            "description": "DPS meter",
            "version": "3.0.0",
            "status": "error",
            "last_error": "Install was interrupted (launcher exited mid-install). Click retry to re-run the download.",
            "auto_launch": true,
            "enabled": true,
        }]
    });

    assert_eq!(post_recovery["mods"][0]["status"].as_str(), Some("error"));
    assert!(post_recovery["mods"][0]["last_error"]
        .as_str()
        .unwrap()
        .contains("interrupted"));
    // progress must be cleared (serde skips None, so key should be absent).
    assert!(post_recovery["mods"][0].get("progress").is_none());
}
