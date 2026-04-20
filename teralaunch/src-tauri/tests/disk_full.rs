//! PRD 3.2.8.disk-full-revert — integration-level pin.
//!
//! Bin-crate limitation: can't import `revert_partial_install_dir` /
//! `revert_partial_install_file` directly. The behavioural tests live in
//! `src/services/mods/external_app.rs::tests::{revert_on_enospc,
//! revert_partial_gpk_file_removes_it}`. This file pins the external
//! contract — when extract/write fails, next retry must see a clean
//! dest — by modelling the reversal logic against `std::fs` and asserting
//! the same state transitions.
//!
//! Rule: after a partial install fails,
//!   - dest dir path is absent (never partially populated for the user)
//!   - dest file path is absent (never zero-byte or truncated)
//!
//! No matter what state disk was in when the failure hit.

use std::fs;
use tempfile::TempDir;

fn revert_dir_model(dest: &std::path::Path) {
    if dest.exists() {
        let _ = fs::remove_dir_all(dest);
    }
}

fn revert_file_model(dest: &std::path::Path) {
    if dest.exists() {
        let _ = fs::remove_file(dest);
    }
}

/// PRD 3.2.8 acceptance: partial writes reversed on ENOSPC. The extract
/// path went through and wrote 3 files before the error; after revert,
/// the dest dir is gone so the user's next retry starts clean.
#[test]
fn revert_on_enospc() {
    let tmp = TempDir::new().unwrap();
    let dest = tmp.path().join("mod_root");

    fs::create_dir_all(&dest).unwrap();
    fs::create_dir_all(dest.join("bin")).unwrap();
    fs::write(dest.join("app.exe"), b"partial").unwrap();
    fs::write(dest.join("bin").join("plugin.dll"), b"also partial").unwrap();
    fs::write(dest.join("bin").join("helper.dll"), b"third file").unwrap();

    revert_dir_model(&dest);

    assert!(!dest.exists(), "dest must be gone after revert");
}

/// Symmetric: a partial GPK file is cleaned up so the mapper patcher on
/// retry doesn't see a truncated footer.
#[test]
fn revert_partial_gpk_file() {
    let tmp = TempDir::new().unwrap();
    let dest = tmp.path().join("classicplus.minimap.gpk");

    fs::write(&dest, b"truncated GPK bytes").unwrap();
    assert!(dest.exists());

    revert_file_model(&dest);

    assert!(!dest.exists(), "partial file must be gone after revert");
}

/// Reverting a missing path is a safe no-op — covers the "failure before
/// dest was created" branch (connection refused, DNS failure before any
/// bytes arrived).
#[test]
fn revert_missing_path_is_noop() {
    let tmp = TempDir::new().unwrap();

    let dir = tmp.path().join("never_created_dir");
    revert_dir_model(&dir);
    assert!(!dir.exists());

    let file = tmp.path().join("never_created.gpk");
    revert_file_model(&file);
    assert!(!file.exists());
}

/// Idempotency: calling revert twice is safe. If retry logic re-enters
/// the cleanup path (e.g. because it runs after every Err branch), that
/// must not panic or leave stale state.
#[test]
fn revert_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let dest = tmp.path().join("mod_root");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("file"), b"x").unwrap();

    revert_dir_model(&dest);
    revert_dir_model(&dest);

    assert!(!dest.exists());
}

// --------------------------------------------------------------------
// Iter 165 structural pins — revert helper signatures + call-site
// ordering in download_and_extract / download_file.
// --------------------------------------------------------------------
//
// The tests above prove the cleanup MODEL is correct. These pins
// defend the PRODUCTION wiring: the two revert helpers must stay
// best-effort (no Result return that could mask the primary error),
// they must use recursive fs::remove_dir_all (non-recursive would
// fail on any populated extract), and the call sites must invoke
// revert BEFORE the Err return — order flip = dead cleanup.

const EXTERNAL_APP_RS: &str = "src/services/mods/external_app.rs";

fn external_app_src() -> String {
    // Normalize CRLF -> LF so fn-body extractors that search for `\n}\n`
    // work correctly on Windows checkouts (iter 234: caught this when
    // new pins saw 1200-char fallback windows instead of fn-scoped bodies).
    fs::read_to_string(EXTERNAL_APP_RS)
        .unwrap_or_else(|e| panic!("{EXTERNAL_APP_RS} must be readable: {e}"))
        .replace("\r\n", "\n")
}

/// `revert_partial_install_dir` must be best-effort: it returns `()` (no
/// Result). A refactor to `-> Result<(), String>` would let the caller
/// propagate cleanup errors, which would mask the PRIMARY error (the
/// ENOSPC / extract failure that triggered the revert). The user would
/// see "Failed to clean up partial install" instead of "Disk full".
#[test]
fn revert_dir_signature_is_unit_returning_best_effort() {
    let src = external_app_src();
    assert!(
        src.contains("pub(crate) fn revert_partial_install_dir(dest_dir: &Path) {"),
        "PRD 3.2.8: revert_partial_install_dir must keep the \
         `pub(crate) fn ... (dest_dir: &Path) {{` (unit return) \
         signature. A Result return would propagate cleanup failures \
         and mask the primary ENOSPC/extract error."
    );
}

/// `revert_partial_install_file` shares the same best-effort invariant
/// for the single-file GPK path.
#[test]
fn revert_file_signature_is_unit_returning_best_effort() {
    let src = external_app_src();
    assert!(
        src.contains("pub(crate) fn revert_partial_install_file(dest_file: &Path) {"),
        "PRD 3.2.8: revert_partial_install_file must keep the \
         `pub(crate) fn ... (dest_file: &Path) {{` (unit return) \
         signature. A Result return would mask the primary fs::write \
         error behind a cleanup failure."
    );
}

/// `revert_partial_install_dir` must call `fs::remove_dir_all`, not
/// `fs::remove_dir`. The non-recursive variant fails with "directory
/// not empty" on any non-trivial extract — which is exactly the case
/// revert is trying to clean up (3 files already extracted before
/// ENOSPC). The warn-log would fire every time and the user would
/// have a lingering mod_root on retry.
#[test]
fn revert_dir_uses_recursive_remove_dir_all() {
    let src = external_app_src();
    let fn_pos = src
        .find("pub(crate) fn revert_partial_install_dir")
        .expect("revert_partial_install_dir must exist");
    let rest = &src[fn_pos..];
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(800));
    let body = &rest[..end];
    assert!(
        body.contains("fs::remove_dir_all(dest_dir)"),
        "PRD 3.2.8: revert_partial_install_dir must call \
         `fs::remove_dir_all(dest_dir)`. `fs::remove_dir` is \
         non-recursive and fails on any populated extract — the \
         cleanup branch the revert exists to handle.\n\
         Body:\n{body}"
    );
    // And must NOT call the non-recursive variant (this catches an
    // accidental typo where someone drops the `_all`).
    assert!(
        !body.contains("fs::remove_dir(dest_dir)"),
        "PRD 3.2.8: revert_partial_install_dir must NOT call \
         `fs::remove_dir` (non-recursive); only `fs::remove_dir_all`."
    );
}

/// In `download_and_extract`, the revert call must precede the
/// `return Err(e)` on the extract-failure path. A refactor that
/// returns before reverting makes the cleanup dead code — the
/// function exits and the partial extract stays on disk.
#[test]
fn revert_dir_runs_before_err_return_in_download_and_extract() {
    let src = external_app_src();
    let fn_pos = src
        .find("pub async fn download_and_extract")
        .expect("download_and_extract must exist");
    let rest = &src[fn_pos..];
    // The extract-Err branch is well within 3000 chars of the fn head.
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(3000));
    let body = &rest[..end];

    // Find the `if let Err(e) = extract_zip(...)` block.
    let err_block_pos = body
        .find("if let Err(e) = extract_zip(")
        .expect("download_and_extract must guard `if let Err(e) = extract_zip(...)`");
    let revert_pos = body[err_block_pos..]
        .find("revert_partial_install_dir(dest_dir)")
        .map(|p| err_block_pos + p)
        .expect(
            "download_and_extract must call revert_partial_install_dir \
             inside the extract-Err branch",
        );
    let return_pos = body[err_block_pos..]
        .find("return Err(e);")
        .map(|p| err_block_pos + p)
        .expect("download_and_extract must return Err(e) after revert");
    assert!(
        revert_pos < return_pos,
        "PRD 3.2.8: revert_partial_install_dir(dest_dir) must be \
         called BEFORE `return Err(e);`. A `return` before the \
         revert makes cleanup dead code — partial extract stays on \
         disk.\nBody window:\n{body}"
    );
}

/// In `download_file`, the revert call must precede the `return Err(...)`
/// on the fs::write-failure path. Same ordering invariant as
/// download_and_extract — a return-before-revert leaves a truncated GPK
/// that the mapper patcher will later choke on (crafted-footer attack
/// surface, caught by bogus_gpk_footer's parse pins).
#[test]
fn revert_file_runs_before_err_return_in_download_file() {
    let src = external_app_src();
    let fn_pos = src
        .find("pub async fn download_file")
        .expect("download_file must exist");
    let rest = &src[fn_pos..];
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(3000));
    let body = &rest[..end];

    let err_block_pos = body
        .find("if let Err(e) = fs::write(dest_file")
        .expect("download_file must guard `if let Err(e) = fs::write(dest_file, ...)`");
    let revert_pos = body[err_block_pos..]
        .find("revert_partial_install_file(dest_file)")
        .map(|p| err_block_pos + p)
        .expect(
            "download_file must call revert_partial_install_file \
             inside the fs::write-Err branch",
        );
    let return_pos = body[err_block_pos..]
        .find("return Err(")
        .map(|p| err_block_pos + p)
        .expect("download_file must return Err after revert");
    assert!(
        revert_pos < return_pos,
        "PRD 3.2.8: revert_partial_install_file(dest_file) must be \
         called BEFORE the `return Err(...)`. A return-before-revert \
         leaves a truncated GPK that the next retry's mapper patcher \
         would try to parse.\nBody window:\n{body}"
    );
}

// --------------------------------------------------------------------
// Iter 196 structural pins — guard traceability + no-panic revert +
// missing-file short-circuit + pub(crate) visibility + &Path sig.
// --------------------------------------------------------------------

const GUARD_SOURCE: &str = "tests/disk_full.rs";

fn external_app_fn_body<'a>(src: &'a str, sig: &str) -> &'a str {
    let fn_pos = src
        .find(sig)
        .unwrap_or_else(|| panic!("{sig} must exist in external_app.rs"));
    let rest = &src[fn_pos..];
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(1200));
    &rest[..end]
}

/// Iter 196: guard source header must cite `PRD 3.2.8` + the fix-
/// plan slot `disk-full-revert` so the criterion and P-slot are
/// reachable via grep.
#[test]
fn guard_file_header_cites_prd_and_disk_full_revert() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    let header = &body[..body.len().min(2000)];
    assert!(
        header.contains("PRD 3.2.8"),
        "PRD 3.2.8 (iter 196): {GUARD_SOURCE} header must cite \
         `PRD 3.2.8` so the criterion is reachable via grep."
    );
    assert!(
        header.contains("disk-full-revert"),
        "PRD 3.2.8 (iter 196): {GUARD_SOURCE} header must cite \
         `disk-full-revert` so the fix-plan P-slot is reachable \
         via grep."
    );
}

/// Iter 196: `revert_partial_install_dir` must log a `log::warn!`
/// on the `fs::remove_dir_all` Err arm rather than `.unwrap()` /
/// `.expect()` / `panic!`. The revert runs during error recovery;
/// a panic inside the cleanup path would mask the primary error
/// (ENOSPC, extract failure) with a process crash and leave the
/// user with no useful signal.
#[test]
fn revert_dir_logs_warn_on_failure_not_panic() {
    let src = external_app_src();
    let body = external_app_fn_body(&src, "pub(crate) fn revert_partial_install_dir");
    assert!(
        body.contains("log::warn!"),
        "PRD 3.2.8 (iter 196): revert_partial_install_dir must call \
         `log::warn!` on the fs::remove_dir_all Err arm. Without \
         the warn, a cleanup failure is silent — the next retry may \
         hit a stale partial install without any signal.\n\
         Body:\n{body}"
    );
    for forbidden in [".unwrap()", ".expect(", "panic!("] {
        assert!(
            !body.contains(forbidden),
            "PRD 3.2.8 (iter 196): revert_partial_install_dir must \
             not contain `{forbidden}` — a panic during cleanup \
             masks the primary ENOSPC/extract error with a process \
             crash."
        );
    }
}

/// Iter 196: `revert_partial_install_file` must short-circuit when
/// the dest_file doesn't exist (`if !dest_file.exists() { return; }`).
/// Without the check, calling revert on a path that was never
/// created (download failed before the write) fires an ENOENT
/// warn every time — the logs fill with false-positive noise.
#[test]
fn revert_file_short_circuits_on_missing_file() {
    let src = external_app_src();
    let body = external_app_fn_body(&src, "pub(crate) fn revert_partial_install_file");
    assert!(
        body.contains("if !dest_file.exists()"),
        "PRD 3.2.8 (iter 196): revert_partial_install_file must \
         guard `if !dest_file.exists()` before fs::remove_file. \
         Without it, reverting a never-created file logs ENOENT \
         warn noise on every download-before-write failure.\n\
         Body:\n{body}"
    );
    let exists_idx = body.find("if !dest_file.exists()").unwrap();
    let remove_idx = body
        .find("fs::remove_file(dest_file)")
        .expect("fs::remove_file must still be called");
    assert!(
        exists_idx < remove_idx,
        "PRD 3.2.8 (iter 196): the `!dest_file.exists()` check must \
         precede `fs::remove_file`. Ordering matters: the short-\
         circuit return only suppresses the ENOENT warn if it runs \
         first."
    );
}

/// Iter 196: both revert helpers must be `pub(crate)`, never `pub`.
/// `pub(crate)` keeps the functions reachable from sibling modules
/// (commands/mods.rs) without leaking into the external API
/// surface. Widening to `pub` would let downstream consumers (if
/// this crate ever became a lib) call cleanup directly — a
/// confusing API given the best-effort semantics.
#[test]
fn revert_helpers_stay_pub_crate_not_public_api() {
    let src = external_app_src();
    // Negative pin: reject a bare `pub fn revert_partial_install_dir`
    // or `pub fn revert_partial_install_file`.
    assert!(
        !src.contains("pub fn revert_partial_install_dir(")
            && !src.contains("pub fn revert_partial_install_file("),
        "PRD 3.2.8 (iter 196): revert helpers must stay \
         `pub(crate)`, never bare `pub`. The best-effort semantics \
         are internal-call-site aware; widening to `pub` lets \
         downstream code call cleanup in a confused order."
    );
}

/// Iter 196: both revert helper signatures must accept `&Path`,
/// not `PathBuf`. `&Path` is zero-alloc; `PathBuf` would force the
/// install/download call sites to clone before passing. The call
/// sites already have a `&Path` in scope; a PathBuf param would be
/// a friction spike for no gain.
#[test]
fn revert_helpers_take_path_ref_not_pathbuf_by_value() {
    let src = external_app_src();
    assert!(
        src.contains("pub(crate) fn revert_partial_install_dir(dest_dir: &Path)"),
        "PRD 3.2.8 (iter 196): revert_partial_install_dir must keep \
         `dest_dir: &Path` signature. A `PathBuf` by-value param \
         forces callers to clone; every call site already has a \
         `&Path` in scope."
    );
    assert!(
        src.contains("pub(crate) fn revert_partial_install_file(dest_file: &Path)"),
        "PRD 3.2.8 (iter 196): revert_partial_install_file must \
         keep `dest_file: &Path` signature. Same rationale as \
         revert_partial_install_dir."
    );
}

// --------------------------------------------------------------------
// Iter 234 structural pins — GUARD_SOURCE canonical, helper
// co-location, no tokio::fs creep, no cross-call coupling between
// the two revert helpers, and original-error preservation in
// download_file's Err arm.
//
// Iter-156 + iter-170-ish + iter-196 covered signatures + ordering +
// visibility + warn-not-panic + short-circuit. These five extend to
// the structural invariants a confident refactor could still miss:
// a constant drift (header-inspection silently reads wrong path), a
// helper moved to a utility module (call-site discoverability
// breaks), a tokio::fs migration (async-cleanup semantics diverge
// from best-effort), cross-calling helpers (couples two install
// paths into one failure class), and an error-message rewrite that
// hides ENOSPC signals from operators.
// --------------------------------------------------------------------

/// Iter 234: `GUARD_SOURCE` constant must stay canonical. Every
/// header-inspection pin in this guard reads through it; a rename
/// of `disk_full.rs` would panic at test time with "guard source
/// must exist" — misrouting triage toward a missing file instead
/// of a constant drift.
#[test]
fn guard_source_constant_is_canonical() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    assert!(
        body.contains(r#"const GUARD_SOURCE: &str = "tests/disk_full.rs";"#),
        "PRD 3.2.8 (iter 234): {GUARD_SOURCE} must keep \
         `const GUARD_SOURCE: &str = \"tests/disk_full.rs\";` \
         verbatim. A rename of the test file without updating the \
         constant produces `file not found` panics that misroute \
         triage."
    );
}

/// Iter 234: both revert helpers must live in
/// `src/services/mods/external_app.rs`. A refactor that moved them
/// to `src/utils/revert.rs` or `src/services/mods/revert.rs` would
/// leave every source-inspection pin in this guard reading the
/// wrong file. The helpers are tightly coupled to the download /
/// extract call sites (they undo the exact IO those fns perform)
/// and belong where the IO lives.
#[test]
fn revert_helpers_co_located_with_install_call_sites() {
    let src = external_app_src();
    assert!(
        src.contains("pub(crate) fn revert_partial_install_dir("),
        "PRD 3.2.8 (iter 234): revert_partial_install_dir must be \
         defined in src/services/mods/external_app.rs. A move to \
         utils/revert.rs couples the download path to a remote \
         file — every future reviewer looking at download_and_extract \
         has to cross-reference."
    );
    assert!(
        src.contains("pub(crate) fn revert_partial_install_file("),
        "PRD 3.2.8 (iter 234): revert_partial_install_file must be \
         defined in src/services/mods/external_app.rs."
    );
}

/// Iter 234: revert helpers must use sync `std::fs`, NOT `tokio::fs`.
/// The helpers run from the error path of async fns, but they must
/// NOT themselves be async: a `tokio::fs::remove_dir_all` returns a
/// Future that must be awaited, adding an .await point inside the
/// error recovery path. An .await there means cancellation / panic
/// during cleanup can leave the dest half-cleaned. Best-effort sync
/// cleanup is atomic at the OS level.
#[test]
fn revert_helpers_use_sync_fs_not_tokio_fs() {
    let src = external_app_src();
    let dir_body = external_app_fn_body(&src, "pub(crate) fn revert_partial_install_dir");
    let file_body = external_app_fn_body(&src, "pub(crate) fn revert_partial_install_file");
    for (name, body) in [("revert_partial_install_dir", dir_body), ("revert_partial_install_file", file_body)] {
        assert!(
            !body.contains("tokio::fs"),
            "PRD 3.2.8 (iter 234): {name} must NOT use `tokio::fs::...`. \
             An async cleanup introduces an .await inside the error-\
             recovery path; cancellation there leaves the dest \
             half-cleaned. Best-effort sync cleanup is atomic at the \
             OS level.\nBody:\n{body}"
        );
        assert!(
            !body.contains(".await"),
            "PRD 3.2.8 (iter 234): {name} must NOT contain `.await`. \
             Same reason: async cleanup is not best-effort."
        );
    }
}

/// Iter 234: the two revert helpers must not call each other. Each
/// undoes exactly one kind of install (external dir vs GPK single
/// file); coupling them into a shared helper or cross-calling would
/// mean a failure during GPK revert could spuriously touch an
/// external-mod slot (or vice versa).
#[test]
fn revert_helpers_do_not_cross_call() {
    let src = external_app_src();
    let dir_body = external_app_fn_body(&src, "pub(crate) fn revert_partial_install_dir");
    let file_body = external_app_fn_body(&src, "pub(crate) fn revert_partial_install_file");
    assert!(
        !dir_body.contains("revert_partial_install_file"),
        "PRD 3.2.8 (iter 234): revert_partial_install_dir must NOT \
         call revert_partial_install_file. The two helpers are \
         single-responsibility — crossing them couples external-mod \
         and GPK error paths into a shared failure class.\nBody:\n{dir_body}"
    );
    assert!(
        !file_body.contains("revert_partial_install_dir"),
        "PRD 3.2.8 (iter 234): revert_partial_install_file must NOT \
         call revert_partial_install_dir. Same reason.\nBody:\n{file_body}"
    );
}

/// Iter 234: the `return Err(...)` in `download_file`'s
/// fs::write-error branch must forward the original IO error as
/// part of the message (via the `{}` / `{e}` formatter). A rewrite
/// to a generic `"download failed"` string hides the underlying
/// cause — operators reading the user's support-bundle log can't
/// distinguish ENOSPC from permission-denied from a truncated
/// download. The fix is load-bearing on incident response.
#[test]
fn download_file_err_arm_preserves_original_io_error() {
    let src = external_app_src();
    let fn_pos = src
        .find("pub async fn download_file")
        .expect("download_file must exist");
    let rest = &src[fn_pos..];
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(3000));
    let body = &rest[..end];
    // Find the fs::write-error branch.
    let err_idx = body
        .find("if let Err(e) = fs::write(dest_file")
        .expect("download_file must guard fs::write with `if let Err(e)`");
    let err_block = &body[err_idx..body.len().min(err_idx + 600)];
    // The return Err(...) inside the branch must forward the original
    // `e` into the returned message — via `{e}` interpolation, `{}`
    // positional with `e` as argument, or `e.to_string()`. A bare
    // string without `e` hides ENOSPC from operators.
    let has_e_interpolated = err_block.contains("{e}");
    let has_e_to_string = err_block.contains("e.to_string()");
    // Positional: `format!("... {} ...", e)` — look for `, e)` or `, e,`
    // near a format! call.
    let has_e_positional = err_block.contains(", e)") || err_block.contains(", e,");
    assert!(
        has_e_interpolated || has_e_to_string || has_e_positional,
        "PRD 3.2.8 (iter 234): download_file's fs::write-Err branch \
         must forward the original `e` into the returned Err \
         message (via `{{e}}` interpolation, `e.to_string()`, or \
         `format!(\"... {{}} ...\", e)` positional). A generic \
         message hides ENOSPC from operators reading support \
         bundles.\nBranch:\n{err_block}"
    );
}
