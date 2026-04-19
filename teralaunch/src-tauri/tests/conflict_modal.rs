//! fix.conflict-modal-wiring — wiring guard for
//! `commands::mods::preview_mod_install_conflicts`.
//!
//! Pure predicate tests live inline in
//! `src/services/mods/tmm.rs::tests` (six detect_conflicts cases
//! covering vanilla / self-reinstall / mixed / multi-slot / missing).
//! This file only guards the Tauri command layer + the bundle helper
//! `preview_conflicts_from_bytes` that glues the fs/decrypt/parse path
//! together for the command body.

use std::fs;

/// The Tauri command must be annotated `#[tauri::command]` with the
/// attribute directly adjacent to the fn decl, delegate to
/// `tmm::preview_conflicts_from_bytes`, resolve the game root via the
/// shared helper, and return `Vec<ModConflict>`.
#[test]
fn preview_mod_install_conflicts_is_a_tauri_command_and_delegates_to_tmm() {
    let mods_rs =
        fs::read_to_string("src/commands/mods.rs").expect("commands/mods.rs must exist");

    let decl_pos = mods_rs
        .find("pub async fn preview_mod_install_conflicts")
        .expect("commands/mods.rs must declare preview_mod_install_conflicts");

    let pre = &mods_rs[..decl_pos];
    let last_attr = pre.rfind("#[tauri::command]").expect(
        "preview_mod_install_conflicts must be annotated #[tauri::command] — \
         otherwise generate_handler! can't register it",
    );
    assert!(
        decl_pos - last_attr < 200,
        "#[tauri::command] attribute must be adjacent to \
         preview_mod_install_conflicts — found {} chars away",
        decl_pos - last_attr
    );

    // Inspect the body window after the decl for the expected delegate
    // calls. The bundle-helper is where the actual fs/decrypt/parse
    // chain lives — the command body must call through to it rather
    // than re-implementing the path inline.
    let body_window = &mods_rs[decl_pos..decl_pos.saturating_add(1200)];
    assert!(
        body_window.contains("tmm::preview_conflicts_from_bytes"),
        "preview_mod_install_conflicts must delegate to \
         tmm::preview_conflicts_from_bytes (bundle helper)"
    );
    assert!(
        body_window.contains("resolve_game_root"),
        "preview_mod_install_conflicts must resolve game root via the \
         shared resolve_game_root helper (same error shape as install/uninstall)"
    );
    assert!(
        body_window.contains("Vec<ModConflict>"),
        "preview_mod_install_conflicts must return Vec<ModConflict> so \
         the frontend modal can iterate slots"
    );
}

/// The command must be registered in `main.rs::generate_handler!` or
/// the frontend's invoke() resolves at runtime with "command not found."
#[test]
fn preview_mod_install_conflicts_is_registered_in_main_generate_handler() {
    let main_rs = fs::read_to_string("src/main.rs").expect("main.rs must exist");

    assert!(
        main_rs.contains("commands::mods::preview_mod_install_conflicts"),
        "main.rs::generate_handler! must register \
         commands::mods::preview_mod_install_conflicts — otherwise \
         invoke('preview_mod_install_conflicts') fails at runtime"
    );
}

/// `ModConflict` must derive `serde::Serialize` so Tauri can return
/// `Vec<ModConflict>` across the IPC boundary without a manual impl.
/// Guard against someone dropping the derive during a refactor.
#[test]
fn mod_conflict_is_serializable_across_the_ipc_boundary() {
    let tmm_rs =
        fs::read_to_string("src/services/mods/tmm.rs").expect("services/mods/tmm.rs must exist");

    let decl_pos = tmm_rs
        .find("pub struct ModConflict")
        .expect("tmm.rs must still define ModConflict");

    // Look back 400 chars for the derive attribute (doc comment +
    // derive line together fit comfortably in that window).
    let pre = &tmm_rs[decl_pos.saturating_sub(400)..decl_pos];
    assert!(
        pre.contains("Serialize"),
        "ModConflict must derive serde::Serialize so Tauri can return \
         Vec<ModConflict> — grep the struct's derive attribute"
    );
}

/// `preview_conflicts_from_bytes` bundle helper must keep its current
/// "missing .clean → empty result, not error" behaviour. Without this,
/// a user who hasn't yet triggered clean-recovery would get an error
/// instead of the best-effort preview — degrading UX.
#[test]
fn preview_conflicts_from_bytes_is_best_effort_on_missing_backup() {
    let tmm_rs =
        fs::read_to_string("src/services/mods/tmm.rs").expect("services/mods/tmm.rs must exist");

    let decl_pos = tmm_rs
        .find("pub fn preview_conflicts_from_bytes")
        .expect("tmm.rs must define preview_conflicts_from_bytes");

    // The "return Ok(Vec::new())" on missing backup must appear in the
    // first ~500 chars of the body — i.e. as an early-return before any
    // potentially-erroring I/O. Guards against a refactor that moves
    // the check after a fallible `fs::read(&backup)` call.
    let body_window = &tmm_rs[decl_pos..decl_pos.saturating_add(500)];
    let backup_exists_pos = body_window
        .find("backup.exists()")
        .expect("preview_conflicts_from_bytes must check backup.exists() before reading");
    let ok_empty_pos = body_window
        .find("return Ok(Vec::new())")
        .expect("preview_conflicts_from_bytes must return Ok(Vec::new()) on missing backup");
    assert!(
        backup_exists_pos < ok_empty_pos,
        "the missing-backup check must appear before the empty-vec \
         early return"
    );
}

// --------------------------------------------------------------------
// Iter 162 structural pins — detect_conflicts predicate body + struct
// shape + Windows-case semantics.
// --------------------------------------------------------------------
//
// The tests above guard the Tauri command wiring + the bundle helper.
// These pins defend the pure-predicate body of `detect_conflicts` —
// the classifier at the heart of the GPK conflict UX. A case-sensitive
// compare, a missing-slot misclassification, or an asymmetric
// region_lock branch would each produce wrong conflict modals (false
// positives over-reporting slots, false negatives missing real
// collisions).

const TMM_RS: &str = "src/services/mods/tmm.rs";

fn tmm_src() -> String {
    fs::read_to_string(TMM_RS).unwrap_or_else(|e| panic!("{TMM_RS} must be readable: {e}"))
}

/// Returns the body of `detect_conflicts` as a slice — from the `pub fn`
/// up to the trailing `\n}\n` that closes the fn.
fn detect_conflicts_body(src: &str) -> &str {
    let fn_pos = src
        .find("pub fn detect_conflicts(")
        .expect("detect_conflicts must exist");
    let rest = &src[fn_pos..];
    // The function body spans ~40 lines; 1600 chars is safely past the close.
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(1600));
    &rest[..end]
}

/// `detect_conflicts` must take `&HashMap<..., MapperEntry>` twice plus
/// `&ModFile` and return `Vec<ModConflict>`. Mutating either map would
/// violate the "pure predicate" contract the inline unit tests depend
/// on; returning `Result<...>` would push error-handling into the call
/// site and hide the no-conflict case.
#[test]
fn detect_conflicts_signature_is_two_maps_plus_modfile_to_vec() {
    let src = tmm_src().replace("\r\n", "\n");
    assert!(
        src.contains(
            "pub fn detect_conflicts(\n    vanilla_map: &HashMap<String, MapperEntry>,\n    current_map: &HashMap<String, MapperEntry>,\n    incoming: &ModFile,\n) -> Vec<ModConflict>"
        ),
        "PRD §3.2 / fix.conflict-modal-wiring: detect_conflicts must \
         keep its pure-predicate signature — `(&HashMap, &HashMap, \
         &ModFile) -> Vec<ModConflict>`. Changing to `&mut` defeats \
         the pure contract; changing to `Result<...>` hides the \
         no-conflict path."
    );
}

/// `ModConflict` must carry exactly the three fields the frontend
/// modal renders: `composite_name`, `object_path`, `previous_filename`.
/// Removing any one breaks the modal's display; the bundle helper
/// would still return items but the UI would show an incomplete row.
#[test]
fn mod_conflict_has_three_string_fields_for_ui() {
    let src = tmm_src();
    let decl_pos = src
        .find("pub struct ModConflict")
        .expect("ModConflict struct must exist");
    let rest = &src[decl_pos..];
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(500));
    let body = &rest[..end];
    for field in [
        "pub composite_name: String,",
        "pub object_path: String,",
        "pub previous_filename: String,",
    ] {
        assert!(
            body.contains(field),
            "PRD §3.2: ModConflict must declare `{field}` — the \
             frontend modal reads all three fields.\n\
             Struct body:\n{body}"
        );
    }
}

/// The filename comparisons in `detect_conflicts` must be
/// case-insensitive (`.eq_ignore_ascii_case(...)`) because Windows
/// treats `Shinra.gpk` and `shinra.gpk` as the same file. A refactor
/// to `==` would falsely flag a self-reinstall (different case) as a
/// foreign-mod conflict, forcing a confusing modal on legit reinstalls.
#[test]
fn detect_conflicts_uses_case_insensitive_filename_compare() {
    let src = tmm_src();
    let body = detect_conflicts_body(&src);
    // Both checks must use `.eq_ignore_ascii_case(` — one for vanilla
    // baseline, one for the self-reinstall short-circuit.
    let case_insensitive_count = body.matches(".eq_ignore_ascii_case(").count();
    assert!(
        case_insensitive_count >= 2,
        "PRD §3.2 (Windows filename semantics): detect_conflicts \
         body must use `.eq_ignore_ascii_case(` for BOTH the vanilla-\
         unchanged check AND the self-reinstall check. Found \
         {case_insensitive_count}; expected ≥ 2. A case-sensitive \
         `==` falsely reports `Shinra.gpk` vs `shinra.gpk` as a \
         conflict.\nBody:\n{body}"
    );
}

/// The missing-slot branch must skip via `continue`, NOT push a
/// conflict. A slot absent from `current_map` means the current
/// mapper doesn't cover that object_path at all — which is a
/// different failure class (install_gpk raises on missing slot).
/// If detect_conflicts misclassified missing-slot as conflict, the
/// user would see a ghost modal for something that isn't actually
/// a mod collision.
#[test]
fn detect_conflicts_skips_missing_current_slots() {
    let src = tmm_src();
    let body = detect_conflicts_body(&src);
    // The match must have a None arm that short-circuits with continue.
    assert!(
        body.contains("None => continue"),
        "PRD §3.2: detect_conflicts must short-circuit via `None => \
         continue` when the current mapper lacks the slot. Treating \
         a missing slot as a conflict would over-report.\n\
         Body:\n{body}"
    );
}

/// Both the `current` lookup and the `vanilla` lookup must gate on
/// `incoming.region_lock`, mirroring the same semantics that
/// `install_gpk` uses. Asymmetry (e.g. current gated, vanilla always
/// exact) would compare entries from two different lookup regimes
/// and silently mis-identify vanilla-unchanged slots.
#[test]
fn detect_conflicts_gates_lookup_on_region_lock_both_sides() {
    let src = tmm_src();
    let body = detect_conflicts_body(&src);
    let region_lock_branches = body.matches("if incoming.region_lock {").count();
    assert!(
        region_lock_branches >= 2,
        "PRD §3.2: detect_conflicts must branch on \
         `if incoming.region_lock {{` at least TWICE — once for the \
         current_map lookup, once for the vanilla_map lookup. \
         Asymmetric gating compares entries from different lookup \
         regimes and mis-identifies vanilla-unchanged slots.\n\
         Body:\n{body}"
    );
    // And each branch must pick between the two lookup helpers.
    assert!(
        body.contains("get_entry_by_object_path(")
            && body.contains("get_entry_by_incomplete_object_path("),
        "PRD §3.2: detect_conflicts must call BOTH \
         `get_entry_by_object_path` (region-locked) and \
         `get_entry_by_incomplete_object_path` (loose). Dropping \
         either arm breaks the region_lock routing.\n\
         Body:\n{body}"
    );
}

// --------------------------------------------------------------------
// Iter 193 structural pins — command purity + loop shape + struct
// exactness + guard header traceability.
// --------------------------------------------------------------------

const COMMANDS_MODS_RS: &str = "src/commands/mods.rs";
const GUARD_SOURCE: &str = "tests/conflict_modal.rs";

/// Iter 193: guard file header must cite `fix.conflict-modal-wiring`
/// so the fix-plan P-slot is reachable via grep. Without the
/// citation, a maintainer might relax a pin thinking the purpose
/// was obvious.
#[test]
fn guard_file_header_cites_fix_slot() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("fix.conflict-modal-wiring"),
        "fix.conflict-modal-wiring (iter 193): {GUARD_SOURCE} header \
         must cite `fix.conflict-modal-wiring` so the fix-plan P-slot \
         is reachable via grep."
    );
}

/// Iter 193: `preview_mod_install_conflicts` must be pure read-only.
/// A preview command that mutates state (registry upsert, mapper
/// write, backup touch) would surprise users who expect "preview"
/// to be side-effect-free. Reject any write-classed call.
#[test]
fn preview_command_has_no_write_side_effects() {
    let src = fs::read_to_string(COMMANDS_MODS_RS)
        .expect("commands/mods.rs must exist")
        .replace("\r\n", "\n");
    let fn_pos = src
        .find("pub async fn preview_mod_install_conflicts")
        .expect("preview_mod_install_conflicts must exist");
    let end = src[fn_pos..]
        .find("\n}\n")
        .map(|offset| fn_pos + offset)
        .expect("preview_mod_install_conflicts body must close with `}`");
    let body = &src[fn_pos..end];
    for forbidden in [
        "mods_state::mutate",
        "reg.upsert(",
        "fs::write(",
        "std::fs::write(",
        "ensure_backup(",
        "install_gpk(",
        "try_deploy_gpk(",
    ] {
        assert!(
            !body.contains(forbidden),
            "fix.conflict-modal-wiring (iter 193): \
             preview_mod_install_conflicts must not call `{forbidden}` \
             — preview is read-only; write-side-effects would \
             surprise users. Body:\n{body}"
        );
    }
}

/// Iter 193: `preview_mod_install_conflicts` must short-circuit for
/// non-GPK entries (e.g. external-app mods). Running the mapper-read
/// path for an external-app catalog entry would either error or
/// return a vacuous empty list after unnecessary I/O.
#[test]
fn preview_command_short_circuits_for_non_gpk_entries() {
    let src = fs::read_to_string(COMMANDS_MODS_RS).expect("commands/mods.rs must exist");
    let fn_pos = src
        .find("pub async fn preview_mod_install_conflicts")
        .expect("preview_mod_install_conflicts must exist");
    let window = &src[fn_pos..fn_pos.saturating_add(600)];
    assert!(
        window.contains("matches!(entry.kind, ModKind::Gpk)"),
        "fix.conflict-modal-wiring (iter 193): \
         preview_mod_install_conflicts must check \
         `matches!(entry.kind, ModKind::Gpk)` early and return \
         `Ok(Vec::new())` for external-app kinds. Without the check, \
         the mapper-read path runs for every kind of mod."
    );
    // Early return Ok(Vec::new()) must appear before the fs::read
    // that follows in the body.
    let empty_idx = window
        .find("return Ok(Vec::new())")
        .expect("early return for non-GPK must exist");
    let fs_read_idx = window
        .find("std::fs::read(&source_gpk)")
        .unwrap_or(window.len());
    assert!(
        empty_idx < fs_read_idx,
        "fix.conflict-modal-wiring (iter 193): \
         the non-GPK early-return must come BEFORE `std::fs::read`. \
         Running the fs::read for a non-GPK entry is wasted I/O."
    );
}

/// Iter 193: `detect_conflicts` must iterate over `incoming.packages`
/// — the incoming mod's declared packages are what the function
/// checks against the current mapper. Iterating `current_map` (or
/// vanilla_map) instead would invert the semantics and flag the
/// wrong set.
#[test]
fn detect_conflicts_iterates_over_incoming_packages() {
    let src = tmm_src();
    let body = detect_conflicts_body(&src);
    assert!(
        body.contains("for pkg in &incoming.packages"),
        "fix.conflict-modal-wiring (iter 193): detect_conflicts must \
         iterate `for pkg in &incoming.packages` — iterating \
         current_map or vanilla_map inverts the semantics and flags \
         the wrong set.\nBody:\n{body}"
    );
    // And must not loop over the alternative maps (negative pin).
    assert!(
        !body.contains("for pkg in current_map")
            && !body.contains("for pkg in vanilla_map"),
        "fix.conflict-modal-wiring (iter 193): detect_conflicts must \
         NOT iterate `current_map` / `vanilla_map` — those maps are \
         lookup targets, not iteration sources. Body:\n{body}"
    );
}

/// Iter 193: `ModConflict` must carry EXACTLY three public String
/// fields. A fourth field would add a UI-render responsibility the
/// frontend modal isn't wired to handle; a field-count mismatch
/// would drift the IPC schema silently (Tauri still serialises, but
/// the frontend destructure misses or aliases keys).
#[test]
fn mod_conflict_has_exactly_three_public_fields() {
    let src = tmm_src();
    let decl_pos = src
        .find("pub struct ModConflict")
        .expect("ModConflict struct must exist");
    let rest = &src[decl_pos..];
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(500));
    let body = &rest[..end];
    let pub_count = body.matches("    pub ").count();
    assert_eq!(
        pub_count, 3,
        "fix.conflict-modal-wiring (iter 193): ModConflict must \
         carry exactly 3 `pub` fields (composite_name, object_path, \
         previous_filename). Found {pub_count}. Extras require a \
         frontend modal update; removals break the existing UI.\n\
         Body:\n{body}"
    );
}
