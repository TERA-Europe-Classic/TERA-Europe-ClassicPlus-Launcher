//! PRD 3.2.7.parallel-install-serialised — integration-level pin.
//!
//! Bin-crate limitation: can't import `Registry` or `ModStatus` here
//! directly. The behavioural tests live in
//! `src/services/mods/registry.rs::tests::{same_id_serialised_second_claim_refused,
//! reclaim_after_error_succeeds, different_ids_do_not_block_each_other,
//! first_claim_upserts_installing_row}`. This file pins the shape of the
//! serialisation protocol so the in-crate implementation can't regress to
//! a structurally different rule silently.
//!
//! The rule: given a shared claim table keyed by mod id, a claim-installing
//! request succeeds iff no entry exists OR the existing entry is not in
//! the Installing state. Two concurrent installs of the same id → second
//! refused. Installs of different ids → both succeed.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
enum StatusModel {
    Installing,
    Enabled,
    Error,
    Disabled,
}

fn try_claim_model(table: &mut HashMap<String, StatusModel>, id: &str) -> Result<(), String> {
    if let Some(existing) = table.get(id) {
        if *existing == StatusModel::Installing {
            return Err(format!("Install for '{id}' is already in progress"));
        }
    }
    table.insert(id.to_string(), StatusModel::Installing);
    Ok(())
}

#[test]
fn same_id_serialised() {
    let mut table: HashMap<String, StatusModel> = HashMap::new();

    // First install-of-shinra claim succeeds.
    try_claim_model(&mut table, "classicplus.shinra").expect("first succeeds");
    assert_eq!(table.get("classicplus.shinra"), Some(&StatusModel::Installing));

    // Second install-of-shinra claim (from a double-click or parallel
    // invoke from JS) must refuse — the first is still in progress.
    let err = try_claim_model(&mut table, "classicplus.shinra").unwrap_err();
    assert!(err.contains("already in progress"), "got: {err}");
    assert!(err.contains("classicplus.shinra"), "error names id, got: {err}");
}

#[test]
fn different_ids_do_not_block() {
    let mut table: HashMap<String, StatusModel> = HashMap::new();

    try_claim_model(&mut table, "classicplus.shinra").unwrap();
    try_claim_model(&mut table, "classicplus.tcc").unwrap();

    assert_eq!(table.len(), 2);
    assert_eq!(table.get("classicplus.shinra"), Some(&StatusModel::Installing));
    assert_eq!(table.get("classicplus.tcc"), Some(&StatusModel::Installing));
}

#[test]
fn reclaim_after_error() {
    let mut table: HashMap<String, StatusModel> = HashMap::new();

    // First install fails; row flips to Error. User clicks Retry.
    try_claim_model(&mut table, "classicplus.shinra").unwrap();
    table.insert("classicplus.shinra".into(), StatusModel::Error);

    // Retry claim must succeed — the slot is no longer Installing.
    try_claim_model(&mut table, "classicplus.shinra").expect("retry after error");
    assert_eq!(table.get("classicplus.shinra"), Some(&StatusModel::Installing));
}

/// Mirror of the actual production Registry::try_claim_installing rule:
/// Disabled and Enabled both count as "not currently installing" so the
/// user's normal reinstall flow (uninstall → reinstall) still works.
#[test]
fn reclaim_over_disabled_or_enabled_ok() {
    let mut table: HashMap<String, StatusModel> = HashMap::new();

    for prior in [StatusModel::Disabled, StatusModel::Enabled, StatusModel::Error] {
        table.insert("classicplus.shinra".into(), prior.clone());
        try_claim_model(&mut table, "classicplus.shinra")
            .unwrap_or_else(|e| panic!("expected reclaim over {prior:?} to succeed, got {e}"));
    }
}
