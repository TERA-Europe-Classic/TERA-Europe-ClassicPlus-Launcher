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
    assert_eq!(
        table.get("classicplus.shinra"),
        Some(&StatusModel::Installing)
    );

    // Second install-of-shinra claim (from a double-click or parallel
    // invoke from JS) must refuse — the first is still in progress.
    let err = try_claim_model(&mut table, "classicplus.shinra").unwrap_err();
    assert!(err.contains("already in progress"), "got: {err}");
    assert!(
        err.contains("classicplus.shinra"),
        "error names id, got: {err}"
    );
}

#[test]
fn different_ids_do_not_block() {
    let mut table: HashMap<String, StatusModel> = HashMap::new();

    try_claim_model(&mut table, "classicplus.shinra").unwrap();
    try_claim_model(&mut table, "classicplus.tcc").unwrap();

    assert_eq!(table.len(), 2);
    assert_eq!(
        table.get("classicplus.shinra"),
        Some(&StatusModel::Installing)
    );
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
    assert_eq!(
        table.get("classicplus.shinra"),
        Some(&StatusModel::Installing)
    );
}

/// Mirror of the actual production Registry::try_claim_installing rule:
/// Disabled and Enabled both count as "not currently installing" so the
/// user's normal reinstall flow (uninstall → reinstall) still works.
#[test]
fn reclaim_over_disabled_or_enabled_ok() {
    let mut table: HashMap<String, StatusModel> = HashMap::new();

    for prior in [
        StatusModel::Disabled,
        StatusModel::Enabled,
        StatusModel::Error,
    ] {
        table.insert("classicplus.shinra".into(), prior.clone());
        try_claim_model(&mut table, "classicplus.shinra")
            .unwrap_or_else(|e| panic!("expected reclaim over {prior:?} to succeed, got {e}"));
    }
}

// --------------------------------------------------------------------
// Iter 159 structural pins — locking primitive + predicate body +
// write-through save + error format.
// --------------------------------------------------------------------
//
// The models above prove the claim-table rule is correct by construction.
// These pins protect the production wiring: the process-global lock, the
// write-vs-read choice on `mutate`, the write-through save, the exact
// shape of `try_claim_installing` (widened refusal breaks retry), and
// the error format (vague message is useless when several mods race).

use std::fs;

const MODS_STATE_RS: &str = "src/state/mods_state.rs";
const REGISTRY_RS: &str = "src/services/mods/registry.rs";

fn mods_state_src() -> String {
    fs::read_to_string(MODS_STATE_RS)
        .unwrap_or_else(|e| panic!("{MODS_STATE_RS} must be readable: {e}"))
}

fn registry_src() -> String {
    fs::read_to_string(REGISTRY_RS)
        .unwrap_or_else(|e| panic!("{REGISTRY_RS} must be readable: {e}"))
}

/// `MODS_STATE` must be a process-global `RwLock<...>`. A refactor to
/// per-call `Mutex::new(...)` would break the cross-command
/// serialisation — two concurrent `install_*` calls would each get
/// their own lock and race. `RwLock` specifically (not plain `Mutex`)
/// matters because `list_mods` is read-only and should not contend
/// with other reads.
#[test]
fn mods_state_is_process_global_rwlock() {
    let body = mods_state_src();
    assert!(
        body.contains("static ref MODS_STATE: RwLock<"),
        "PRD 3.2.7: MODS_STATE must be a `static ref ... RwLock<...>` \
         (via lazy_static). Per-call locks defeat cross-command \
         serialisation; Mutex serialises reads unnecessarily."
    );
    assert!(
        body.contains("lazy_static!"),
        "PRD 3.2.7: mods_state.rs must use `lazy_static!` to scope \
         the process-global state. A bare `static` with runtime init \
         wouldn't compile here; losing this wrapper usually means the \
         lock moved to a struct field — which is per-instance, not \
         process-global."
    );
}

/// `mutate` must take the WRITE lock (`.write()`), not the read lock.
/// A refactor to `.read()` would let two `install_*` calls both see
/// "no Installing slot" and both upsert, breaking §3.2.7.
#[test]
fn mutate_takes_write_lock_not_read() {
    let body = mods_state_src();
    let fn_pos = body
        .find("pub fn mutate<F, T>")
        .expect("mutate must exist");
    let rest = &body[fn_pos..];
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(1200));
    let fn_body = &rest[..end];
    assert!(
        fn_body.contains("MODS_STATE\n        .write()") || fn_body.contains(".write()"),
        "PRD 3.2.7: mutate() must call `.write()` on MODS_STATE. \
         `.read()` defeats exclusion between parallel installs and \
         lets two claims both see `no Installing slot`.\n\
         Body:\n{fn_body}"
    );
    // And MUST NOT call `.read()` on the success path — that would
    // mean the function is not exclusive.
    assert!(
        !fn_body.contains(".read()"),
        "PRD 3.2.7: mutate() must not call `.read()` anywhere in its \
         body. Even a mid-function read-lock between writes introduces \
         a TOCTOU window where another writer can sneak in.\n\
         Body:\n{fn_body}"
    );
}

/// `mutate` must call `state.registry.save(...)` on the success path
/// BEFORE returning Ok. Without write-through persist, a crash
/// mid-flight leaves disk and memory diverged; on next boot the
/// recovery logic can't flip stranded Installing rows (the iter-153
/// Registry::load auto-recovery only sees what's on disk).
#[test]
fn mutate_saves_registry_write_through() {
    let body = mods_state_src();
    let fn_pos = body
        .find("pub fn mutate<F, T>")
        .expect("mutate must exist");
    let rest = &body[fn_pos..];
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(1200));
    let fn_body = &rest[..end];
    // The save must happen after the closure runs, and before Ok(result).
    let save_pos = fn_body.find("state.registry.save(");
    let ok_return_pos = fn_body.find("Ok(result)");
    assert!(
        save_pos.is_some(),
        "PRD 3.2.7: mutate() must call `state.registry.save(...)` so \
         every mutation persists before releasing the lock. Without \
         write-through, a crash mid-install leaves the registry \
         inconsistent with disk.\nBody:\n{fn_body}"
    );
    if let (Some(sp), Some(op)) = (save_pos, ok_return_pos) {
        assert!(
            sp < op,
            "PRD 3.2.7: `state.registry.save(...)` must run BEFORE \
             `Ok(result)` returns. A save-after-return would leak a \
             success status while disk is still stale."
        );
    }
}

/// `try_claim_installing` must refuse ONLY when the existing row is
/// `ModStatus::Installing`. Widening to
/// `ModStatus::Installing | ModStatus::Error` (or any other variant)
/// breaks the retry-after-error flow that
/// `reclaim_over_disabled_or_enabled_ok` above depends on. Narrowing
/// (e.g. removing the check entirely) re-opens the concurrent-install
/// race.
#[test]
fn try_claim_installing_refuses_only_on_installing() {
    let body = registry_src();
    let fn_pos = body
        .find("pub fn try_claim_installing(")
        .expect("try_claim_installing must exist");
    let rest = &body[fn_pos..];
    let end = rest.find("\n    }\n").unwrap_or(rest.len().min(1200));
    let fn_body = &rest[..end];
    assert!(
        fn_body.contains("matches!(slot.status, ModStatus::Installing)"),
        "PRD 3.2.7: try_claim_installing must refuse exclusively on \
         `matches!(slot.status, ModStatus::Installing)`. Widening \
         (e.g. `| ModStatus::Error`) breaks retry-after-error; \
         narrowing opens the install race.\nBody:\n{fn_body}"
    );
}

/// `try_claim_installing`'s refusal error must contain the id so
/// users see WHICH mod is stuck when multiple install flows collide.
/// A generic "install in progress" is operator-useless when the UI
/// races two different installs.
#[test]
fn try_claim_installing_error_names_the_mod_id() {
    let body = registry_src();
    let fn_pos = body
        .find("pub fn try_claim_installing(")
        .expect("try_claim_installing must exist");
    let rest = &body[fn_pos..];
    let end = rest.find("\n    }\n").unwrap_or(rest.len().min(1200));
    let fn_body = &rest[..end];
    // The error must interpolate `row.id` AND carry the
    // "already in progress" phrase (matches the behavioural test
    // `same_id_serialised` above).
    assert!(
        fn_body.contains("row.id"),
        "PRD 3.2.7: try_claim_installing error must interpolate \
         `row.id` so users see which mod is stuck.\nBody:\n{fn_body}"
    );
    assert!(
        fn_body.contains("already in progress"),
        "PRD 3.2.7: try_claim_installing error must carry the \
         `already in progress` phrase — the behavioural test \
         `same_id_serialised` pins this string, changing it here \
         without updating that test silently breaks the contract.\n\
         Body:\n{fn_body}"
    );
}

// --------------------------------------------------------------------
// Iter 202 structural pins — guard traceability + error-short-circuits-
// save + ensure_loaded ordering + upsert-on-success + poisoned-lock
// signal.
// --------------------------------------------------------------------

const GUARD_SOURCE: &str = "tests/parallel_install.rs";

/// Iter 202: guard source header must cite `PRD 3.2.7` + the
/// `parallel-install-serialised` criterion name so both are
/// reachable via grep.
#[test]
fn guard_file_header_cites_prd_and_parallel_install_name() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.2.7"),
        "PRD 3.2.7 (iter 202): {GUARD_SOURCE} header must cite \
         `PRD 3.2.7` so the criterion is reachable via grep."
    );
    assert!(
        header.contains("parallel-install-serialised"),
        "PRD 3.2.7 (iter 202): {GUARD_SOURCE} header must cite \
         `parallel-install-serialised` so the criterion nomenclature \
         is reachable via grep."
    );
}

/// Iter 202: `mutate` must short-circuit on closure Err BEFORE
/// `state.registry.save(...)` runs. The production shape is
/// `let result = f(&mut state.registry)?;` followed by `save(...)?`
/// — the `?` on the closure call propagates Err and skips save, so
/// a failed mutation doesn't persist a broken registry state.
#[test]
fn mutate_closure_err_short_circuits_before_save() {
    let body = mods_state_src();
    let fn_pos = body
        .find("pub fn mutate<F, T>")
        .expect("mutate must exist");
    let rest = &body[fn_pos..];
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(1200));
    let fn_body = &rest[..end];
    // The closure call must carry a `?` immediately (no let-binding
    // that silently eats the Err).
    assert!(
        fn_body.contains("let result = f(&mut state.registry)?;"),
        "PRD 3.2.7 (iter 202): mutate must use \
         `let result = f(&mut state.registry)?;` — the `?` \
         propagates closure Err BEFORE save. Without it, a failed \
         mutation persists through to disk. Body:\n{fn_body}"
    );
    // Ordering: `f(...)?` must appear BEFORE `state.registry.save(...)`.
    let closure_idx = fn_body
        .find("f(&mut state.registry)?")
        .expect("closure call must exist with `?`");
    let save_idx = fn_body
        .find("state.registry.save(")
        .expect("save call must exist");
    assert!(
        closure_idx < save_idx,
        "PRD 3.2.7 (iter 202): `f(&mut state.registry)?` must run \
         BEFORE `state.registry.save(...)` — otherwise a closure \
         error can't skip the persist step."
    );
}

/// Iter 202: `mutate` must call `ensure_loaded()?` BEFORE acquiring
/// the write lock on `MODS_STATE`. Acquiring the lock first would
/// deadlock if `ensure_loaded` itself needs to acquire (it
/// typically does, to populate the Option-wrapped state). Pin the
/// ordering so a refactor to `write()` first doesn't ship a
/// latent deadlock.
#[test]
fn mutate_calls_ensure_loaded_before_write_lock() {
    let body = mods_state_src();
    let fn_pos = body
        .find("pub fn mutate<F, T>")
        .expect("mutate must exist");
    let rest = &body[fn_pos..];
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(1200));
    let fn_body = &rest[..end];
    let ensure_idx = fn_body
        .find("ensure_loaded()?")
        .expect("mutate must call ensure_loaded()?");
    let lock_idx = fn_body
        .find("MODS_STATE\n        .write()")
        .or_else(|| fn_body.find(".write()"))
        .expect("mutate must call .write() on MODS_STATE");
    assert!(
        ensure_idx < lock_idx,
        "PRD 3.2.7 (iter 202): `ensure_loaded()?` must run BEFORE \
         `MODS_STATE.write()` in mutate(). If ensure_loaded itself \
         acquires the lock (to populate the state), calling write() \
         first creates a latent deadlock. Body:\n{fn_body}"
    );
}

/// Iter 202: `try_claim_installing`'s success path must insert the
/// Installing row via `self.upsert(row)` after the refusal check.
/// A claim that "succeeds" without tracking a row defeats the
/// whole serialisation contract — subsequent claims for the same
/// id would see no Installing row and racing installs would both
/// proceed.
#[test]
fn try_claim_installing_upserts_row_on_success() {
    let body = registry_src();
    let fn_pos = body
        .find("pub fn try_claim_installing(")
        .expect("try_claim_installing must exist");
    let rest = &body[fn_pos..];
    let end = rest.find("\n    }\n").unwrap_or(rest.len().min(1200));
    let fn_body = &rest[..end];
    assert!(
        fn_body.contains("self.upsert(row)"),
        "PRD 3.2.7 (iter 202): try_claim_installing must call \
         `self.upsert(row)` on the success path — otherwise the \
         claim 'succeeds' but no Installing row is tracked, \
         defeating the serialisation contract.\nBody:\n{fn_body}"
    );
    // Ordering: upsert must come AFTER the `if matches!(...)
    // Installing)` refusal block, not before. If upsert ran first,
    // every claim would always upsert even on refusal.
    let refusal_idx = fn_body
        .find("matches!(slot.status, ModStatus::Installing)")
        .expect("refusal check must exist");
    let upsert_idx = fn_body
        .find("self.upsert(row)")
        .expect("upsert call must exist");
    assert!(
        refusal_idx < upsert_idx,
        "PRD 3.2.7 (iter 202): the refusal check (at offset \
         {refusal_idx}) must precede `self.upsert(row)` (at offset \
         {upsert_idx}). An upsert-before-check would overwrite an \
         already-Installing row, defeating serialisation."
    );
}

/// Iter 202: `mutate`'s write-lock acquisition must map
/// `PoisonError` to a user-facing `Mods state poisoned` message.
/// Without the mapping, a prior panic that left the lock poisoned
/// surfaces as a generic IO-like error — operators can't
/// distinguish "state corrupted by an earlier bug" from a routine
/// IO failure, which blocks root-cause analysis.
#[test]
fn mutate_surfaces_poisoned_lock_error_explicitly() {
    let body = mods_state_src();
    let fn_pos = body
        .find("pub fn mutate<F, T>")
        .expect("mutate must exist");
    let rest = &body[fn_pos..];
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(1200));
    let fn_body = &rest[..end];
    assert!(
        fn_body.contains("Mods state poisoned"),
        "PRD 3.2.7 (iter 202): mutate must surface \
         `Mods state poisoned` in its PoisonError mapping. Without \
         this specific phrase, a prior-panic-poisoned lock looks \
         like a generic IO error — operators can't tell corrupted \
         state from transient IO.\nBody:\n{fn_body}"
    );
    assert!(
        fn_body.contains(".map_err(|e|") || fn_body.contains(".map_err(|"),
        "PRD 3.2.7 (iter 202): the PoisonError surface must go \
         through `.map_err(|e| format!(...))` — a bare `.unwrap()` \
         on a poisoned lock panics the process instead of surfacing \
         the condition to the user.\nBody:\n{fn_body}"
    );
}
