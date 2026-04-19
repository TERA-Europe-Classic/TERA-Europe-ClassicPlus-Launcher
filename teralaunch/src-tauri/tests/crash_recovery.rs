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

// --- adv.sigkill-mid-download (iter 95) ------------------------------------
//
// PRD §5.3 requires that a SIGKILL mid-download leaves (a) the registry
// row recoverable to Error on boot, (b) any partial file on disk removed
// before the retry commits. The registry-side half lives in
// `src/services/mods/registry.rs::tests` (four tests). The filesystem-
// side half is structural: downloads buffer bytes in memory via
// `fetch_bytes_streaming`, so a SIGKILL mid-download leaves NO on-disk
// partial — the download either completes fully in RAM before any
// `fs::write` / `extract_zip` call, or dies without touching disk.
//
// The residual failure mode is SIGKILL AFTER the memory buffer is full
// but DURING the commit step:
//   - `download_and_extract` (external): extract_zip wrote some files
//     into `dest_dir` before the signal. The RETRY path MUST clear
//     `dest_dir` before re-extracting, otherwise leftover files from
//     the crashed install would mix with the new install's files.
//   - `download_file` (GPK): `fs::write(dest_file, &bytes)` may leave
//     a half-written file if SIGKILL hit mid-write. The RETRY path
//     must overwrite (not append to) the partial. `fs::write`
//     truncates, so this is automatic.
//
// These two source-inspection tests pin the "retry path cleans up
// before commit" invariant. A refactor that drops either check would
// break adv.sigkill-mid-download's filesystem half without any
// behavioural test firing.

const EXTERNAL_APP_RS: &str = "src/services/mods/external_app.rs";

#[test]
fn sigkill_recovery_external_retry_clears_dest_dir_before_extract() {
    // adv.sigkill-mid-download (filesystem half, external path).
    //
    // `download_and_extract` must remove any existing `dest_dir` before
    // calling `extract_zip`. Without this, a SIGKILL mid-extract would
    // leave leftover files that the next install's extract would
    // silently merge with — producing a franken-mod tree.
    let src = fs::read_to_string(EXTERNAL_APP_RS).expect("external_app.rs must exist");
    let fn_pos = src
        .find("pub async fn download_and_extract")
        .or_else(|| src.find("async fn download_and_extract"))
        .expect("download_and_extract must exist");
    // Window: the function body is ~80 lines; take 3000 chars.
    let window = &src[fn_pos..fn_pos.saturating_add(3000)];

    // The cleanup branch must appear in the function body.
    assert!(
        window.contains("if dest_dir.exists()") || window.contains("dest_dir.exists() {"),
        "download_and_extract must check dest_dir.exists() — see \
         adv.sigkill-mid-download filesystem invariant"
    );
    assert!(
        window.contains("remove_dir_all(dest_dir)"),
        "download_and_extract must remove_dir_all(dest_dir) before \
         extract_zip — otherwise a retry leaves SIGKILL leftovers \
         mixed into the new install"
    );

    // Ordering: the remove_dir_all must appear BEFORE extract_zip in
    // source order. Otherwise the check is dead code.
    let remove_idx = window
        .find("remove_dir_all(dest_dir)")
        .expect("remove_dir_all must exist");
    let extract_idx = window
        .find("extract_zip(")
        .expect("extract_zip call must exist in download_and_extract");
    assert!(
        remove_idx < extract_idx,
        "remove_dir_all(dest_dir) must appear before extract_zip() \
         in download_and_extract"
    );
}

#[test]
fn sigkill_recovery_gpk_retry_truncates_partial_via_fs_write() {
    // adv.sigkill-mid-download (filesystem half, GPK path).
    //
    // `download_file` writes the downloaded bytes via `fs::write(dest_file,
    // &bytes)`. `fs::write` is implemented as open(TRUNC) + write + close,
    // which overwrites any partial file from a prior interrupted install.
    // Pin the call shape — a refactor to `OpenOptions::new().append(true)`
    // or `std::io::Write::write_all` on a pre-existing file handle would
    // break the implicit truncation.
    let src = fs::read_to_string(EXTERNAL_APP_RS).expect("external_app.rs must exist");
    let fn_pos = src
        .find("pub async fn download_file")
        .or_else(|| src.find("async fn download_file"))
        .expect("download_file must exist");
    let window = &src[fn_pos..fn_pos.saturating_add(2000)];

    assert!(
        window.contains("fs::write(dest_file"),
        "download_file must use fs::write (truncating write) — \
         any non-truncating API would leave SIGKILL partial GPK \
         bytes mixed with new download"
    );
}

#[test]
fn sigkill_recovery_detector_self_test() {
    // Self-test: prove the source-inspection detectors above bite
    // on known-bad shapes. If the detectors themselves regressed,
    // the real tests would silently pass.
    let no_cleanup = "pub async fn download_and_extract() {\n  \
                      extract_zip(bytes, dest_dir);\n}";
    assert!(
        !no_cleanup.contains("remove_dir_all(dest_dir)"),
        "self-test: detector must flag a function missing the cleanup"
    );

    let wrong_order = "pub async fn download_and_extract() {\n  \
                       extract_zip(bytes, dest_dir);\n  \
                       if dest_dir.exists() { fs::remove_dir_all(dest_dir).unwrap(); }\n}";
    let remove_idx = wrong_order.find("remove_dir_all(dest_dir)").unwrap();
    let extract_idx = wrong_order.find("extract_zip(").unwrap();
    assert!(
        remove_idx > extract_idx,
        "self-test: detector ordering check must reject dead-code cleanup"
    );
}

// --------------------------------------------------------------------
// Iter 161 structural pins — recover_stuck_installs wiring + save
// atomicity.
// --------------------------------------------------------------------
//
// The JSON + filesystem pins above prove the shape of the recovery
// contract. These pins protect the in-memory SIDE of that contract —
// the Rust functions that actually read the stranded row, flip its
// status, clear progress, write the last_error message, and persist
// atomically. A refactor that drops the recover call from load(),
// widens the status match, forgets to clear progress, or replaces
// the atomic rename with a direct write would pass every test above
// while silently breaking boot-time recovery.

const REGISTRY_RS: &str = "src/services/mods/registry.rs";

fn registry_src() -> String {
    fs::read_to_string(REGISTRY_RS)
        .unwrap_or_else(|e| panic!("{REGISTRY_RS} must be readable: {e}"))
}

/// `Registry::load` must call `reg.recover_stuck_installs()` before
/// returning the loaded registry. Without this call, stranded
/// `Installing` rows survive across launcher restarts forever — the
/// UI keeps showing "installing…" at 42 % for a mod that will never
/// progress, and the user can't retry because the try_claim gate
/// still sees Installing (§3.2.7).
#[test]
fn load_calls_recover_stuck_installs_before_return() {
    let body = registry_src();
    let fn_pos = body
        .find("pub fn load(path: &Path) -> Result<Self, String>")
        .expect("Registry::load must exist");
    let rest = &body[fn_pos..];
    let end = rest.find("\n    }\n").unwrap_or(rest.len().min(1000));
    let fn_body = &rest[..end];
    let recover_pos = fn_body
        .find("recover_stuck_installs()")
        .expect(
            "Registry::load must call recover_stuck_installs() — \
             without it, stranded Installing rows never get flipped \
             to Error and the UI is stuck forever (PRD 3.2.2)",
        );
    let return_pos = fn_body
        .find("Ok(reg)")
        .expect("Registry::load must return Ok(reg)");
    assert!(
        recover_pos < return_pos,
        "PRD 3.2.2: recover_stuck_installs() must be called BEFORE \
         the `Ok(reg)` return. Calling it after means the registry \
         returned to callers still has stranded Installing rows."
    );
}

/// `recover_stuck_installs` must match ONLY on `ModStatus::Installing`.
/// Widening to `Installing | Error` (or `!= Enabled`) would re-flip
/// already-recovered rows on every load, stamping a fresh last_error
/// message each boot and defeating idempotence. Narrowing (removing
/// the check entirely) flips every row to Error — catastrophic.
#[test]
fn recover_stuck_installs_matches_only_installing_variant() {
    let body = registry_src();
    let fn_pos = body
        .find("pub fn recover_stuck_installs(")
        .expect("recover_stuck_installs must exist");
    let rest = &body[fn_pos..];
    let end = rest.find("\n    }\n").unwrap_or(rest.len().min(1200));
    let fn_body = &rest[..end];
    assert!(
        fn_body.contains("m.status == ModStatus::Installing"),
        "PRD 3.2.2: recover_stuck_installs must compare exactly \
         `m.status == ModStatus::Installing`. Widening breaks \
         idempotence (re-stamps last_error each boot); narrowing \
         either misses stranded rows or flips everything.\n\
         Body:\n{fn_body}"
    );
    // The widened `|` variant pattern must not appear.
    assert!(
        !fn_body.contains("ModStatus::Installing | ModStatus::")
            && !fn_body.contains("ModStatus::Error | ModStatus::Installing"),
        "PRD 3.2.2: recover_stuck_installs must NOT match on an OR \
         of variants — re-flipping Error rows on every load defeats \
         idempotence.\nBody:\n{fn_body}"
    );
}

/// The recovery body must clear `progress = None` on the stranded
/// row. A recovered row with `progress: 42` still renders as a
/// progress bar in the UI (looks like "still installing" to the
/// user) — exactly the confusion the status flip was supposed to
/// resolve. The `error_state_expected_shape` test above pins the
/// JSON absence of `progress`; this pin guards the Rust side.
#[test]
fn recover_stuck_installs_clears_progress_field() {
    let body = registry_src();
    let fn_pos = body
        .find("pub fn recover_stuck_installs(")
        .expect("recover_stuck_installs must exist");
    let rest = &body[fn_pos..];
    let end = rest.find("\n    }\n").unwrap_or(rest.len().min(1200));
    let fn_body = &rest[..end];
    assert!(
        fn_body.contains("m.progress = None"),
        "PRD 3.2.2: recover_stuck_installs must clear \
         `m.progress = None` on the recovered row. Leaving a numeric \
         progress value makes the UI still render the progress bar \
         — the user sees `installing at 42 %` for a row that will \
         never move.\nBody:\n{fn_body}"
    );
}

/// The recovery message must contain the phrase "Install was
/// interrupted". This string is shared between `error_state_expected
/// _shape` above (JSON assertion) and this guard (Rust assertion) —
/// if one changes without the other, the two halves of the §3.2.2
/// contract silently drift.
#[test]
fn recover_stuck_installs_last_error_says_interrupted() {
    let body = registry_src();
    let fn_pos = body
        .find("pub fn recover_stuck_installs(")
        .expect("recover_stuck_installs must exist");
    let rest = &body[fn_pos..];
    let end = rest.find("\n    }\n").unwrap_or(rest.len().min(1200));
    let fn_body = &rest[..end];
    assert!(
        fn_body.contains("Install was interrupted"),
        "PRD 3.2.2: recover_stuck_installs must stamp a last_error \
         containing the phrase `Install was interrupted`. \
         Changing it here without updating error_state_expected_shape \
         splits the JSON + Rust sides of the contract.\n\
         Body:\n{fn_body}"
    );
}

/// `Registry::save` must write to a `.tmp` sibling and then `rename`.
/// A direct `fs::write(path, body)` could leave the registry half-
/// written if the launcher SIGKILLs mid-write, corrupting the file
/// such that `Registry::load`'s `serde_json::from_str` fails and the
/// user loses every mod row on next boot. Atomic rename is the only
/// safe shape.
#[test]
fn save_is_atomic_tmp_plus_rename_not_direct_write() {
    let body = registry_src();
    let fn_pos = body
        .find("pub fn save(&self, path: &Path) -> Result<(), String>")
        .expect("Registry::save must exist");
    let rest = &body[fn_pos..];
    let end = rest.find("\n    }\n").unwrap_or(rest.len().min(1500));
    let fn_body = &rest[..end];
    // A `.tmp` sibling must be written to.
    let tmp_write_pos = fn_body
        .find("fs::write(&tmp")
        .expect(
            "PRD 3.2.2: Registry::save must write to `&tmp` (a .tmp \
             sibling) first. Direct `fs::write(path, ...)` corrupts \
             the registry on SIGKILL mid-write.",
        );
    // Then atomically renamed to the real path.
    let rename_pos = fn_body
        .find("fs::rename(&tmp, path)")
        .expect(
            "PRD 3.2.2: Registry::save must call \
             `fs::rename(&tmp, path)` after the tmp write. Without \
             the rename, the real registry file never gets the new \
             content.",
        );
    assert!(
        tmp_write_pos < rename_pos,
        "PRD 3.2.2: the tmp write must precede the rename in source \
         order. Otherwise the rename clobbers the real file with an \
         empty tmp."
    );
}

// --------------------------------------------------------------------
// Iter 194 structural pins — guard traceability + load error path +
// create-before-extract + extract-failure revert + first-run fallback.
// --------------------------------------------------------------------

const GUARD_SOURCE: &str = "tests/crash_recovery.rs";

/// Iter 194: guard source header must cite the PRD criterion AND
/// the `adv.sigkill-mid-download` fix-plan slot so both layers of
/// the recovery contract (normal crash + SIGKILL mid-download) are
/// reachable via grep.
#[test]
fn guard_file_cites_prd_and_adv_sigkill() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    // PRD criterion must be in the leading module comment for
    // at-a-glance visibility.
    let header = &body[..body.len().min(2500)];
    assert!(
        header.contains("PRD 3.2.2"),
        "PRD 3.2.2 (iter 194): {GUARD_SOURCE} header must cite \
         `PRD 3.2.2` so the criterion is reachable via grep."
    );
    // adv.sigkill-mid-download citation can appear anywhere in the
    // file (it's referenced in the iter-95 section marker); grep-
    // reachability is the goal.
    assert!(
        body.contains("adv.sigkill-mid-download"),
        "PRD 3.2.2 (iter 194): {GUARD_SOURCE} must cite \
         `adv.sigkill-mid-download` somewhere in the file so the \
         fix-plan P-slot name is reachable via grep."
    );
}

/// Iter 194: `Registry::load` must map BOTH the `fs::read_to_string`
/// error AND the `serde_json::from_str` error to user-facing
/// `String`s via `.map_err(|e| format!(...))?`. A `.unwrap()` on
/// either would panic the launcher on a corrupt registry file,
/// leaving the user no path but to delete the registry and lose
/// every mod row. The mapped error surfaces through the UI instead.
#[test]
fn registry_load_maps_serde_err_to_user_facing_string() {
    let body = registry_src();
    let fn_pos = body
        .find("pub fn load(path: &Path) -> Result<Self, String>")
        .expect("Registry::load must exist");
    let rest = &body[fn_pos..];
    let end = rest.find("\n    }\n").unwrap_or(rest.len().min(1200));
    let fn_body = &rest[..end];
    assert!(
        fn_body.contains("fs::read_to_string(path)") && fn_body.contains(".map_err(|e|"),
        "PRD 3.2.2 (iter 194): Registry::load must use \
         `fs::read_to_string(path).map_err(|e| format!(...))?` — a \
         raw `.unwrap()` panics the launcher on IO errors.\n\
         Body:\n{fn_body}"
    );
    assert!(
        fn_body.contains("serde_json::from_str(&body)"),
        "PRD 3.2.2 (iter 194): Registry::load must parse via \
         `serde_json::from_str(&body)`.\nBody:\n{fn_body}"
    );
    assert!(
        fn_body.contains("is corrupted"),
        "PRD 3.2.2 (iter 194): Registry::load must map serde \
         errors to a user-facing string containing `is corrupted` \
         so the UI can show a specific reason.\nBody:\n{fn_body}"
    );
    // Reject raw .unwrap() patterns in the body.
    assert!(
        !fn_body.contains("fs::read_to_string(path).unwrap()"),
        "PRD 3.2.2 (iter 194): Registry::load must not \
         `.unwrap()` on fs::read_to_string — panics the launcher."
    );
    assert!(
        !fn_body.contains("serde_json::from_str(&body).unwrap()"),
        "PRD 3.2.2 (iter 194): Registry::load must not \
         `.unwrap()` on serde_json::from_str — panics the launcher."
    );
}

/// Iter 194: `download_and_extract` must call `fs::create_dir_all`
/// BEFORE `extract_zip(`. Without the create, the first install of
/// a given mod fails with ENOENT — dest_dir doesn't exist yet on
/// fresh systems. Order: `remove_dir_all` (cleanup) < `create_dir_all`
/// (re-create) < `extract_zip` (populate).
#[test]
fn download_and_extract_create_dir_all_precedes_extract_zip() {
    let src = fs::read_to_string(EXTERNAL_APP_RS).expect("external_app.rs must exist");
    let fn_pos = src
        .find("pub async fn download_and_extract")
        .or_else(|| src.find("async fn download_and_extract"))
        .expect("download_and_extract must exist");
    let window = &src[fn_pos..fn_pos.saturating_add(3000)];
    let create_idx = window
        .find("fs::create_dir_all(dest_dir)")
        .expect("download_and_extract must call fs::create_dir_all(dest_dir)");
    let extract_idx = window
        .find("extract_zip(")
        .expect("extract_zip call must exist");
    assert!(
        create_idx < extract_idx,
        "PRD 3.2.2 (iter 194): fs::create_dir_all(dest_dir) (at \
         offset {create_idx}) must come BEFORE extract_zip (at \
         offset {extract_idx}). Otherwise first-install of a given \
         mod fails with ENOENT because dest_dir hasn't been \
         created yet."
    );
}

/// Iter 194: `download_and_extract` must call
/// `revert_partial_install_dir(dest_dir)` on `extract_zip` failure.
/// PRD 3.2.8 `disk-full-revert`: if extraction partially succeeds
/// then errors (classic trigger: ENOSPC mid-zip), half the files
/// are on disk. Without the revert, the next Play attempt tries to
/// spawn an executable missing its dependent DLLs. The revert pairs
/// with the `if let Err(e) = extract_zip` match arm.
#[test]
fn download_and_extract_reverts_partial_on_extract_zip_failure() {
    let src = fs::read_to_string(EXTERNAL_APP_RS).expect("external_app.rs must exist");
    let fn_pos = src
        .find("pub async fn download_and_extract")
        .or_else(|| src.find("async fn download_and_extract"))
        .expect("download_and_extract must exist");
    let window = &src[fn_pos..fn_pos.saturating_add(3000)];
    assert!(
        window.contains("if let Err(e) = extract_zip(&bytes, dest_dir)")
            || window.contains("if let Err(e) = extract_zip("),
        "PRD 3.2.8 (iter 194): download_and_extract must wrap \
         `extract_zip(...)` in an `if let Err(e) = ...` match arm so \
         the partial-install can be cleaned up on failure.\n\
         Window:\n{window}"
    );
    assert!(
        window.contains("revert_partial_install_dir(dest_dir)"),
        "PRD 3.2.8 (iter 194): download_and_extract's extract_zip \
         failure arm must call \
         `revert_partial_install_dir(dest_dir)`. Without it, a \
         half-extracted zip (ENOSPC mid-write) leaves the user \
         with an un-runnable mod tree. This pairs with `if let \
         Err(e) = extract_zip`."
    );
}

/// Iter 194: `Registry::load` must treat a missing registry file
/// as "first-run" — return `Ok(Self::default())` instead of
/// erroring. A missing file is the expected initial state on a
/// fresh install; returning Err would block the launcher from
/// starting until the user creates an empty registry manually.
#[test]
fn registry_load_returns_default_on_missing_path() {
    let body = registry_src();
    let fn_pos = body
        .find("pub fn load(path: &Path) -> Result<Self, String>")
        .expect("Registry::load must exist");
    let window = &body[fn_pos..fn_pos.saturating_add(400)];
    assert!(
        window.contains("if !path.exists()"),
        "PRD 3.2.2 (iter 194): Registry::load must carry \
         `if !path.exists()` as the first branch — fresh installs \
         must not fail to load a registry that doesn't exist yet."
    );
    assert!(
        window.contains("return Ok(Self::default());"),
        "PRD 3.2.2 (iter 194): Registry::load's missing-path branch \
         must `return Ok(Self::default())`. Returning Err blocks \
         launcher startup on a fresh install."
    );
    // Ordering: the !exists check must come before any fs::read.
    let not_exists_idx = window.find("if !path.exists()").unwrap();
    let read_idx = window
        .find("fs::read_to_string(path)")
        .unwrap_or(window.len());
    assert!(
        not_exists_idx < read_idx,
        "PRD 3.2.2 (iter 194): the `!path.exists()` check must \
         come BEFORE `fs::read_to_string(path)` — otherwise the \
         missing file still hits the read and errors out."
    );
}
