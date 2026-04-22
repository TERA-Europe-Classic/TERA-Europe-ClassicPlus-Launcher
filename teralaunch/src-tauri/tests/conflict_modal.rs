//! fix.conflict-modal-wiring — wiring guard for
//! `commands::mods::preview_mod_install_conflicts`.
//!
//! Pure predicate tests live inline in
//! `src/services/mods/gpk.rs::tests` (six detect_conflicts cases
//! covering vanilla / self-reinstall / mixed / multi-slot / missing).
//! This file only guards the Tauri command layer + the bundle helper
//! `preview_conflicts_from_bytes` that glues the fs/decrypt/parse path
//! together for the command body.

use std::fs;

/// The Tauri command must be annotated `#[tauri::command]` with the
/// attribute directly adjacent to the fn decl, delegate to
/// `gpk::preview_conflicts_from_bytes`, resolve the game root via the
/// shared helper, and return `Vec<ModConflict>`.
#[test]
fn preview_mod_install_conflicts_is_a_tauri_command_and_delegates_to_gpk() {
    let mods_rs = fs::read_to_string("src/commands/mods.rs").expect("commands/mods.rs must exist");

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
        body_window.contains("gpk::preview_conflicts_from_bytes"),
        "preview_mod_install_conflicts must delegate to \
         gpk::preview_conflicts_from_bytes (bundle helper)"
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
    let gpk_rs =
        fs::read_to_string("src/services/mods/gpk.rs").expect("services/mods/gpk.rs must exist");

    let decl_pos = gpk_rs
        .find("pub struct ModConflict")
        .expect("gpk.rs must still define ModConflict");

    // Look back 400 chars for the derive attribute (doc comment +
    // derive line together fit comfortably in that window).
    let pre = &gpk_rs[decl_pos.saturating_sub(400)..decl_pos];
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
    let gpk_rs =
        fs::read_to_string("src/services/mods/gpk.rs").expect("services/mods/gpk.rs must exist");

    let decl_pos = gpk_rs
        .find("pub fn preview_conflicts_from_bytes")
        .expect("gpk.rs must define preview_conflicts_from_bytes");

    // The "return Ok(Vec::new())" on missing backup must appear in the
    // first ~500 chars of the body — i.e. as an early-return before any
    // potentially-erroring I/O. Guards against a refactor that moves
    // the check after a fallible `fs::read(&backup)` call.
    let body_window = &gpk_rs[decl_pos..decl_pos.saturating_add(500)];
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

const GPK_RS: &str = "src/services/mods/gpk.rs";

fn gpk_src() -> String {
    fs::read_to_string(GPK_RS).unwrap_or_else(|e| panic!("{GPK_RS} must be readable: {e}"))
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
    let src = gpk_src().replace("\r\n", "\n");
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
    let src = gpk_src();
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
    let src = gpk_src();
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
    let src = gpk_src();
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
    let src = gpk_src();
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
    let src = gpk_src();
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
        !body.contains("for pkg in current_map") && !body.contains("for pkg in vanilla_map"),
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
    let src = gpk_src();
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

// --------------------------------------------------------------------
// Iter 235 structural pins — path-constant canonicalisation, return-
// type vs Option, field-order pin for IPC stability, empty-input
// short-circuit, command-name literal pin.
//
// Iter-193 covered wiring + helper semantics + struct shape. These
// five extend to the meta-guard + IPC-stability surface a confident
// refactor could still miss: a path-constant drift (header-inspection
// panics on file-not-found), a return type swapped to Option (UI
// render-empty becomes render-nothing), a field-order swap (serde
// positional tuples silently drift), an empty-input case that scans
// anyway (waste of CPU + potentially spurious entries), and a
// command-name rename that breaks the IPC contract without CI trip.
// --------------------------------------------------------------------

/// Iter 235: `GPK_RS`, `COMMANDS_MODS_RS`, `GUARD_SOURCE` constants
/// must stay canonical. Every source-inspection pin in this guard
/// reads through one of these; drift renders header/body checks
/// inert with a misleading `file not found` panic.
#[test]
fn guard_path_constants_are_canonical() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    assert!(
        body.contains(r#"const GPK_RS: &str = "src/services/mods/gpk.rs";"#),
        "fix.conflict-modal-wiring (iter 235): {GUARD_SOURCE} must \
         keep `const GPK_RS: &str = \"src/services/mods/gpk.rs\";` \
         verbatim. A rename leaves every gpk_src() with file-not-found."
    );
    assert!(
        body.contains(r#"const COMMANDS_MODS_RS: &str = "src/commands/mods.rs";"#),
        "fix.conflict-modal-wiring (iter 235): {GUARD_SOURCE} must \
         keep `const COMMANDS_MODS_RS: &str = \"src/commands/mods.rs\";` \
         verbatim."
    );
    assert!(
        body.contains(r#"const GUARD_SOURCE: &str = "tests/conflict_modal.rs";"#),
        "fix.conflict-modal-wiring (iter 235): {GUARD_SOURCE} must \
         keep `const GUARD_SOURCE: &str = \"tests/conflict_modal.rs\";` \
         verbatim."
    );
}

/// Iter 235: `detect_conflicts` and the preview command must return
/// `Vec<ModConflict>`, not `Option<Vec<ModConflict>>` or
/// `Result<Vec<ModConflict>, _>` for the no-conflict case. An empty
/// Vec is the canonical "nothing to warn about" signal — the UI
/// renders empty, and the command stays infallible for its best-
/// effort contract (see `preview_conflicts_from_bytes_is_best_effort
/// _on_missing_backup`).
#[test]
fn detect_conflicts_return_type_is_vec_not_option() {
    let src = gpk_src();
    let body = detect_conflicts_body(&src);
    // Find the return type in the signature line.
    let sig_end = body
        .find(") -> ")
        .expect("detect_conflicts signature must have `) -> `");
    let after_arrow = &body[sig_end + ") -> ".len()..];
    let ret_line: String = after_arrow
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('{')
        .trim()
        .to_string();
    assert!(
        ret_line.starts_with("Vec<ModConflict>"),
        "fix.conflict-modal-wiring (iter 235): detect_conflicts must \
         return `Vec<ModConflict>`. Got `{ret_line}`. An Option<Vec<>> \
         wrapper makes the UI destructure a 2-state rather than 1-\
         state signal (empty vs None adds no information); a Result \
         breaks the best-effort contract."
    );
}

/// Iter 235: `ModConflict`'s first field must be `composite_name`.
/// The frontend modal renders rows using positional / key-name
/// access; a field swap to `(object_path, composite_name, …)` would
/// leave serde JSON (named fields) unchanged but any consumer using
/// positional destructuring (tuple style) would bind to the wrong
/// slot. Pinning the order guards against both cases.
#[test]
fn mod_conflict_first_field_is_composite_name() {
    let src = gpk_src();
    let decl_pos = src
        .find("pub struct ModConflict")
        .expect("ModConflict struct must exist");
    let rest = &src[decl_pos..];
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(500));
    let body = &rest[..end];
    // Find the first `pub <field>: <type>` line — must contain `: ` to
    // distinguish a field decl from the struct decl line itself.
    let first_field_line = body
        .lines()
        .find(|l| {
            let t = l.trim_start();
            t.starts_with("pub ") && t.contains(": ")
        })
        .unwrap_or("");
    assert!(
        first_field_line.contains("composite_name"),
        "fix.conflict-modal-wiring (iter 235): ModConflict's first \
         public field must be `composite_name`. Got: `{first_field_line}`. \
         The frontend modal renders composite_name as the row's title; \
         reordering shifts the title to another field without a type \
         check firing."
    );
}

/// Iter 235: `detect_conflicts` must return an empty Vec when the
/// incoming ModFile has no packages. No map scan should happen —
/// the iter-193 "iterates incoming.packages" pin already asserts
/// the loop target, but doesn't prove the fast path. A refactor
/// that always scanned current_map (e.g. for diagnostic purposes)
/// could leak spurious entries when called from the preview
/// command.
#[test]
fn detect_conflicts_short_circuits_on_empty_incoming() {
    let src = gpk_src();
    let body = detect_conflicts_body(&src);
    // The fn must iterate `incoming.packages` (covered by iter-193).
    // An empty packages iterator trivially produces an empty Vec.
    // We pin this here by asserting the loop body is the ONLY place
    // that pushes into the result vec — no early push from a
    // diagnostic pass over current_map.
    assert!(
        body.contains("for pkg in &incoming.packages"),
        "fix.conflict-modal-wiring (iter 235): detect_conflicts must \
         iterate `for pkg in &incoming.packages` (iter-193 invariant). \
         Empty input then trivially yields empty output."
    );
    // Count push sites: there must be exactly ONE (inside the loop).
    let push_count = body.matches(".push(").count();
    assert_eq!(
        push_count, 1,
        "fix.conflict-modal-wiring (iter 235): detect_conflicts must \
         contain exactly one `.push(` call site — inside the \
         incoming-packages loop. Found {push_count}. A second push \
         site implies a diagnostic / always-scan pass that could \
         leak spurious ModConflict entries on empty input.\nBody:\n{body}"
    );
}

/// Iter 235: the Tauri command name MUST be exactly
/// `preview_mod_install_conflicts`. The frontend invokes this via
/// `invoke('preview_mod_install_conflicts', ...)`; a rename without
/// a coordinated frontend update silently breaks the conflict modal
/// (invoke rejects unknown commands with a runtime error, but the
/// mods page's try/catch swallows it into a toast).
#[test]
fn preview_command_name_is_pinned_verbatim() {
    let commands_body =
        fs::read_to_string(COMMANDS_MODS_RS).expect("commands/mods.rs must be readable");
    assert!(
        commands_body.contains("pub async fn preview_mod_install_conflicts")
            || commands_body.contains("pub fn preview_mod_install_conflicts"),
        "fix.conflict-modal-wiring (iter 235): commands/mods.rs must \
         define `preview_mod_install_conflicts`. A rename breaks the \
         frontend's `invoke('preview_mod_install_conflicts', ...)` \
         call — the mods page's try/catch turns the failure into a \
         generic toast, masking the rename."
    );
    // Negative pin: no alternate spelling has crept in.
    for wrong in [
        "preview_install_conflicts",
        "preview_conflicts",
        "preview_mod_conflicts",
        "check_mod_conflicts",
    ] {
        assert!(
            !commands_body.contains(&format!("fn {wrong}(")),
            "fix.conflict-modal-wiring (iter 235): commands/mods.rs \
             must NOT define an alternate spelling `{wrong}` — the \
             canonical name is `preview_mod_install_conflicts` and \
             any drift breaks the frontend invoke."
        );
    }
}

// --------------------------------------------------------------------
// Iter 271 structural pins — guard/commands/tmm bounds + slot cite +
// invoke-handler registration.
// --------------------------------------------------------------------

/// Iter 271: guard source byte bounds.
#[test]
fn guard_source_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 5000;
    const MAX_BYTES: usize = 80_000;
    let bytes = std::fs::metadata(GUARD_SOURCE)
        .expect("guard must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "fix.conflict-modal-wiring (iter 271): guard is {bytes} \
         bytes; expected [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

/// Iter 271: commands/mods.rs byte bounds.
#[test]
fn commands_mods_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 3000;
    const MAX_BYTES: usize = 200_000;
    let bytes = std::fs::metadata(COMMANDS_MODS_RS)
        .expect("commands/mods.rs must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "fix.conflict-modal-wiring (iter 271): {COMMANDS_MODS_RS} is \
         {bytes} bytes; expected [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

/// Iter 271: gpk.rs byte bounds (pure predicate lives there).
#[test]
fn tmm_rs_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 5000;
    const MAX_BYTES: usize = 200_000;
    let bytes = std::fs::metadata(GPK_RS).expect("gpk.rs must exist").len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "fix.conflict-modal-wiring (iter 271): {GPK_RS} is {bytes} \
         bytes; expected [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

/// Iter 271: guard header must cite fix-plan slot.
#[test]
fn guard_source_cites_fix_conflict_modal_wiring_slot() {
    let body = std::fs::read_to_string(GUARD_SOURCE).expect("guard must exist");
    let header = &body[..body.len().min(500)];
    assert!(
        header.contains("fix.conflict-modal-wiring"),
        "fix.conflict-modal-wiring (iter 271): guard header must \
         cite the slot verbatim.\nHeader:\n{header}"
    );
}

/// Iter 271: `preview_mod_install_conflicts` must be registered in
/// main.rs invoke-handler. Without registration, the frontend errors
/// with `command not found`.
#[test]
fn preview_mod_install_conflicts_is_registered_in_main() {
    let main = std::fs::read_to_string("src/main.rs").expect("main.rs must exist");
    assert!(
        main.contains("preview_mod_install_conflicts"),
        "fix.conflict-modal-wiring (iter 271): src/main.rs must \
         register `preview_mod_install_conflicts` in \
         generate_handler! / invoke_handler. Without registration, \
         the frontend invoke() call errors with 'command not found'."
    );
}
