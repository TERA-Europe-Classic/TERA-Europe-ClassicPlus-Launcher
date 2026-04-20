//! PRD 5.3.adv.tampered-catalog — wiring guard.
//!
//! The behavioural half of "catalog entry with wrong SHA returns Err
//! plus 0 bytes on disk" is pinned inline in `external_app.rs`
//! (`sha_mismatch_aborts_before_write`, `sha_mismatch_aborts_before_write_gpk`,
//! `sha_match_writes_file`). Those prove the downloader fails closed.
//!
//! The registry half — "and the registry row ends up as Error" — is
//! three source wires deep: (1) `download_and_extract` / `download_file`
//! return Err with the text "hash mismatch" when the SHA doesn't match;
//! (2) `install_external_mod` / `install_gpk_mod` route that Err through
//! `finalize_error` (not swallowed, not rethrown bare); (3) `finalize_error`
//! flips `status = ModStatus::Error`, clears `progress`, and stashes the
//! message in `last_error`.
//!
//! A refactor that breaks any one of those links (e.g. swallows the
//! error, bypasses `finalize_error`, renames the status enum variant)
//! leaves the registry stuck in Installing forever until the boot-
//! recovery path (`recover_stuck_installs`) eventually flips it to
//! Error — but by then the user sees a spinner that never resolves.
//!
//! This file source-inspects the three links so any refactor that
//! breaks the chain trips a test. Complements the behavioural SHA
//! tests in `external_app.rs` — together they cover end-to-end.

use std::fs;

const EXTERNAL_APP_RS: &str = "src/services/mods/external_app.rs";
const COMMANDS_MODS_RS: &str = "src/commands/mods.rs";
const TYPES_RS: &str = "src/services/mods/types.rs";
const GUARD_SOURCE: &str = "tests/tampered_catalog.rs";

fn read_file(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|_| panic!("{path} must exist"))
}

/// Link 1 — both downloader entry points must surface SHA mismatch as an
/// Err containing "hash mismatch" (the stable user-facing message that
/// `finalize_error` stashes into `last_error`).
#[test]
fn downloader_surfaces_hash_mismatch_error_text() {
    let src = read_file(EXTERNAL_APP_RS);

    // download_and_extract path (external zip mods).
    let and_extract_pos = src
        .find("pub async fn download_and_extract")
        .or_else(|| src.find("async fn download_and_extract"))
        .expect("download_and_extract must exist");
    let and_extract_win = &src[and_extract_pos..and_extract_pos.saturating_add(2000)];
    assert!(
        and_extract_win.contains("Sha256::digest"),
        "download_and_extract must compute Sha256::digest of downloaded bytes"
    );
    assert!(
        and_extract_win.contains("hash mismatch") || and_extract_win.contains("Hash mismatch"),
        "download_and_extract must surface SHA mismatch as \"hash mismatch\" \
         error text — this wording is pinned so finalize_error's \
         last_error field carries a stable message the UI can match on"
    );

    // download_file path (GPK / single-file mods).
    let file_pos = src
        .find("pub async fn download_file")
        .or_else(|| src.find("async fn download_file"))
        .expect("download_file must exist");
    let file_win = &src[file_pos..file_pos.saturating_add(2000)];
    assert!(
        file_win.contains("Sha256::digest"),
        "download_file must compute Sha256::digest of downloaded bytes"
    );
    assert!(
        file_win.contains("hash mismatch") || file_win.contains("Hash mismatch"),
        "download_file must surface SHA mismatch as \"hash mismatch\" \
         error text — same rationale as download_and_extract"
    );
}

/// Link 2a — `install_external_mod`'s Err branch must route through
/// `finalize_error`. Without this, a SHA mismatch from the downloader
/// would propagate as a bare Err and leave the registry row stuck at
/// Installing until the next boot's recover_stuck_installs pass.
#[test]
fn install_external_mod_routes_err_through_finalize_error() {
    let src = read_file(COMMANDS_MODS_RS);
    let fn_pos = src
        .find("async fn install_external_mod")
        .expect("install_external_mod must exist");
    // install_external_mod is ~100 lines (download + progress + match
    // + finalize) — take 6000 chars to cover the body comfortably.
    let window = &src[fn_pos..fn_pos.saturating_add(6000)];

    assert!(
        window.contains("download_and_extract"),
        "install_external_mod must call download_and_extract"
    );
    // The match arm handling the Err must funnel through finalize_error.
    // We look for the Err arm pattern + finalize_error call inside the
    // function body window.
    assert!(
        window.contains("Err(err) => finalize_error")
            || window.contains("Err(err) => { finalize_error")
            || (window.contains("Err(err)") && window.contains("finalize_error")),
        "install_external_mod's Err branch must call finalize_error — \
         a bare `return Err(...)` would leave the registry stuck at \
         Installing. See PRD §5.3 adv.tampered-catalog."
    );
}

/// Link 2b — same wire, GPK path.
#[test]
fn install_gpk_mod_routes_err_through_finalize_error() {
    let src = read_file(COMMANDS_MODS_RS);
    let fn_pos = src
        .find("async fn install_gpk_mod")
        .expect("install_gpk_mod must exist");
    // install_gpk_mod is ~80 lines; take up to 6000 bytes safely.
    // `get` avoids panicking if the end falls inside a multi-byte char.
    let end = fn_pos.saturating_add(6000).min(src.len());
    let window = src.get(fn_pos..end).unwrap_or(&src[fn_pos..]);

    assert!(
        window.contains("download_file"),
        "install_gpk_mod must call download_file"
    );
    assert!(
        window.contains("Err(err) => finalize_error")
            || window.contains("Err(err) => { finalize_error")
            || (window.contains("Err(err)") && window.contains("finalize_error")),
        "install_gpk_mod's Err branch must call finalize_error — \
         same rationale as install_external_mod"
    );
}

/// Link 3 — `finalize_error` must flip the three registry fields that
/// make the row renderable as an error state in the UI:
///   - status → ModStatus::Error
///   - progress → None (cleared so the bar doesn't linger)
///   - last_error → Some(err) (populated so the UI has a reason)
///
/// If any of these three fields were ever dropped, the registry row
/// would render inconsistently (stale progress, missing reason, etc.)
/// even though the install clearly failed.
#[test]
fn finalize_error_flips_status_progress_and_last_error() {
    let src = read_file(COMMANDS_MODS_RS);
    let fn_pos = src
        .find("fn finalize_error")
        .expect("finalize_error must exist");
    let window = &src[fn_pos..fn_pos.saturating_add(1500)];

    assert!(
        window.contains("status = ModStatus::Error"),
        "finalize_error must set slot.status = ModStatus::Error — \
         otherwise the registry row stays Installing after a failed \
         install"
    );
    assert!(
        window.contains("progress = None"),
        "finalize_error must clear slot.progress — otherwise the UI \
         renders a half-full bar under an error state"
    );
    assert!(
        window.contains("last_error = Some("),
        "finalize_error must populate slot.last_error — otherwise the \
         UI has no reason string to show the user"
    );
}

/// Link 4 (iter 149) — fail-closed invariant: SHA verification must
/// run BEFORE any filesystem write in the downloader paths. A
/// refactor that reorders write-before-verify would let a tampered
/// catalog entry land bytes on disk even when the mismatch Err is
/// later returned; the behavioural SHA tests would still pass
/// (Err is Err), but a partial file would linger.
#[test]
fn downloaders_verify_sha_before_fs_write() {
    let src = read_file(EXTERNAL_APP_RS);

    // download_file: simpler path, single-file write.
    let file_pos = src
        .find("pub async fn download_file")
        .expect("download_file must exist");
    let file_end = src[file_pos..]
        .find("\n}\n")
        .map(|i| file_pos + i)
        .unwrap_or(file_pos + 2000);
    let file_body = &src[file_pos..file_end];
    let sha_idx = file_body
        .find("Sha256::digest")
        .expect("download_file must call Sha256::digest");
    let write_idx = file_body
        .find("fs::write")
        .expect("download_file must call fs::write");
    assert!(
        sha_idx < write_idx,
        "download_file must verify SHA BEFORE fs::write — otherwise \
         a tampered download lands bytes on disk even when the \
         mismatch Err is returned. sha_idx={sha_idx}, \
         write_idx={write_idx}."
    );
}

/// Link 5 (iter 149) — `finalize_error`'s signature must take the
/// error message as a `String` so `last_error` gets a real reason
/// stashed. If the signature drops the param (e.g. only takes an
/// id), `last_error` would default to a generic "unknown" and the
/// UI can't show the user why their install failed.
#[test]
fn finalize_error_signature_takes_error_string() {
    let src = read_file(COMMANDS_MODS_RS);
    let fn_pos = src
        .find("fn finalize_error")
        .expect("finalize_error must exist");
    // Signature spans the line(s) until the `{` opening the body.
    let body_open = src[fn_pos..]
        .find('{')
        .map(|i| fn_pos + i)
        .expect("finalize_error body must open with {");
    let sig = &src[fn_pos..body_open];
    // Must accept something shaped like `err: String` or `msg:
    // String`. A bare `fn finalize_error(id: &str)` signature
    // loses the reason.
    assert!(
        sig.contains(": String") || sig.contains(": &str"),
        "finalize_error signature must accept the error message as \
         a String / &str param so `last_error` can stash it. \
         Current signature: {sig}"
    );
    assert!(
        sig.contains("id"),
        "finalize_error signature must accept an id so the specific \
         registry row is flipped. Current signature: {sig}"
    );
}

/// Link 6 (iter 149) — `ModStatus::Error` variant must exist in the
/// types module. `finalize_error` assigns it by name; removing or
/// renaming the variant breaks the whole error-surfacing chain at
/// compile time — but catching it at test time gives a clearer
/// error message than a build break.
#[test]
fn mod_status_error_variant_exists() {
    let types_body = read_file("src/services/mods/types.rs");
    assert!(
        types_body.contains("Error") && types_body.contains("ModStatus"),
        "types.rs must carry the ModStatus enum with an Error \
         variant — finalize_error routes through ModStatus::Error \
         to render the row as failed."
    );
    // The variant must be spelled `Error` (case-sensitive). A
    // hypothetical `Errored` or `Failed` rename would miss \
    // finalize_error's assignment. Grep for the pattern.
    assert!(
        types_body.contains("Error,")
            || types_body.contains("Error\n")
            || types_body.contains("Error "),
        "types.rs ModStatus must have the exact variant name \
         `Error`. finalize_error assigns by name so a rename to \
         `Errored` or `Failed` would break silently at test time."
    );
}

/// Self-test — prove the detectors bite on known-bad shapes. Without
/// this, a regressed detector would silently pass the real tests.
#[test]
fn tampered_catalog_wiring_detector_self_test() {
    // Bad shape A: install fn that swallows the Err instead of routing
    // to the registry-flipping helper. (Kept deliberately verbatim-free
    // of the real helper name so the detector's positive-match assertion
    // below isn't self-satisfied by our own comment text.)
    let swallowed = "async fn install_external_mod() {
        let r = download_and_extract().await;
        match r {
            Ok(_) => {}
            Err(e) => return Err(e),
        }
    }";
    let swallowed_calls_helper = swallowed.contains("finalize_error");
    assert!(
        !swallowed_calls_helper,
        "self-test: detector must flag install fn that bypasses the \
         registry-flipping helper"
    );

    // Bad shape B: registry-flipping helper that clears the reason
    // instead of setting it (so the user sees no explanation for the
    // failure). We assert the positive-match form the real detector
    // uses (`last_error = Some(`) is NOT present.
    let missing_reason = "fn foo(id: &str, err: String) {
        if let Some(slot) = reg.find_mut(id) {
            slot.status = ModStatus::Error;
            slot.progress = None;
            slot.last_error = None;
        }
    }";
    assert!(
        !missing_reason.contains("last_error = Some("),
        "self-test: detector must flag helper that drops the reason"
    );

    // Bad shape C: downloader that returns Err without the stable
    // error text — UI / last_error matcher would see different wording.
    let wrong_text = "async fn download_file() {
        if actual != expected {
            return Err(\"checksum problem\".into());
        }
    }";
    assert!(
        !wrong_text.contains("hash mismatch"),
        "self-test: detector must flag downloader that changes the \
         stable error text"
    );

    // Bad shape D (iter 149): downloader that writes before SHA check.
    let write_first = "async fn download_file() {
        let bytes = fetch().await?;
        fs::write(dest, &bytes)?;
        let actual = Sha256::digest(&bytes);
        if actual != expected {
            return Err(\"hash mismatch\".into());
        }
    }";
    let sha_idx = write_first.find("Sha256::digest").unwrap();
    let write_idx = write_first.find("fs::write").unwrap();
    assert!(
        write_idx < sha_idx,
        "self-test: write-first fixture must have write before sha"
    );

    // Bad shape E: finalize_error signature that drops the reason
    // string param.
    let bare_sig = "fn finalize_error(id: &str) {";
    assert!(
        !(bare_sig.contains(": String") || bare_sig.contains(": &str ")
            || bare_sig.contains(": String,")),
        "self-test: bare id-only signature must be flagged"
    );

    // Bad shape F: ModStatus without Error variant (a rename to
    // Failed would be the canonical regression).
    let renamed_enum =
        "pub enum ModStatus {\n    Installed,\n    Installing,\n    Failed,\n}\n";
    assert!(
        !renamed_enum.contains("Error,") && !renamed_enum.contains("Error\n"),
        "self-test: enum without Error variant must be flagged"
    );

    // Iter 189 — additional bad shapes.

    // Bad shape G: downloader using md5 instead of sha256.
    let weak_hash = "use md5::Digest; let actual = md5::compute(&bytes);";
    assert!(
        !weak_hash.contains("sha2::") && !weak_hash.contains("Sha256"),
        "self-test: downgrade to weaker hash must be flagged"
    );

    // Bad shape H: finalize_error without a window.emit() call.
    let no_emit = "fn finalize_error(id: &str, err: String) {\n  slot.status = ModStatus::Error;\n}\n";
    assert!(
        !no_emit.contains("window.emit"),
        "self-test: finalize_error without window.emit must be flagged"
    );

    // Bad shape I: finalize_error called without the err propagation.
    let dropped_err = r#"Err(_) => finalize_error(&entry.id, String::new(), &window),"#;
    assert!(
        dropped_err.contains("String::new()"),
        "self-test: err dropped via String::new() must be flagged"
    );
}

/// Iter 189: guard file header must cite `PRD 5.3` + `adv.tampered-
/// catalog` so the fix-plan P-slot and PRD criterion are reachable
/// via grep. Without the citation, a maintainer unfamiliar with the
/// wiring chain might relax a link to match a "simpler" call shape
/// and silently break the error-surfacing path.
#[test]
fn guard_file_header_cites_prd_and_adv_slot() {
    let body = read_file(GUARD_SOURCE);
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 5.3"),
        "tampered-catalog (iter 189): {GUARD_SOURCE} header must \
         cite `PRD 5.3` so the criterion is reachable via grep."
    );
    assert!(
        header.contains("adv.tampered-catalog"),
        "tampered-catalog (iter 189): {GUARD_SOURCE} header must \
         cite `adv.tampered-catalog` so the fix-plan P-slot is \
         reachable via grep."
    );
}

/// Iter 189: `ModStatus` enum must carry the canonical set of
/// variants observed at iter 149. A rename (`Error` → `Failed`) is
/// pinned elsewhere; this widens the check to the full set so a
/// surprise addition (`Pending`, `Uninstalling`) or deletion
/// (`UpdateAvailable`) surfaces here with a clear failure message.
#[test]
fn mod_status_enum_carries_canonical_variant_set() {
    let types_body = read_file(TYPES_RS);
    for variant in [
        "NotInstalled",
        "Disabled",
        "Running",
        "Starting",
        "Enabled",
        "UpdateAvailable",
        "Error",
    ] {
        assert!(
            types_body.contains(variant),
            "tampered-catalog (iter 189): {TYPES_RS} ModStatus must \
             carry the `{variant}` variant. Every caller sites to a \
             specific variant by name; deletion or rename breaks \
             the error-surfacing chain silently in unrelated code."
        );
    }
}

/// Iter 189: `external_app.rs` must import SHA-256 from the `sha2`
/// crate. A switch to a weaker hash (md5, sha1) would reduce the
/// cost of a tampered-catalog preimage attack to feasible. The
/// hash mismatch check would still Err — but a deliberate collider
/// could now pass verification.
#[test]
fn external_app_uses_sha256_digest_from_sha2_crate() {
    let src = read_file(EXTERNAL_APP_RS);
    assert!(
        src.contains("use sha2::") && src.contains("Sha256"),
        "tampered-catalog (iter 189): {EXTERNAL_APP_RS} must import \
         from `sha2::` crate and reference `Sha256`. A switch to a \
         weaker hash (md5, sha1) would make a tampered-catalog \
         preimage attack feasible."
    );
    // And explicitly reject weaker hash crates.
    for weak in ["use md5::", "use sha1::", "use md4::"] {
        assert!(
            !src.contains(weak),
            "tampered-catalog (iter 189): {EXTERNAL_APP_RS} must \
             not import `{weak}` — those hashes have feasible \
             collision attacks relative to the catalog-tampering \
             threat model."
        );
    }
}

/// Iter 189: both install functions must pass the propagated `err`
/// as finalize_error's second arg. An `Err(err) => finalize_error(
/// &entry.id, String::new(), &window)` shape would flip the registry
/// but lose the reason — `last_error` ends up empty and the UI
/// can't explain the failure.
#[test]
fn install_funcs_pass_propagated_err_to_finalize_error() {
    let src = read_file(COMMANDS_MODS_RS);
    // The canonical call shape appears at least twice (external + gpk paths).
    let canonical_call_count = src
        .matches("finalize_error(&entry.id, err, &window)")
        .count();
    assert!(
        canonical_call_count >= 2,
        "tampered-catalog (iter 189): {COMMANDS_MODS_RS} must carry \
         at least 2 call sites of `finalize_error(&entry.id, err, \
         &window)` — one per install path (external + gpk). Found \
         {canonical_call_count}. A shape change that drops `err` \
         (e.g. replaces with `String::new()` / `\"error\".into()`) \
         would lose the reason and leave `last_error` empty."
    );
    // Also reject any call that passes a literal empty string.
    assert!(
        !src.contains("finalize_error(&entry.id, String::new()"),
        "tampered-catalog (iter 189): {COMMANDS_MODS_RS} must not \
         call finalize_error with `String::new()` as the reason — \
         that loses the propagated error text."
    );
}

/// Iter 226: the four path constants (EXTERNAL_APP_RS / COMMANDS_MODS_RS
/// / TYPES_RS / GUARD_SOURCE) must be pinned verbatim. A rename of
/// any target file (external_app.rs → downloader.rs, types.rs →
/// model.rs) would surface only as a generic "must exist" panic at
/// `read_file` time. Pinning the canonical literal gives the diff a
/// loud anchor and makes the rename a conscious update.
#[test]
fn guard_path_constants_are_canonical() {
    let src = fs::read_to_string("tests/tampered_catalog.rs")
        .expect("tests/tampered_catalog.rs must exist");
    for line in [
        r#"const EXTERNAL_APP_RS: &str = "src/services/mods/external_app.rs";"#,
        r#"const COMMANDS_MODS_RS: &str = "src/commands/mods.rs";"#,
        r#"const TYPES_RS: &str = "src/services/mods/types.rs";"#,
        r#"const GUARD_SOURCE: &str = "tests/tampered_catalog.rs";"#,
    ] {
        assert!(
            src.contains(line),
            "canonical path constant missing: `{line}`. A rename of \
             any of these targets must surface as a guard update, not \
             a generic must-exist panic."
        );
    }
}

/// Iter 226: the detector self-test must carry all nine shape labels
/// A-I plus the `Iter 189 — additional bad shapes.` divider comment.
/// Shapes A-F predate iter 189 (swallowed-Err / missing-reason /
/// wrong-text / write-first / bare-sig / renamed-enum); G-I were
/// added iter 189 (md5-weak-hash / no-window-emit / err-dropped-via-
/// String::new). If any label is dropped by a cleanup pass, the
/// detector stops biting on that class of synthetic regression
/// silently.
#[test]
fn detector_self_test_covers_all_nine_era_shapes() {
    let src = fs::read_to_string("tests/tampered_catalog.rs")
        .expect("tests/tampered_catalog.rs must exist")
        .replace("\r\n", "\n");
    let fn_pos = src
        .find("fn tampered_catalog_wiring_detector_self_test()")
        .expect("self-test fn must exist");
    let body_start = src[fn_pos..]
        .find('{')
        .map(|o| fn_pos + o)
        .expect("self-test body open brace");
    let body_end = src[body_start..]
        .find("\n}\n")
        .map(|o| body_start + o)
        .expect("self-test body close brace");
    let body = &src[body_start..body_end];
    for marker in [
        "Bad shape A",
        "Bad shape B",
        "Bad shape C",
        "Bad shape D",
        "Bad shape E",
        "Bad shape F",
        "Iter 189 — additional bad shapes",
        "Bad shape G",
        "Bad shape H",
        "Bad shape I",
    ] {
        assert!(
            body.contains(marker),
            "self-test must carry era marker `{marker}` — dropping it \
             silently stops the detector from biting on that class of \
             synthetic regression."
        );
    }
}

/// Iter 226: `fn finalize_error`, `async fn install_external_mod`, and
/// `async fn install_gpk_mod` must each appear exactly once in
/// commands/mods.rs. Sibling tests use `src.find("fn …")` to compute a
/// body window and read source around the match; a second declaration
/// (e.g. a shadowed helper moved out but not deleted) would silently
/// push the window onto the wrong body.
#[test]
fn finalize_error_and_install_fns_are_structurally_unique() {
    let src = read_file(COMMANDS_MODS_RS);
    for needle in [
        "fn finalize_error",
        "async fn install_external_mod",
        "async fn install_gpk_mod",
    ] {
        let count = src.matches(needle).count();
        assert_eq!(
            count, 1,
            "{COMMANDS_MODS_RS} must carry exactly one `{needle}` \
             declaration. Found {count}. A duplicate would let \
             sibling tests compute their body window against the \
             wrong copy and silently pass."
        );
    }
}

/// Iter 226: the guard file's module header must enumerate all three
/// wiring links by their exact fn names. If a maintenance edit
/// shortens the header to "the registry surfacing chain" without
/// listing the functions, a reader loses the map of what the guard
/// actually protects and the call-site tests below become
/// unmotivated cargo.
#[test]
fn guard_header_enumerates_three_wiring_links_by_fn_name() {
    let src = fs::read_to_string("tests/tampered_catalog.rs")
        .expect("tests/tampered_catalog.rs must exist");
    let header_end = src.find("use std::fs;").expect("file must `use std::fs;`");
    let header = &src[..header_end];
    for needle in [
        "download_and_extract",
        "download_file",
        "install_external_mod",
        "install_gpk_mod",
        "finalize_error",
        "ModStatus::Error",
    ] {
        assert!(
            header.contains(needle),
            "tampered_catalog.rs header must name `{needle}` — the \
             header is the map of which link each pin protects; \
             dropping a name orphans the corresponding test."
        );
    }
}

/// Iter 226: the Tauri `install_mod` command must dispatch by
/// `ModKind` to both `install_external_mod` AND `install_gpk_mod` via
/// a `match entry.kind` expression. A short-circuit that routes only
/// one kind would leave the other installer (and its whole registry-
/// flipping chain) dead code at runtime — the behavioural SHA tests
/// would still pass (they call the helper directly), but the
/// tampered-catalog user flow for the unrouted kind would never
/// exercise the error surfacing path.
#[test]
fn install_mod_dispatches_to_both_installers_via_modkind_match() {
    let src = read_file(COMMANDS_MODS_RS);
    let fn_pos = src
        .find("pub async fn install_mod")
        .expect("install_mod must exist and be pub");
    let window = &src[fn_pos..fn_pos.saturating_add(800)];
    assert!(
        window.contains("match entry.kind"),
        "install_mod must use `match entry.kind` to dispatch — a \
         single-branch call leaves the other kind's installer dead \
         code at runtime."
    );
    for arm in [
        "ModKind::External => install_external_mod",
        "ModKind::Gpk => install_gpk_mod",
    ] {
        assert!(
            window.contains(arm),
            "install_mod dispatch must carry `{arm}` arm. Dropping \
             either kind's arm would leave the other installer's \
             error-surfacing chain unexercised at runtime."
        );
    }
}

/// Iter 189: `finalize_error` must emit a `mod_download_progress`
/// event with `state: "error"` to the window. Without this, the
/// registry row flips to Error but the UI (which re-renders on the
/// event) never sees the change until the next catalog-refresh
/// cycle — user sees a spinner that never resolves.
#[test]
fn finalize_error_emits_state_error_event_to_window() {
    let src = read_file(COMMANDS_MODS_RS);
    let fn_pos = src
        .find("fn finalize_error")
        .expect("finalize_error must exist");
    let window = &src[fn_pos..fn_pos.saturating_add(1500)];
    assert!(
        window.contains("window.emit"),
        "tampered-catalog (iter 189): finalize_error must call \
         `window.emit(...)` so the UI sees the status flip. Without \
         the emit, the registry row is Error but the UI still shows \
         Installing until the next refresh."
    );
    assert!(
        window.contains("mod_download_progress"),
        "tampered-catalog (iter 189): finalize_error's emit must use \
         the canonical event name `mod_download_progress` — the \
         frontend listens on that specific channel."
    );
    assert!(
        window.contains(r#""state": "error""#)
            || window.contains(r#""state":"error""#),
        "tampered-catalog (iter 189): finalize_error's emit payload \
         must include `\"state\": \"error\"` — the frontend branches \
         on `state` to decide how to render the row."
    );
}

// --------------------------------------------------------------------
// Iter 267 structural pins — 3 source bounds + guard bounds + adv-
// slot cite.
// --------------------------------------------------------------------

/// Iter 267: external_app.rs byte bounds.
#[test]
fn external_app_rs_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 3000;
    const MAX_BYTES: usize = 200_000;
    let bytes = fs::metadata(EXTERNAL_APP_RS)
        .expect("external_app.rs must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "tampered-catalog (iter 267): {EXTERNAL_APP_RS} is {bytes} \
         bytes; expected [{MIN_BYTES}, {MAX_BYTES}]. A gutting drops \
         the hash-mismatch path entirely."
    );
}

/// Iter 267: commands/mods.rs byte bounds.
#[test]
fn commands_mods_rs_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 3000;
    const MAX_BYTES: usize = 200_000;
    let bytes = fs::metadata(COMMANDS_MODS_RS)
        .expect("commands/mods.rs must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "tampered-catalog (iter 267): {COMMANDS_MODS_RS} is {bytes} \
         bytes; expected [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

/// Iter 267: types.rs byte bounds.
#[test]
fn types_rs_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 1000;
    const MAX_BYTES: usize = 80_000;
    let bytes = fs::metadata(TYPES_RS)
        .expect("types.rs must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "tampered-catalog (iter 267): {TYPES_RS} is {bytes} bytes; \
         expected [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

/// Iter 267: guard byte bounds.
#[test]
fn guard_source_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 5000;
    const MAX_BYTES: usize = 80_000;
    let bytes = fs::metadata(GUARD_SOURCE)
        .expect("guard must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "tampered-catalog (iter 267): guard is {bytes} bytes; expected \
         [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

/// Iter 267: guard header must cite `adv.tampered-catalog` for slot-
/// grep discoverability.
#[test]
fn guard_source_cites_adv_tampered_catalog_slot() {
    let body = fs::read_to_string(GUARD_SOURCE)
        .expect("guard must exist");
    let header = &body[..body.len().min(500)];
    assert!(
        header.contains("adv.tampered-catalog") || header.contains("tampered-catalog"),
        "tampered-catalog (iter 267): guard header must cite \
         `adv.tampered-catalog` or `tampered-catalog` for slot-grep \
         discoverability.\nHeader:\n{header}"
    );
}
