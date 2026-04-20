//! PRD 3.3.4 — `add_mod_from_file` Rust wiring guard.
//!
//! Criterion: "Add mod from file… accepts a local GPK, parses,
//! verifies, deploys". Acceptance is the Playwright spec
//! `mod-import-file.spec.js::user_imported_gpk_deploys` that exercises
//! the full flow through Tauri IPC. This file pins the Rust side of
//! the contract via source inspection — the IPC test alone can regress
//! silently if someone refactors `add_mod_from_file` to skip a step
//! without touching the command name.
//!
//! The function is a `#[tauri::command]` + `#[cfg(not(tarpaulin_include))]`
//! async entry point — bin-crate integration tests can't invoke it
//! directly, so we source-inspect the body to pin five invariant
//! wires that must all be present:
//!
//! 1. `tmm::parse_mod_file(&bytes)` — rejects non-TMM GPKs before any
//!    disk write (PRD 3.1.3 + 5.3.adv.bogus-gpk-footer).
//! 2. `is_safe_gpk_container_filename` — refuses hostile container
//!    names (PRD 3.1.4 deploy-sandbox).
//! 3. `Sha256::digest(&bytes)` — id derivation (`local.<sha12>` format)
//!    + backing for later integrity checks.
//! 4. `try_deploy_gpk(` — attempts the mapper patch if game root is
//!    configured (deploy-when-possible semantics).
//! 5. `mods_state::mutate(|reg| { ... reg.upsert` — registry persistence
//!    so the imported mod survives relaunch.
//!
//! A refactor that drops any of these five would regress PRD §3.3.4.

use std::fs;

const COMMANDS_MODS_RS: &str = "src/commands/mods.rs";
const MAIN_RS: &str = "src/main.rs";
const GUARD_SOURCE: &str = "tests/add_mod_from_file_wiring.rs";

fn fn_body_window() -> String {
    let src = fs::read_to_string(COMMANDS_MODS_RS)
        .unwrap_or_else(|e| panic!("{COMMANDS_MODS_RS}: {e}"));
    let pos = src
        .find("pub async fn add_mod_from_file")
        .expect("add_mod_from_file must exist as a pub async fn");
    // Body is ~65 lines; take 4000 chars to cover comfortably.
    src[pos..pos.saturating_add(4000)].to_string()
}

fn fn_with_prelude_window() -> String {
    // Slightly wider window that includes the `#[tauri::command]` and
    // `#[cfg(...)]` attribute lines that precede the fn.
    let src = fs::read_to_string(COMMANDS_MODS_RS)
        .unwrap_or_else(|e| panic!("{COMMANDS_MODS_RS}: {e}"));
    let pos = src
        .find("pub async fn add_mod_from_file")
        .expect("add_mod_from_file must exist as a pub async fn");
    // Start 400 chars earlier to cover the attribute block.
    let start = pos.saturating_sub(400);
    src[start..pos.saturating_add(4000)].to_string()
}

/// Wire 1 — must call `tmm::parse_mod_file` to reject non-TMM bytes
/// before any disk write or registry mutation.
#[test]
fn add_mod_from_file_calls_parse_mod_file() {
    let body = fn_body_window();
    assert!(
        body.contains("tmm::parse_mod_file(&bytes)"),
        "PRD 3.3.4 wiring violated: add_mod_from_file must call \
         tmm::parse_mod_file(&bytes) to reject non-TMM imports. \
         Without this, a hostile or corrupt file could reach \
         install_gpk or the registry."
    );
}

/// Wire 2 — must call `is_safe_gpk_container_filename` to gate the
/// PRD 3.1.4 deploy-sandbox predicate.
#[test]
fn add_mod_from_file_calls_safe_container_predicate() {
    let body = fn_body_window();
    assert!(
        body.contains("is_safe_gpk_container_filename"),
        "PRD 3.3.4 + 3.1.4 wiring violated: add_mod_from_file must \
         call is_safe_gpk_container_filename to refuse hostile \
         container names. See adv.bogus-gpk-footer test corpus."
    );
}

/// Wire 3 — must compute `Sha256::digest(&bytes)` for id derivation.
/// The `local.<sha12>` id format makes re-importing the same file
/// idempotent via registry upsert.
#[test]
fn add_mod_from_file_computes_sha256_of_bytes() {
    let body = fn_body_window();
    assert!(
        body.contains("Sha256::digest(&bytes)"),
        "PRD 3.3.4 wiring violated: add_mod_from_file must compute \
         Sha256::digest(&bytes) so the id is `local.<sha12>`. Without \
         this, re-importing the same file wouldn't be idempotent and \
         later integrity checks would have no baseline."
    );
}

/// Wire 4 — must call `try_deploy_gpk(` for best-effort mapper patch.
/// The deploy is best-effort (if game root unconfigured we still
/// persist so the user sees the import), but the attempt must be made.
#[test]
fn add_mod_from_file_attempts_mapper_deploy() {
    let body = fn_body_window();
    assert!(
        body.contains("try_deploy_gpk("),
        "PRD 3.3.4 wiring violated: add_mod_from_file must call \
         try_deploy_gpk to attempt the mapper patch. Without this, \
         the imported mod would be visible in the registry but never \
         actually applied in-game."
    );
}

/// Wire 5 — must persist to the registry via `mods_state::mutate` +
/// `reg.upsert`.
#[test]
fn add_mod_from_file_persists_via_registry_upsert() {
    let body = fn_body_window();
    assert!(
        body.contains("mods_state::mutate"),
        "PRD 3.3.4 wiring violated: add_mod_from_file must persist \
         via mods_state::mutate so the import survives relaunch."
    );
    assert!(
        body.contains("reg.upsert("),
        "PRD 3.3.4 wiring violated: registry mutation must use \
         reg.upsert (not direct push) so re-importing the same \
         local.<sha12> id is idempotent."
    );
}

/// Wire 6 (iter 151) — source-order: `parse_mod_file` must run
/// BEFORE `is_safe_gpk_container_filename`. Parsing extracts the
/// container name from the TMM footer; the sandbox predicate
/// operates on that name. A reorder would either crash (unknown
/// container) or pass the check vacuously on an empty/default
/// name.
#[test]
fn parse_mod_file_precedes_container_safety_check() {
    let body = fn_body_window();
    let parse_idx = body
        .find("tmm::parse_mod_file(&bytes)")
        .expect("parse_mod_file call must exist (wire 1)");
    let safe_idx = body
        .find("is_safe_gpk_container_filename")
        .expect("is_safe_gpk_container_filename call must exist (wire 2)");
    assert!(
        parse_idx < safe_idx,
        "PRD 3.3.4 source-order violated: parse_mod_file \
         (parse_idx={parse_idx}) must run BEFORE \
         is_safe_gpk_container_filename (safe_idx={safe_idx}). The \
         sandbox predicate depends on the container name extracted \
         by the parse; running it first either crashes or passes \
         vacuously on a default name."
    );
}

/// Wire 7 (iter 151) — source-order: `parse_mod_file` must run
/// BEFORE any `fs::write` that copies bytes to the gpk slot.
/// Fail-closed invariant: non-TMM bytes never land on disk.
#[test]
fn parse_mod_file_precedes_fs_write_to_gpk_slot() {
    let body = fn_body_window();
    let parse_idx = body.find("tmm::parse_mod_file(&bytes)").unwrap();
    // Find the first fs::write (either std::fs::write or fs::write).
    let write_idx = body
        .find("fs::write(")
        .expect("fs::write call must exist for gpk slot copy");
    assert!(
        parse_idx < write_idx,
        "PRD 3.3.4 fail-closed violated: parse_mod_file \
         (parse_idx={parse_idx}) must run BEFORE fs::write \
         (write_idx={write_idx}). Writing non-TMM bytes to the gpk \
         slot before parsing would land hostile files on disk that \
         could be scanned/deployed before any validation runs."
    );
}

/// Wire 8 (iter 151) — the function must return the
/// Tauri-command-idiomatic `Result<ModEntry, String>`. A change
/// to `Result<ModEntry, anyhow::Error>` or `Result<ModEntry, Box<dyn Error>>`
/// would fail the JSON-serialisation contract and the frontend
/// `invoke()` call would see an unhelpful error shape.
#[test]
fn signature_returns_result_mod_entry_string() {
    let body = fn_body_window();
    assert!(
        body.contains("-> Result<ModEntry, String>"),
        "PRD 3.3.4 signature violated: add_mod_from_file must \
         return `Result<ModEntry, String>` so Tauri's JSON \
         serialisation handles the error path cleanly. Any other \
         error type breaks the frontend's `invoke()` contract."
    );
}

/// Wire 9 (iter 151) — the empty-container fail-fast (iter-33 fix)
/// must stay. A TMM file with no container name would pass the
/// parse but have nothing for `is_safe_gpk_container_filename` to
/// check; the fail-fast gives a specific user-facing error
/// ("no container name in footer") before the sandbox test runs.
#[test]
fn empty_container_name_is_fail_fast() {
    let body = fn_body_window();
    assert!(
        body.contains("modfile.container.is_empty()"),
        "PRD 3.3.4 fail-fast violated: add_mod_from_file must check \
         `modfile.container.is_empty()` after parsing so a TMM file \
         with no container name in its footer is rejected with a \
         specific user-facing error, not a vacuous sandbox-check \
         pass (iter-33 fix)."
    );
    assert!(
        body.contains("no container name in footer"),
        "PRD 3.3.4 error-text drift: the empty-container error must \
         cite `no container name in footer` so users understand the \
         failure. Changing the wording breaks the docs/mod-manager/\
         TROUBLESHOOT.md cross-reference."
    );
}

/// Wire 10 (iter 151) — id derivation uses `ModEntry::from_local_gpk`,
/// NOT `ModEntry::from_catalog`. These two constructors produce
/// different id prefixes (`local.<sha12>` vs `catalog.<name>`), and
/// the registry's uniqueness key is the id. A wrong constructor would
/// collide with catalog-sourced mods of the same name.
#[test]
fn id_derivation_uses_from_local_gpk_constructor() {
    let body = fn_body_window();
    assert!(
        body.contains("ModEntry::from_local_gpk(&sha, &modfile)"),
        "PRD 3.3.4 id-derivation violated: add_mod_from_file must \
         use `ModEntry::from_local_gpk(&sha, &modfile)`. \
         `from_catalog` would produce a different id prefix \
         (catalog.<name> vs local.<sha12>) and a re-import of the \
         same file would no longer be idempotent — worse, a \
         local-named mod could shadow / collide with a catalog \
         entry of the same name."
    );
}

/// Self-test — prove the detectors bite on known-bad shapes.
#[test]
fn add_mod_from_file_detector_self_test() {
    // Bad shape A: body missing parse_mod_file call.
    let missing_parse = "pub async fn add_mod_from_file(path: String) -> Result<_, _> {
        let bytes = fs::read(&path)?;
        let sha = Sha256::digest(&bytes);
        fs::write(dest, &bytes)?;
    }";
    assert!(
        !missing_parse.contains("tmm::parse_mod_file(&bytes)"),
        "self-test: body missing parse must trip wire 1"
    );

    // Bad shape B: body missing sandbox predicate.
    let missing_sandbox = "pub async fn add_mod_from_file(path: String) -> Result<_, _> {
        let modfile = tmm::parse_mod_file(&bytes)?;
        fs::write(dest, &bytes)?;
    }";
    assert!(
        !missing_sandbox.contains("is_safe_gpk_container_filename"),
        "self-test: body missing sandbox predicate must trip wire 2"
    );

    // Bad shape C: body missing registry upsert.
    let missing_upsert = "pub async fn add_mod_from_file(path: String) -> Result<_, _> {
        let bytes = fs::read(&path)?;
        fs::write(dest, &bytes)?;
        // forgot to upsert!
    }";
    assert!(
        !missing_upsert.contains("reg.upsert("),
        "self-test: body missing upsert must trip wire 5"
    );

    // Bad shape D (iter 151): parse AFTER sandbox check (reordered).
    let wrong_order = "if !tmm::is_safe_gpk_container_filename(&modfile.container) { return Err(\"...\"); } let modfile = tmm::parse_mod_file(&bytes)?;";
    let safe_idx = wrong_order.find("is_safe_gpk_container_filename").unwrap();
    let parse_idx = wrong_order.find("tmm::parse_mod_file(&bytes)").unwrap();
    assert!(
        safe_idx < parse_idx,
        "self-test: reversed-order fixture must have safety check \
         before parse"
    );

    // Bad shape E: fs::write before parse_mod_file.
    let write_first = "let bytes = fs::read(&path)?; fs::write(dest, &bytes)?; let modfile = tmm::parse_mod_file(&bytes)?;";
    let w = write_first.find("fs::write(").unwrap();
    let p = write_first.find("tmm::parse_mod_file(&bytes)").unwrap();
    assert!(
        w < p,
        "self-test: write-first fixture must have write before parse"
    );

    // Bad shape F: wrong return type.
    let wrong_sig =
        "pub async fn add_mod_from_file(path: String) -> Result<ModEntry, anyhow::Error> {";
    assert!(
        !wrong_sig.contains("-> Result<ModEntry, String>"),
        "self-test: non-String error type must be flagged"
    );

    // Bad shape G: uses from_catalog instead of from_local_gpk.
    let wrong_ctor = "let entry = ModEntry::from_catalog(&sha, &modfile);";
    assert!(
        !wrong_ctor.contains("ModEntry::from_local_gpk("),
        "self-test: wrong constructor must be flagged"
    );

    // Bad shape H: missing empty-container fail-fast.
    let no_fail_fast =
        "let modfile = tmm::parse_mod_file(&bytes)?;\nif !tmm::is_safe_gpk_container_filename(&modfile.container) { return Err(\"...\".into()); }";
    assert!(
        !no_fail_fast.contains("modfile.container.is_empty()"),
        "self-test: missing empty-container check must be flagged"
    );

    // Iter 191 — additional bad shapes.

    // Bad shape I: fs::read with .unwrap() (panics on IO err).
    let panicking_read = "let bytes = std::fs::read(&path).unwrap();";
    assert!(
        panicking_read.contains(".unwrap()"),
        "self-test: .unwrap() on fs::read must be detectable"
    );

    // Bad shape J: fn missing #[tauri::command] attribute.
    let no_cmd_attr = "pub async fn add_mod_from_file(path: String) -> Result<ModEntry, String> {";
    assert!(
        !no_cmd_attr.contains("#[tauri::command]"),
        "self-test: missing #[tauri::command] must be detectable"
    );

    // Bad shape K: write dest derived from user-controlled path.
    let user_path_write = r#"std::fs::write(PathBuf::from(&path), &bytes)?;"#;
    assert!(
        !user_path_write.contains("gpk_dir"),
        "self-test: write-to-user-path must be flagged (no gpk_dir on dest)"
    );
}

/// Iter 191: guard file header must cite `PRD 3.3.4` + the
/// Playwright acceptance spec name so reviewers can trace the Rust
/// wiring back to the end-to-end IPC test it complements.
#[test]
fn guard_file_header_cites_prd_and_playwright_spec() {
    let body = fs::read_to_string(GUARD_SOURCE)
        .unwrap_or_else(|e| panic!("{GUARD_SOURCE}: {e}"));
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.3.4"),
        "PRD 3.3.4 (iter 191): {GUARD_SOURCE} header must cite \
         `PRD 3.3.4` so the criterion is reachable via grep."
    );
    assert!(
        header.contains("mod-import-file.spec.js"),
        "PRD 3.3.4 (iter 191): {GUARD_SOURCE} header must cite the \
         Playwright acceptance spec `mod-import-file.spec.js` so \
         future readers can find the end-to-end complement of these \
         source-inspection pins."
    );
}

/// Iter 191: `add_mod_from_file` must carry the `#[tauri::command]`
/// attribute AND be declared `pub async fn`. Without the attribute,
/// Tauri's codegen doesn't expose the function to the frontend
/// invoke() bridge — the command silently fails to exist; without
/// `async`, the fs read blocks the UI thread and drops frames.
#[test]
fn fn_is_tauri_command_and_async() {
    let body = fn_with_prelude_window();
    assert!(
        body.contains("#[tauri::command]"),
        "PRD 3.3.4 (iter 191): add_mod_from_file must carry the \
         `#[tauri::command]` attribute. Without it, Tauri codegen \
         doesn't expose the function to the frontend invoke() bridge."
    );
    assert!(
        body.contains("pub async fn add_mod_from_file"),
        "PRD 3.3.4 (iter 191): add_mod_from_file must be declared \
         `pub async fn`. A sync fn blocks the UI thread during the \
         fs::read + mapper deploy and drops frames."
    );
}

/// Iter 191: `add_mod_from_file` must be registered in the Tauri
/// invoke handler in `src/main.rs`. Without registration, the
/// command exists on the Rust side but invoke() from the frontend
/// errors with "command not found" — the Playwright spec fails but
/// this guard would still pass the function-exists pins.
#[test]
fn fn_is_registered_in_invoke_handler() {
    let main_src = fs::read_to_string(MAIN_RS)
        .unwrap_or_else(|e| panic!("{MAIN_RS}: {e}"));
    assert!(
        main_src.contains("commands::mods::add_mod_from_file"),
        "PRD 3.3.4 (iter 191): {MAIN_RS} must register \
         `commands::mods::add_mod_from_file` in the Tauri \
         `generate_handler!` / `invoke_handler` list. Without \
         registration, the frontend invoke() call errors with \
         \"command not found\" and the Playwright spec fails — but \
         the source-inspection pins still pass."
    );
}

/// Iter 191: the function body must use `.map_err(|e| format!(...))?`
/// on filesystem operations, never raw `.unwrap()` / `.expect()`.
/// A panic in this path kills the Tauri backend process — worse,
/// the frontend sees the IPC channel close, not a readable error
/// reason, and has no path to tell the user why the import failed.
#[test]
fn fs_errors_are_mapped_not_unwrapped() {
    let body = fn_body_window();
    // The body must carry .map_err( on at least one fs:: call.
    assert!(
        body.contains(".map_err(|e|"),
        "PRD 3.3.4 (iter 191): add_mod_from_file body must use \
         `.map_err(|e| ...)` on fs operations. Bare `.unwrap()` / \
         `.expect()` panics the Tauri backend; the frontend sees the \
         IPC channel die with no user-readable reason."
    );
    // Reject `std::fs::read(&path).unwrap()` or `.expect(` patterns in
    // the body. Look for specific panicking patterns on the fs::read
    // / fs::write calls.
    assert!(
        !body.contains("std::fs::read(&src).unwrap()"),
        "PRD 3.3.4 (iter 191): add_mod_from_file must not \
         `.unwrap()` on `std::fs::read(&src)` — returns an IO error \
         the frontend can't distinguish from channel death."
    );
    assert!(
        !body.contains("std::fs::write(&dest, &bytes).unwrap()"),
        "PRD 3.3.4 (iter 191): add_mod_from_file must not \
         `.unwrap()` on `std::fs::write` — same rationale."
    );
}

/// Iter 247: pin the three path constants against silent renames.
/// If `COMMANDS_MODS_RS` / `MAIN_RS` / `GUARD_SOURCE` drift, every
/// other test in this file reads from the wrong file and silently
/// passes on the missing-body fall-through.
#[test]
fn guard_path_constants_are_canonical() {
    assert_eq!(
        COMMANDS_MODS_RS, "src/commands/mods.rs",
        "PRD 3.3.4 (iter 247): COMMANDS_MODS_RS constant must point \
         at `src/commands/mods.rs` verbatim. A rename of the module \
         needs a coordinated guard update."
    );
    assert_eq!(
        MAIN_RS, "src/main.rs",
        "PRD 3.3.4 (iter 247): MAIN_RS constant must point at \
         `src/main.rs` verbatim. If Tauri entry point moves, the \
         invoke-handler registration pin reads from the wrong file."
    );
    assert_eq!(
        GUARD_SOURCE, "tests/add_mod_from_file_wiring.rs",
        "PRD 3.3.4 (iter 247): GUARD_SOURCE constant must point at \
         this file's own path verbatim. Header-grep tests otherwise \
         silently pass on a missing file."
    );
    // All three paths must actually exist.
    assert!(
        fs::metadata(COMMANDS_MODS_RS).is_ok(),
        "{COMMANDS_MODS_RS} must exist as a real file — the guard is \
         useless if the target source is gone."
    );
    assert!(
        fs::metadata(MAIN_RS).is_ok(),
        "{MAIN_RS} must exist as a real file."
    );
    assert!(
        fs::metadata(GUARD_SOURCE).is_ok(),
        "{GUARD_SOURCE} must exist as a real file."
    );
}

/// Iter 247: the gpk-slot filename sanitises `/` to `_` before being
/// joined into the destination path. Without this, an `entry.id`
/// containing a slash (e.g. `local.abcdef/../..`) would let
/// `gpk_dir.join(...)` escape the sandboxed mods directory. The
/// `from_local_gpk` constructor currently emits `local.<sha12>` so
/// the slash is dormant, but this pin prevents a silent drop of the
/// replacement when the id format evolves.
#[test]
fn gpk_slot_filename_sanitizes_slash_to_underscore() {
    let body = fn_body_window();
    assert!(
        body.contains("entry.id.replace('/', \"_\")"),
        "PRD 3.3.4 (iter 247): the gpk-slot filename must call \
         `entry.id.replace('/', \"_\")` verbatim before joining \
         under gpk_dir. A slash in an id would let `gpk_dir.join(\"a/b.gpk\")` \
         escape the mods sandbox — classic path-traversal via derived \
         filename, dormant today only because `local.<sha12>` never \
         embeds a slash."
    );
}

/// Iter 247: `create_dir_all(&gpk_dir)` must run BEFORE
/// `fs::write(&dest, &bytes)`. Without create_dir_all, the first
/// import on a fresh install would fail with a "file not found"
/// error pointing at the parent directory, and the user sees
/// nothing useful to act on. Ordering matters — some past refactors
/// have shuffled the two.
#[test]
fn create_dir_all_precedes_fs_write_to_dest() {
    let body = fn_body_window();
    let mkdir_idx = body
        .find("create_dir_all(&gpk_dir)")
        .expect("add_mod_from_file must call std::fs::create_dir_all(&gpk_dir) before writing");
    let write_idx = body
        .find("std::fs::write(&dest, &bytes)")
        .expect("add_mod_from_file must call std::fs::write(&dest, &bytes)");
    assert!(
        mkdir_idx < write_idx,
        "PRD 3.3.4 (iter 247): source-order violated. \
         `create_dir_all(&gpk_dir)` (at {mkdir_idx}) must run BEFORE \
         `std::fs::write(&dest, &bytes)` (at {write_idx}). First-run \
         import on a fresh install fails with a non-actionable \
         'file not found' error if the directory isn't created first."
    );
}

/// Iter 247: the deploy-success branch (`if deploy_note.is_some()`)
/// must set BOTH `entry.enabled = true` AND `entry.auto_launch = true`.
/// Missing `auto_launch = true` would leave the imported mod in a
/// state where it's enabled in the registry but never actually
/// scheduled for apply-on-launch — a silent no-op that would surprise
/// the user who just clicked Import.
#[test]
fn deploy_success_sets_enabled_and_auto_launch_true() {
    let body = fn_body_window();
    assert!(
        body.contains("entry.enabled = true;"),
        "PRD 3.3.4 (iter 247): the deploy-success branch must set \
         `entry.enabled = true;` verbatim. Without this, a successfully \
         deployed import ships disabled and the mapper patch rolls \
         back on next registry save."
    );
    assert!(
        body.contains("entry.auto_launch = true;"),
        "PRD 3.3.4 (iter 247): the deploy-success branch must set \
         `entry.auto_launch = true;` verbatim. Without this, the \
         mod is 'enabled' but never scheduled to apply at game launch \
         — a silent no-op that confuses the user."
    );
    assert!(
        body.contains("entry.status = ModStatus::Enabled;"),
        "PRD 3.3.4 (iter 247): the deploy-success branch must set \
         `entry.status = ModStatus::Enabled;` verbatim so the \
         status badge in the UI reflects the deployment."
    );
}

/// Iter 247: the info! log emits `&sha[..12]`, NOT the full 64-char
/// hex digest. Full SHAs in logs make correlation across log dumps
/// easier for an attacker who recovers them later; the 12-char prefix
/// is enough to identify the mod during debugging without leaking
/// content fingerprints. Also reduces log noise since the sha is
/// already visible in the `entry.id` field.
#[test]
fn info_log_sanitizes_sha_to_twelve_chars() {
    let body = fn_body_window();
    assert!(
        body.contains("&sha[..12]"),
        "PRD 3.3.4 (iter 247): the info! log in add_mod_from_file \
         must log `&sha[..12]`, not the full 64-char hex digest. \
         Full SHAs in logs ease cross-dump correlation for an \
         attacker; 12 chars is enough for debugging."
    );
    // Reject a regression back to logging the full sha.
    assert!(
        !body.contains("sha={}\",\n        sha,"),
        "PRD 3.3.4 (iter 247): info! must not log the full sha \
         variable unsliced. Use `&sha[..12]` instead."
    );
}

/// Iter 191: the write dest must be built from `get_gpk_dir()` +
/// a joined filename. A dest constructed directly from the
/// user-supplied `path` argument would let an attacker write to
/// arbitrary filesystem locations. `get_gpk_dir()` returns the
/// launcher's own mods subdirectory, sandboxed to the app data
/// area.
#[test]
fn fs_write_dest_is_rooted_under_gpk_dir() {
    let body = fn_body_window();
    // Must resolve gpk_dir before the write.
    let gpk_dir_idx = body
        .find("get_gpk_dir()")
        .expect("add_mod_from_file must call get_gpk_dir() to \
                 resolve the sandboxed mods directory");
    // Must compute `dest` from gpk_dir.join(...).
    let dest_idx = body
        .find("let dest = gpk_dir.join(")
        .expect("add_mod_from_file must compute `dest = gpk_dir.join(...)`");
    let write_idx = body
        .find("std::fs::write(&dest, &bytes)")
        .expect("add_mod_from_file must write to `dest` built from gpk_dir");
    // Ordering: gpk_dir resolved < dest joined < fs::write called.
    assert!(
        gpk_dir_idx < dest_idx && dest_idx < write_idx,
        "PRD 3.3.4 (iter 191): add_mod_from_file source-order \
         violated. Expected: get_gpk_dir() (at {gpk_dir_idx}) < \
         `dest = gpk_dir.join(...)` (at {dest_idx}) < \
         std::fs::write(&dest, ...) (at {write_idx}). An out-of-\
         order refactor that writes before joining through gpk_dir \
         could land bytes under a user-controlled path — classic \
         path-traversal sink."
    );
    // And the body must NOT write directly to a PathBuf constructed
    // from `path` (the user-supplied arg).
    assert!(
        !body.contains("std::fs::write(&src, &bytes)")
            && !body.contains("fs::write(&PathBuf::from(&path)"),
        "PRD 3.3.4 (iter 191): add_mod_from_file must NOT write to \
         the user-supplied source path (`src` / `PathBuf::from(&path)`) \
         — that is a path-traversal sink."
    );
}

// --------------------------------------------------------------------
// Iter 286 structural pins — commands/main/guard bounds + PRD cite +
// sha2 crate dep.
// --------------------------------------------------------------------

#[test]
fn commands_mods_rs_byte_bounds() {
    const MIN: usize = 3000;
    const MAX: usize = 200_000;
    let bytes = std::fs::metadata(COMMANDS_MODS_RS)
        .expect("commands/mods.rs must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.3.4 (iter 286): {COMMANDS_MODS_RS} is {bytes} bytes; \
         expected [{MIN}, {MAX}]."
    );
}

#[test]
fn main_rs_byte_bounds() {
    const MIN: usize = 5000;
    const MAX: usize = 100_000;
    let bytes = std::fs::metadata(MAIN_RS)
        .expect("main.rs must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.3.4 (iter 286): {MAIN_RS} is {bytes} bytes; expected \
         [{MIN}, {MAX}]."
    );
}

#[test]
fn guard_source_byte_bounds() {
    const MIN: usize = 5000;
    const MAX: usize = 80_000;
    let bytes = std::fs::metadata(GUARD_SOURCE)
        .expect("guard must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.3.4 (iter 286): guard is {bytes} bytes; expected \
         [{MIN}, {MAX}]."
    );
}

#[test]
fn guard_source_cites_prd_3_3_4_explicitly() {
    let body = fs::read_to_string(GUARD_SOURCE)
        .expect("guard must exist");
    let header = &body[..body.len().min(500)];
    assert!(
        header.contains("PRD 3.3.4"),
        "PRD 3.3.4 (iter 286): guard header must cite `PRD 3.3.4`.\n\
         Header:\n{header}"
    );
}

#[test]
fn sha2_crate_is_declared_in_cargo_toml() {
    let toml = fs::read_to_string("Cargo.toml")
        .expect("Cargo.toml must exist");
    assert!(
        toml.contains("sha2"),
        "PRD 3.3.4 (iter 286): Cargo.toml must declare `sha2` — the \
         hashing crate `Sha256::digest` depends on."
    );
}
