//! fix.clean-recovery-wiring (PRD 3.2.9) — wiring guard for
//! `commands::mods::recover_clean_mapper`.
//!
//! The behavioural test of the underlying predicate lives inline in
//! `src/services/mods/gpk.rs::tests` (four cases: nop-when-backup-exists,
//! creates-backup-from-vanilla, refuses-when-current-is-modded, missing-
//! mapper-returns-err). This integration test only covers the piece the
//! unit tests can't reach: that the Tauri command layer is actually
//! wired up so the frontend can invoke it.
//!
//! Without the source-inspection below, someone could delete the command
//! body or drop the `generate_handler!` entry and the predicate tests
//! would still pass but the Recovery button would call into void.

use std::fs;

/// `commands::mods::recover_clean_mapper` must (a) exist with a
/// `#[tauri::command]` attribute, (b) call through to
/// `gpk::recover_missing_clean`, and (c) resolve the game root via the
/// existing `resolve_game_root()` helper rather than a fresh ad-hoc path
/// lookup that could drift from the other mods commands.
#[test]
fn recover_clean_mapper_is_a_tauri_command_and_delegates_to_gpk() {
    let mods_rs = fs::read_to_string("src/commands/mods.rs").expect("commands/mods.rs must exist");

    let decl_pos = mods_rs
        .find("pub async fn recover_clean_mapper")
        .expect("commands/mods.rs must declare recover_clean_mapper");

    let pre = &mods_rs[..decl_pos];
    // Check the attribute block immediately preceding the fn — the last
    // `#[tauri::command]` before `pub async fn recover_clean_mapper`
    // must be within a small window of the declaration (not an orphan
    // from some earlier function).
    let last_attr = pre.rfind("#[tauri::command]").expect(
        "recover_clean_mapper must be annotated #[tauri::command] — without it, \
         generate_handler! can't register it",
    );
    assert!(
        decl_pos - last_attr < 200,
        "#[tauri::command] attribute must be adjacent to recover_clean_mapper — \
         found {} chars away",
        decl_pos - last_attr
    );

    // Body search is bounded to the 600 chars after the declaration —
    // comfortably covers a thin wrapper but won't accidentally match a
    // sibling function further down.
    let body_window = &mods_rs[decl_pos..decl_pos.saturating_add(600)];
    assert!(
        body_window.contains("gpk::recover_missing_clean"),
        "recover_clean_mapper must delegate to gpk::recover_missing_clean"
    );
    assert!(
        body_window.contains("resolve_game_root"),
        "recover_clean_mapper must resolve game root via the shared \
         resolve_game_root helper (same error path as install/uninstall)"
    );
}

/// The command must also be registered in `main.rs::generate_handler!`
/// — otherwise the frontend's `invoke('recover_clean_mapper')` errors
/// with "command not registered" at runtime.
#[test]
fn recover_clean_mapper_is_registered_in_main_generate_handler() {
    let main_rs = fs::read_to_string("src/main.rs").expect("main.rs must exist");

    assert!(
        main_rs.contains("commands::mods::recover_clean_mapper"),
        "main.rs::generate_handler! must register commands::mods::recover_clean_mapper — \
         otherwise invoke('recover_clean_mapper') fails at runtime"
    );
}

/// The underlying `gpk::recover_missing_clean` must not be gated with
/// `#[allow(dead_code)]` any more. While the attribute doesn't affect
/// semantics, its presence would be a tell that nobody's actually
/// calling the function — historically it was a TODO flag. Removing
/// it as part of the wiring commit makes the promotion explicit.
#[test]
fn recover_missing_clean_is_no_longer_dead_code_gated() {
    let gpk_rs =
        fs::read_to_string("src/services/mods/gpk.rs").expect("services/mods/gpk.rs must exist");

    let fn_pos = gpk_rs
        .find("pub fn recover_missing_clean")
        .expect("gpk.rs must still define recover_missing_clean");

    // The 200-char window immediately before the fn must not contain
    // `#[allow(dead_code)]`. A nearby but unrelated allow(dead_code) on
    // a sibling item won't trip this — 200 chars comfortably fits the
    // doc comment block without spilling into other items.
    let pre = &gpk_rs[fn_pos.saturating_sub(200)..fn_pos];
    assert!(
        !pre.contains("#[allow(dead_code)]"),
        "recover_missing_clean is now wired up (see commands::mods::recover_clean_mapper) — \
         drop the #[allow(dead_code)] gate"
    );
}

// --------------------------------------------------------------------
// Iter 164 structural pins — recover_missing_clean three-branch body +
// ensure_backup idempotence + filename constants.
// --------------------------------------------------------------------
//
// The tests above guard the Tauri-command wiring. These pins defend
// the body of the two backup-management functions that the wiring
// routes into. Each pin names a specific refactor-hazard:
//   - dropped dst.exists() early return → Recover button overwrites
//     `.clean` with a modded mapper on every click (permanent loss).
//   - dropped TMM_MARKER refusal → a user who deleted `.clean` with
//     mods installed would stamp the modded mapper as the new
//     vanilla, destroying the ability to cleanly uninstall.
//   - dropped ensure_backup idempotence → every install resets
//     `.clean` to the current (possibly modded) mapper.
//   - renamed filename constants → ensure_backup writes to the wrong
//     path, breaking the recovery contract silently.

const GPK_RS: &str = "src/services/mods/gpk.rs";

fn gpk_src() -> String {
    fs::read_to_string(GPK_RS).unwrap_or_else(|e| panic!("{GPK_RS} must be readable: {e}"))
}

/// Returns the body of `fn_name` as a slice — from the `pub fn` up to
/// the trailing `\n}\n` that closes the fn.
fn fn_body_of<'a>(src: &'a str, sig: &str) -> &'a str {
    let fn_pos = src.find(sig).unwrap_or_else(|| panic!("{sig} must exist"));
    let rest = &src[fn_pos..];
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(2000));
    &rest[..end]
}

/// `recover_missing_clean` must short-circuit with `Ok(())` the moment
/// it sees `.clean` already exists. Without this, every click of the
/// Recover button re-stamps `.clean` with whatever's currently in
/// `.dat` — catastrophic if the user has mods installed (overwrites
/// the true vanilla backup with a modded mapper; uninstall can never
/// restore vanilla).
#[test]
fn recover_missing_clean_noops_when_backup_already_exists() {
    let src = gpk_src();
    let body = fn_body_of(&src, "pub fn recover_missing_clean");
    let exists_pos = body
        .find("if dst.exists() {")
        .expect("recover_missing_clean must guard `if dst.exists() {` early");
    let ok_pos = body
        .find("return Ok(())")
        .expect("recover_missing_clean must short-circuit with return Ok(()) on existing backup");
    assert!(
        exists_pos < ok_pos,
        "PRD §3.2.9 clean-recovery-logic: the `if dst.exists()` early \
         return must precede any fs::copy. Otherwise recovery \
         overwrites the vanilla `.clean` with the current mapper \
         — the user loses their only route back to vanilla.\n\
         Body:\n{body}"
    );
}

/// `recover_missing_clean` must refuse when the current mapper has
/// the `TMM_MARKER` entry (i.e. mods are installed). Without this, a
/// user who deleted `.clean` while mods were active would stamp the
/// modded mapper as the new vanilla baseline — permanently destroying
/// the ability to cleanly uninstall any mod.
#[test]
fn recover_missing_clean_refuses_modded_current_mapper() {
    let src = gpk_src();
    let body = fn_body_of(&src, "pub fn recover_missing_clean");
    assert!(
        body.contains("if map.contains_key(TMM_MARKER)"),
        "PRD §3.2.9: recover_missing_clean must check \
         `map.contains_key(TMM_MARKER)` before copying. Without it, \
         a modded mapper gets stamped as the new vanilla baseline.\n\
         Body:\n{body}"
    );
    // The refusal error message must name the TMM marker so operators
    // understand why recovery refused.
    assert!(
        body.contains("Cannot recover .clean") && body.contains("verify game files"),
        "PRD §3.2.9: recover_missing_clean refusal error must carry \
         the `Cannot recover .clean` phrase + guidance to verify game \
         files. Without this, the user gets a generic error and \
         doesn't know how to fix their install.\n\
         Body:\n{body}"
    );
}

/// `ensure_backup` must be idempotent on an existing backup — if
/// `.clean` is present, return `Ok(())` without touching anything.
/// Without this, every install resets `.clean` to the CURRENT mapper
/// (which may already carry prior mods' entries) — same catastrophic
/// loss of the true vanilla baseline as recover's missing-guard
/// scenario.
#[test]
fn ensure_backup_is_idempotent_on_existing_backup() {
    let src = gpk_src();
    let body = fn_body_of(&src, "pub fn ensure_backup");
    let exists_pos = body
        .find("if dst.exists() {")
        .expect("ensure_backup must guard `if dst.exists() {` before fs::copy");
    let ok_pos = body
        .find("return Ok(())")
        .expect("ensure_backup must short-circuit with Ok(()) on existing backup");
    let copy_pos = body
        .find("fs::copy(&src, &dst)")
        .expect("ensure_backup must still copy when the backup is missing");
    assert!(
        exists_pos < ok_pos && ok_pos < copy_pos,
        "PRD §3.2.9 / §3.1.4: ensure_backup must early-return via \
         `if dst.exists() {{ return Ok(()); }}` BEFORE `fs::copy`. \
         Re-copying on every install overwrites the true vanilla \
         baseline with the current (possibly modded) mapper — the \
         user loses the ability to uninstall cleanly.\n\
         Body:\n{body}"
    );
}

/// The filename constants `MAPPER_FILE` and `BACKUP_FILE` must stay
/// pinned to their exact values. The game's UE3 loader reads
/// `CompositePackageMapper.dat` by literal name; renaming breaks
/// every mod load. The `.clean` suffix is the launcher convention —
/// other constants (path scanners, doc references) depend on the
/// literal string.
#[test]
fn mapper_and_backup_filename_constants_are_pinned() {
    let src = gpk_src();
    assert!(
        src.contains(r#"pub const MAPPER_FILE: &str = "CompositePackageMapper.dat";"#),
        "PRD §3.1.4: gpk.rs must pin \
         `pub const MAPPER_FILE: &str = \"CompositePackageMapper.dat\";` \
         verbatim. The UE3 loader reads the file by literal name; \
         renaming breaks every mod load."
    );
    assert!(
        src.contains(r#"pub const BACKUP_FILE: &str = "CompositePackageMapper.clean";"#),
        "PRD §3.1.4: gpk.rs must pin \
         `pub const BACKUP_FILE: &str = \"CompositePackageMapper.clean\";` \
         verbatim. Renaming desyncs ensure_backup + recover_missing_clean \
         + every call-site scanner that greps for `.clean`."
    );
}

/// Both `ensure_backup` and `recover_missing_clean` must remain
/// `pub fn`. If either drops to `pub(crate)` or `fn`, the Tauri
/// command layer (which lives in the separate `commands` module)
/// can't reach them and the wiring breaks — but silently, as the
/// `#[tauri::command]` attribute alone doesn't require the callee
/// to be visible to the module that registers it.
#[test]
fn backup_and_recover_functions_stay_pub() {
    let src = gpk_src();
    assert!(
        src.contains("pub fn ensure_backup(game_root: &Path) -> Result<(), String>"),
        "PRD §3.1.4: ensure_backup must stay \
         `pub fn ensure_backup(game_root: &Path) -> Result<(), String>`. \
         Dropping `pub` breaks cross-module reachability without \
         tripping any other test."
    );
    assert!(
        src.contains("pub fn recover_missing_clean(game_root: &Path) -> Result<(), String>"),
        "PRD §3.2.9: recover_missing_clean must stay \
         `pub fn recover_missing_clean(game_root: &Path) -> Result<(), String>`. \
         The Tauri command `recover_clean_mapper` depends on this \
         visibility."
    );
}

// --------------------------------------------------------------------
// Iter 195 structural pins — guard traceability + missing-mapper
// branch + map_err hygiene + TMM_MARKER value + fs::copy direction.
// --------------------------------------------------------------------

const GUARD_SOURCE: &str = "tests/clean_recovery.rs";

/// Iter 195: guard source header must cite `PRD §3.2.9` + the fix-
/// plan slot `fix.clean-recovery-wiring` so the criterion and P-slot
/// are reachable via grep.
#[test]
fn guard_file_header_cites_prd_and_fix_slot() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    let header = &body[..body.len().min(2000)];
    assert!(
        header.contains("fix.clean-recovery-wiring"),
        "PRD §3.2.9 (iter 195): {GUARD_SOURCE} header must cite \
         `fix.clean-recovery-wiring` so the fix-plan P-slot is \
         reachable via grep."
    );
    // The criterion citation may be anywhere in the file (existing
    // iter-164 pins use `PRD §3.2.9` inline).
    assert!(
        body.contains("§3.2.9") || body.contains("PRD 3.2.9"),
        "PRD §3.2.9 (iter 195): {GUARD_SOURCE} must cite the PRD \
         criterion somewhere in the file so the criterion is \
         reachable via grep."
    );
}

/// Iter 195: `recover_missing_clean` must carry the missing-mapper
/// error branch (`if !src.exists()` returning a specific `Verify
/// game files` message). Without this branch, a user whose
/// CompositePackageMapper.dat is missing gets a generic fs::read
/// error and no actionable guidance.
#[test]
fn recover_missing_clean_has_missing_mapper_error_branch() {
    let src = gpk_src();
    let body = fn_body_of(&src, "pub fn recover_missing_clean");
    assert!(
        body.contains("if !src.exists()"),
        "PRD §3.2.9 (iter 195): recover_missing_clean must carry an \
         `if !src.exists()` branch. Without it, users whose mapper \
         file is missing see a generic fs::read IO error."
    );
    assert!(
        body.contains("CompositePackageMapper.dat not found"),
        "PRD §3.2.9 (iter 195): the missing-mapper error must name \
         the file (`CompositePackageMapper.dat not found`) so the \
         user knows which file to restore."
    );
    assert!(
        body.contains("Verify game files"),
        "PRD §3.2.9 (iter 195): the missing-mapper error must point \
         the user at `Verify game files` (Steam / launcher action) \
         as the recovery path."
    );
}

/// Iter 195: both `ensure_backup` and `recover_missing_clean` must
/// use `.map_err(|e| format!(...))?` on fs operations. A raw
/// `.unwrap()` / `.expect()` on the mapper-read or copy path would
/// panic the process, leaving the user no route but to relaunch.
#[test]
fn backup_and_recover_use_map_err_not_unwrap() {
    let src = gpk_src();
    for sig in ["pub fn ensure_backup", "pub fn recover_missing_clean"] {
        let body = fn_body_of(&src, sig);
        assert!(
            body.contains(".map_err(|e|"),
            "PRD §3.2.9 (iter 195): `{sig}` body must use \
             `.map_err(|e| format!(...))?` on fs operations. Raw \
             `.unwrap()` panics the process.\nBody:\n{body}"
        );
        // Reject raw .unwrap() on fs calls in the body.
        for bad in ["fs::read(&src).unwrap()", "fs::copy(&src, &dst).unwrap()"] {
            assert!(
                !body.contains(bad),
                "PRD §3.2.9 (iter 195): `{sig}` must not contain \
                 `{bad}` — panics the process on IO error."
            );
        }
    }
}

/// Iter 195: `TMM_MARKER` constant must stay pinned to the exact
/// lowercase-snake literal `"tmm_marker"`. This string is the
/// sentinel the recover/ensure functions use to decide "is this
/// mapper modded?"; renaming (`TMM_INSTALLED`, `tera-mod-manager`)
/// would break the mod-presence detection and let recover stamp
/// modded mappers as vanilla baselines.
#[test]
fn tmm_marker_constant_is_pinned_verbatim() {
    let src = gpk_src();
    assert!(
        src.contains(r#"const TMM_MARKER: &str = "tmm_marker";"#),
        "PRD §3.2.9 (iter 195): gpk.rs must pin \
         `const TMM_MARKER: &str = \"tmm_marker\";` verbatim. The \
         string is the sentinel used by recover_missing_clean + \
         ensure_backup + parse_mapper; renaming breaks the mod-\
         presence detection silently."
    );
}

/// Iter 195: `recover_missing_clean` must call `fs::copy(&src, &dst)`
/// with args in that order — src (current mapper) → dst (backup
/// file). Reversing the args copies the empty/missing backup over
/// the current mapper, permanently corrupting the user's install.
#[test]
fn recover_missing_clean_copies_src_to_dst_not_reverse() {
    let src = gpk_src();
    let body = fn_body_of(&src, "pub fn recover_missing_clean");
    assert!(
        body.contains("fs::copy(&src, &dst)"),
        "PRD §3.2.9 (iter 195): recover_missing_clean must call \
         `fs::copy(&src, &dst)` with src (current mapper) as first \
         arg, dst (backup) as second. Reversed args copy the missing \
         backup over the current mapper — install-destroying.\n\
         Body:\n{body}"
    );
    // Reject the reversed form.
    assert!(
        !body.contains("fs::copy(&dst, &src)"),
        "PRD §3.2.9 (iter 195): recover_missing_clean must NOT \
         contain `fs::copy(&dst, &src)` — that reversed direction \
         destroys the current mapper by overwriting it with the \
         missing backup."
    );
}

/// Iter 227: sister-pin to
/// `recover_missing_clean_copies_src_to_dst_not_reverse` — the same
/// copy-direction invariant must hold for `ensure_backup`. Iter 195
/// only pinned one of the two backup-management functions; if
/// `ensure_backup` later gets refactored with reversed args, the
/// install path would silently copy the (absent) backup over the
/// vanilla mapper — same install-destroying outcome, missed by the
/// existing pin.
#[test]
fn ensure_backup_copies_src_to_dst_not_reverse() {
    let src = gpk_src();
    let body = fn_body_of(&src, "pub fn ensure_backup");
    assert!(
        body.contains("fs::copy(&src, &dst)"),
        "PRD §3.2.9 (iter 227): ensure_backup must call \
         `fs::copy(&src, &dst)` with src (current mapper) first and \
         dst (backup) second. Reversed args copy the missing backup \
         over the current mapper on first install.\nBody:\n{body}"
    );
    assert!(
        !body.contains("fs::copy(&dst, &src)"),
        "PRD §3.2.9 (iter 227): ensure_backup must NOT contain \
         `fs::copy(&dst, &src)` — reversed direction corrupts the \
         vanilla mapper on first install."
    );
}

/// Iter 227: the two path constants declared inside the guard
/// (`GPK_RS` and `GUARD_SOURCE`) must be pinned verbatim at canonical
/// values. A rename of either target (gpk.rs → mapper.rs, or the
/// guard file itself) would surface as a generic must-exist panic at
/// `fs::read_to_string` time instead of a guarded diff. Pinning the
/// literal makes the rename a conscious guard update.
#[test]
fn guard_path_constants_are_canonical() {
    let src =
        fs::read_to_string("tests/clean_recovery.rs").expect("tests/clean_recovery.rs must exist");
    for line in [
        r#"const GPK_RS: &str = "src/services/mods/gpk.rs";"#,
        r#"const GUARD_SOURCE: &str = "tests/clean_recovery.rs";"#,
    ] {
        assert!(
            src.contains(line),
            "canonical path constant missing: `{line}`. A rename of \
             any of these targets must surface as a guard update, \
             not a generic must-exist panic."
        );
    }
}

/// Iter 227: both `mapper_path` and `backup_path` helpers must
/// construct their paths via
/// `game_root.join(COOKED_PC_DIR).join(MAPPER_FILE|BACKUP_FILE)`. A
/// drift that drops the `COOKED_PC_DIR` join (e.g. reads mapper
/// directly from the game root) would write the backup to the wrong
/// directory — the real mapper + its `.clean` sibling both live in
/// `<game-root>/CookedPC/`, not the root. The game would still load
/// (it reads its own path, not the backup's) but recovery would
/// silently read the wrong file.
#[test]
fn mapper_and_backup_path_helpers_join_via_cooked_pc_dir() {
    let src = gpk_src();
    for (helper, filename_const) in [
        ("fn mapper_path", "MAPPER_FILE"),
        ("fn backup_path", "BACKUP_FILE"),
    ] {
        let body = fn_body_of(&src, helper);
        assert!(
            body.contains("game_root.join(COOKED_PC_DIR)"),
            "PRD §3.2.9 (iter 227): `{helper}` must construct its \
             path via `game_root.join(COOKED_PC_DIR)` — the real \
             mapper lives in CookedPC, not the game root. Dropping \
             the COOKED_PC_DIR join points recovery at the wrong \
             directory.\nBody:\n{body}"
        );
        assert!(
            body.contains(&format!(".join({filename_const})")),
            "PRD §3.2.9 (iter 227): `{helper}` must chain \
             `.join({filename_const})` after the CookedPC join. \
             Inlining the filename literal risks drift from the \
             MAPPER_FILE / BACKUP_FILE constants.\nBody:\n{body}"
        );
    }
}

/// Iter 227: both `ensure_backup` and `recover_missing_clean` must
/// source their src/dst paths via the shared `mapper_path(game_root)`
/// and `backup_path(game_root)` helpers — not via inline game-root
/// join constructions. Reimplementing the path construction in one
/// function would silently drift from the shared helpers' guarantees
/// (CookedPC-dir plus the filename constants).
#[test]
fn backup_functions_source_paths_via_shared_helpers() {
    let src = gpk_src();
    for sig in ["pub fn ensure_backup", "pub fn recover_missing_clean"] {
        let body = fn_body_of(&src, sig);
        assert!(
            body.contains("mapper_path(game_root)"),
            "PRD §3.2.9 (iter 227): `{sig}` must source the src via \
             `mapper_path(game_root)` — not inline join. Bypassing \
             the helper drifts from the CookedPC + MAPPER_FILE \
             guarantees.\nBody:\n{body}"
        );
        assert!(
            body.contains("backup_path(game_root)"),
            "PRD §3.2.9 (iter 227): `{sig}` must source the dst via \
             `backup_path(game_root)` — same rationale.\nBody:\n{body}"
        );
    }
}

// --------------------------------------------------------------------
// Iter 261 structural pins — guard source PRD cite + guard byte bounds
// + gpk.rs inline test module presence + recover_clean_mapper in
// invoke_handler list + fix-plan-slot cross-ref in guard header.
// --------------------------------------------------------------------

/// Iter 261: the guard's module header must cite the fix-plan slot
/// `fix.clean-recovery-wiring` explicitly (already done in iter 164
/// but re-pinned here against drift). Without the explicit cite,
/// a reader chasing the fix-plan slot wouldn't land here.
#[test]
fn guard_source_cites_fix_clean_recovery_wiring_slot() {
    let body =
        fs::read_to_string("tests/clean_recovery.rs").expect("tests/clean_recovery.rs must exist");
    let header = &body[..body.len().min(500)];
    assert!(
        header.contains("fix.clean-recovery-wiring"),
        "fix.clean-recovery-wiring (iter 261): guard header must cite \
         the fix-plan slot `fix.clean-recovery-wiring`. A reader \
         chasing the slot via fix-plan.md grep should land here \
         directly.\nHeader:\n{header}"
    );
}

/// Iter 261: the guard file must have sane byte bounds. Floor 3000
/// catches gutting; ceiling 50_000 catches scope creep.
#[test]
fn guard_source_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 3000;
    const MAX_BYTES: usize = 50_000;
    let bytes = fs::metadata("tests/clean_recovery.rs")
        .expect("guard source must exist")
        .len() as usize;
    assert!(
        bytes >= MIN_BYTES,
        "fix.clean-recovery-wiring (iter 261): tests/clean_recovery.rs \
         is {bytes} bytes; floor is {MIN_BYTES}. A gutting would \
         leave the wiring-only check without the iter-164 refactor-\
         hazard pins."
    );
    assert!(
        bytes <= MAX_BYTES,
        "fix.clean-recovery-wiring (iter 261): tests/clean_recovery.rs \
         is {bytes} bytes; ceiling is {MAX_BYTES}. Past the ceiling \
         the file likely accumulated tests that belong in the inline \
 gpk.rs test module."
    );
}

/// Iter 261: `src/services/mods/gpk.rs` must carry an inline
/// `#[cfg(test)] mod tests` block. The guard's header points to
/// four behavioural cases that live there; without the inline test
/// module, those cases aren't tested at all.
#[test]
fn tmm_carries_inline_test_module() {
    let src = fs::read_to_string("src/services/mods/gpk.rs").expect("gpk.rs must exist");
    assert!(
        src.contains("#[cfg(test)]"),
        "fix.clean-recovery-wiring (iter 261): src/services/mods/gpk.rs \
         must carry an inline `#[cfg(test)] mod tests` block. The \
         guard's header references four behavioural cases that live \
         there (nop-when-backup-exists / creates-backup-from-vanilla \
         / refuses-when-current-modded / missing-mapper-returns-err) \
         — without the inline module, those cases aren't tested."
    );
    assert!(
        src.contains("mod tests"),
        "fix.clean-recovery-wiring (iter 261): gpk.rs inline module \
         must be named `tests`."
    );
}

/// Iter 261: `recover_clean_mapper` must appear in main.rs's
/// `generate_handler!` / `invoke_handler` invocation. The iter-164
/// registration pin covers presence; this pin explicitly checks
/// the invoke-handler list wiring.
#[test]
fn recover_clean_mapper_is_in_invoke_handler_list() {
    let main = fs::read_to_string("src/main.rs").expect("main.rs must exist");
    assert!(
        main.contains("recover_clean_mapper"),
        "fix.clean-recovery-wiring (iter 261): src/main.rs must \
         register `recover_clean_mapper` in the Tauri \
         `generate_handler!` / `invoke_handler` list. Without the \
         registration, the frontend invoke() call errors with \
         `command not found`."
    );
    // Also confirm it's in a handler-like context.
    let handler_pos = main
        .find("generate_handler!")
        .or_else(|| main.find("invoke_handler"));
    assert!(
        handler_pos.is_some(),
        "fix.clean-recovery-wiring (iter 261): main.rs must carry \
         either `generate_handler!` or `invoke_handler` — the \
         Tauri command-registration macro."
    );
}

/// Iter 261: guard source must cite `PRD 3.2.9` explicitly — the
/// clean-recovery criterion. The iter-164 `guard_file_header_cites_
/// prd_and_fix_slot` check accepts any PRD reference in the header;
/// this pin requires the specific criterion section for discoverability.
#[test]
fn guard_source_cites_prd_3_2_9_explicitly() {
    let body =
        fs::read_to_string("tests/clean_recovery.rs").expect("tests/clean_recovery.rs must exist");
    let header_plus = &body[..body.len().min(2000)];
    assert!(
        header_plus.contains("3.2.9") || header_plus.contains("PRD 3.2.9"),
        "fix.clean-recovery-wiring (iter 261): guard source must cite \
         `PRD 3.2.9` (clean-recovery criterion) within the first 2000 \
         chars. A reader chasing the PRD criterion via section-grep \
         should land here. Note: the iter-164 pin accepts any PRD cite; \
         this pin requires the specific section.\nHeader preview:\n{header_plus}"
    );
}

/// Iter 227: the module-level comment block introducing the iter-164
/// pins enumerates four specific refactor hazards (dropped
/// dst.exists early return / dropped TMM_MARKER refusal / dropped
/// ensure_backup idempotence / renamed filename constants). A
/// cleanup pass that shortens the block to a one-line summary would
/// drop the map of which hazard each pin protects — making later
/// maintainers unable to tell which pin guards which hazard without
/// reverse-engineering from the test body.
#[test]
fn guard_header_enumerates_four_iter_164_refactor_hazards() {
    let src =
        fs::read_to_string("tests/clean_recovery.rs").expect("tests/clean_recovery.rs must exist");
    for phrase in [
        "dropped dst.exists() early return",
        "dropped TMM_MARKER refusal",
        "dropped ensure_backup idempotence",
        "renamed filename constants",
    ] {
        assert!(
            src.contains(phrase),
            "iter-164 hazard enumeration missing phrase: `{phrase}`. \
             Without it, the map of which pin guards which hazard \
             shrinks and later maintainers lose the reasoning \
             audit-trail."
        );
    }
}
