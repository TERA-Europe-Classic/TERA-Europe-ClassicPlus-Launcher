//! adv.bogus-gpk-footer — wiring guard for the TMM parser's
//! "no magic footer" rejection path.
//!
//! The behavioural corpus lives in
//! `src/services/mods/tmm.rs::tests::parse_mod_file_rejects_non_tmm_gpks`
//! (8 adversarial fixtures — empty / too-small / wrong-magic / magic-only /
//! misplaced-magic / corrupt-footer / small-non-magic / long-junk).
//!
//! That test pins the behaviour. This file is the structural guard that
//! (a) the test stays in the source across refactors, (b) the
//! parse_mod_file magic-check branch is still present, and (c)
//! install_gpk still rejects a ModFile with an empty container.
//!
//! Why two layers: the behavioural test could silently become trivial if
//! someone simplifies `parse_mod_file` to always return Ok(Default) — the
//! test would still "pass" but the adversarial invariant would be gone.
//! The branch-presence pin catches that.

use std::fs;

const TMM_RS: &str = "src/services/mods/tmm.rs";

fn read_tmm_rs() -> String {
    fs::read_to_string(TMM_RS).expect("services/mods/tmm.rs must exist")
}

/// The named adversarial corpus test must stay present in tmm.rs. This
/// is what PRD §5.3 adv.bogus-gpk-footer is stamped against; deleting
/// the test would silently erase the proof that non-TMM bytes are
/// handled without panic.
#[test]
fn adversarial_corpus_test_stays_in_tmm_rs() {
    let src = read_tmm_rs();
    assert!(
        src.contains("fn parse_mod_file_rejects_non_tmm_gpks"),
        "tmm.rs must retain the parse_mod_file_rejects_non_tmm_gpks test — \
         it's the behavioural pin for PRD §5.3 adv.bogus-gpk-footer"
    );
    // The test must still exercise parse_mod_file directly (not some
    // wrapper that could hide a panic behind a catch_unwind).
    let pos = src
        .find("fn parse_mod_file_rejects_non_tmm_gpks")
        .expect("test fn header must exist");
    let window = &src[pos..pos.saturating_add(4000)];
    assert!(
        window.contains("parse_mod_file("),
        "the adversarial corpus test must call parse_mod_file directly"
    );
}

/// `parse_mod_file` must keep its magic-check fallback branch. If
/// `if magic != PACKAGE_MAGIC` disappears, the parser would try to
/// read footer slots off any 4+-byte buffer and either panic or return
/// meaningless data — breaking the "bogus bytes return cleanly" invariant.
#[test]
fn parse_mod_file_retains_magic_check_fallback() {
    let src = read_tmm_rs();
    let fn_pos = src
        .find("pub fn parse_mod_file")
        .expect("tmm.rs must still define parse_mod_file");
    // Window covers the function header + the first ~60 lines of body —
    // enough to cover the magic read + check but not the rest of the
    // footer parsing.
    let window = &src[fn_pos..fn_pos.saturating_add(1500)];
    assert!(
        window.contains("if magic != PACKAGE_MAGIC"),
        "parse_mod_file must keep the `if magic != PACKAGE_MAGIC` fallback — \
         without it, non-TMM bytes would hit the footer-parsing code path \
         and could panic on out-of-bounds reads"
    );
    // The fallback must return Ok, not propagate an Err. An install_gpk
    // call downstream looks at modfile.container.is_empty() to reject —
    // if the parser starts returning Err here, error messages change in
    // subtle ways and the PRD invariant weakens.
    assert!(
        window.contains("return Ok(m);"),
        "the magic-mismatch fallback must return Ok(m) — the downstream \
         install_gpk `container.is_empty()` gate is what catches the mod"
    );
}

/// `install_gpk` must keep the empty-container Err gate. This is the
/// second half of the bogus-footer defence: parse_mod_file returns Ok
/// with empty container, install_gpk refuses to deploy. Lose this and
/// a crafted .gpk could pass through to `ensure_backup` etc. with
/// a zero-length composite that might cause downstream surprises.
#[test]
fn install_gpk_retains_empty_container_rejection() {
    let src = read_tmm_rs();
    let fn_pos = src
        .find("pub fn install_gpk")
        .expect("tmm.rs must still define install_gpk");
    // Window is generous — install_gpk's gate block is near the top but
    // after a bit of setup; 2500 chars comfortably covers the whole gate
    // cluster without spilling into the actual deploy code.
    let window = &src[fn_pos..fn_pos.saturating_add(2500)];
    assert!(
        window.contains("modfile.container.is_empty()"),
        "install_gpk must keep the `modfile.container.is_empty()` check — \
         it's the gate that catches bogus-footer .gpks parse_mod_file \
         surfaces as Ok(degenerate ModFile)"
    );
}

/// Detector self-test — prove the source-inspection logic above actually
/// bites on a known-bad shape. If a refactor silently drops a guard,
/// these asserts should start firing; but if the detectors themselves
/// regress to no-op (e.g. a typo in the substring), the real tests
/// would silently pass. A tiny synthetic source + negative check keeps
/// the detector honest.
#[test]
fn detector_self_test_rejects_source_missing_guards() {
    let bad = "pub fn install_gpk() -> Result<(), String> { Ok(()) }";
    assert!(
        !bad.contains("modfile.container.is_empty()"),
        "self-test: detector must not match on guardless stub"
    );
    let good = "pub fn install_gpk() -> Result<(), String> {\n\
                    if modfile.container.is_empty() { return Err(\"x\".into()); }\n\
                    Ok(()) }";
    assert!(
        good.contains("modfile.container.is_empty()"),
        "self-test: detector must match when the guard is present"
    );
}

// --------------------------------------------------------------------
// Iter 163 structural pins — parse bounds-checks + install_gpk's full
// gate set + parse-before-fs-touch ordering.
// --------------------------------------------------------------------
//
// The tests above pin the existence of the two most visible guards
// (magic-check fallback + empty-container rejection). These extend
// coverage to the remaining defences: the tiny-file underflow
// protection in parse_mod_file, the THREE additional fail-closed
// gates in install_gpk (unsafe filename, empty packages, missing
// object_path), and the critical ordering invariant that parsing
// + gating must complete BEFORE any filesystem touch. A crafted
// .gpk that slipped past a gate while backup / CookedPC writes
// were already in flight would leave a corrupted install tree.

/// `parse_mod_file` must keep the `end < 4` guard before reading the
/// trailing magic bytes. Without it, `end - 4` underflows on a 3-byte
/// (or shorter) input and the subsequent `read_u32_le(bytes, magic_off)`
/// reads out of bounds — either panicking or reading uninitialised
/// stack data. This is a classic small-input-attack class.
#[test]
fn parse_mod_file_guards_against_tiny_input_underflow() {
    let src = read_tmm_rs();
    let fn_pos = src
        .find("pub fn parse_mod_file")
        .expect("parse_mod_file must exist");
    // First 600 chars — the early guards cluster at the top.
    let window = &src[fn_pos..fn_pos.saturating_add(600)];
    assert!(
        window.contains("if end < 4"),
        "PRD §5.3 adv.bogus-gpk-footer: parse_mod_file must guard \
         `if end < 4` before computing `end - 4`. Without it, a 3-byte \
         (or shorter) input triggers a subtraction-underflow and the \
         magic read goes OOB.\nWindow:\n{window}"
    );
    assert!(
        window.contains("Mod file is too small to contain metadata"),
        "PRD §5.3: the too-small branch must carry the \
         `Mod file is too small to contain metadata` error message — \
         the behavioural test `parse_mod_file_rejects_non_tmm_gpks` \
         pins this string via its `empty` fixture."
    );
}

/// The internal `read_back_i32` closure must guard `if *p < 4` before
/// subtracting 4 from the cursor. Without it, a truncated footer (magic
/// present but not enough bytes for the metadata slots) would underflow
/// the usize cursor and OOB-read. This is the second line of defence
/// for footers that pass the magic check but are still malformed.
#[test]
fn parse_mod_file_read_back_guards_cursor_underflow() {
    let src = read_tmm_rs();
    let fn_pos = src
        .find("pub fn parse_mod_file")
        .expect("parse_mod_file must exist");
    // Wider window to capture the closure.
    let window = &src[fn_pos..fn_pos.saturating_add(1500)];
    assert!(
        window.contains("if *p < 4"),
        "PRD §5.3 adv.bogus-gpk-footer: parse_mod_file's read_back_i32 \
         closure must guard `if *p < 4` before `*p -= 4`. Without it, \
         a truncated footer underflows the cursor and reads OOB."
    );
    assert!(
        window.contains("Unexpected EOF while reading mod footer"),
        "PRD §5.3: the read-back EOF branch must carry the exact \
         `Unexpected EOF while reading mod footer` error so truncated \
         footers surface cleanly rather than panicking."
    );
}

/// `install_gpk` runs FOUR fail-closed gates before touching the
/// filesystem. Only one is currently pinned
/// (`install_gpk_retains_empty_container_rejection`). The other three
/// are equally load-bearing — losing any one lets a crafted .gpk
/// slip past rejection and hit `ensure_backup` / `fs::copy` with a
/// degenerate ModFile.
#[test]
fn install_gpk_has_four_fail_closed_gates() {
    let src = read_tmm_rs().replace("\r\n", "\n");
    let fn_pos = src
        .find("pub fn install_gpk")
        .expect("install_gpk must exist");
    // The four gates all live in the first ~1000 chars of the body
    // (before ensure_backup).
    let window = &src[fn_pos..fn_pos.saturating_add(1000)];

    // Gate 1 (already pinned in an earlier test) — empty container.
    assert!(
        window.contains("modfile.container.is_empty()"),
        "PRD §3.1.4 gpk-deploy-sandbox gate 1: \
         `modfile.container.is_empty()` must be present"
    );
    // Gate 2 — container-name sandbox (path-traversal guard).
    assert!(
        window.contains("is_safe_gpk_container_filename(&modfile.container)"),
        "PRD §3.1.4 gpk-deploy-sandbox gate 2: \
         `is_safe_gpk_container_filename(&modfile.container)` must be \
         present. Without it, a crafted container name like \
         `../../../Windows/System32/evil.exe` escapes CookedPC."
    );
    // Gate 3 — no composite packages to override.
    assert!(
        window.contains("modfile.packages.is_empty()"),
        "PRD §3.1.4 gpk-deploy-sandbox gate 3: \
         `modfile.packages.is_empty()` must be present. A .gpk with \
         an empty packages list has nothing to override — without \
         this gate, install_gpk would happily copy the file into \
         CookedPC and never patch the mapper."
    );
    // Gate 4 — at least one package has an empty object_path.
    assert!(
        window.contains("modfile.packages.iter().any(|p| p.object_path.is_empty())"),
        "PRD §3.1.4 gpk-deploy-sandbox gate 4: \
         `modfile.packages.iter().any(|p| p.object_path.is_empty())` \
         must be present. A package with no object path can't be \
         mapped to a slot — installing it would corrupt the mapper."
    );
}

/// The critical ordering invariant: `parse_mod_file` + all four gates
/// must complete BEFORE `ensure_backup(game_root)?`. If any gate
/// fires AFTER the backup is touched, a rejected .gpk can still
/// corrupt the vanilla `.clean` backup — which breaks §3.2
/// clean-recovery because the backup no longer reflects unmodified
/// vanilla bytes.
#[test]
fn install_gpk_parses_and_gates_before_filesystem_touch() {
    let src = read_tmm_rs();
    let fn_pos = src
        .find("pub fn install_gpk")
        .expect("install_gpk must exist");
    let rest = &src[fn_pos..];
    let end = rest
        .find("\n}\n")
        .unwrap_or(rest.len().min(3000));
    let body = &rest[..end];

    let parse_pos = body
        .find("parse_mod_file(&gpk_bytes)?")
        .expect("install_gpk must call `parse_mod_file(&gpk_bytes)?`");
    let backup_pos = body
        .find("ensure_backup(game_root)?")
        .expect("install_gpk must call `ensure_backup(game_root)?`");
    // Each of the four gates must appear BEFORE ensure_backup.
    for gate in [
        "modfile.container.is_empty()",
        "is_safe_gpk_container_filename(&modfile.container)",
        "modfile.packages.is_empty()",
        "modfile.packages.iter().any(|p| p.object_path.is_empty())",
    ] {
        let gate_pos = body.find(gate).unwrap_or_else(|| {
            panic!("install_gpk must carry the `{gate}` gate")
        });
        assert!(
            gate_pos < backup_pos,
            "PRD §3.1.4: gate `{gate}` must fire BEFORE \
             `ensure_backup(game_root)?`. A gate that runs after the \
             backup touch lets a rejected .gpk still corrupt the \
             `.clean` baseline — breaking §3.2 clean-recovery."
        );
    }
    assert!(
        parse_pos < backup_pos,
        "PRD §3.1.4: `parse_mod_file` must complete before \
         `ensure_backup` — otherwise a parse error leaves a partial \
         backup touch on disk."
    );
}

/// `is_safe_gpk_container_filename` must exist as a
/// `pub(crate)`-visible helper. Tests + the install_gpk call site both
/// depend on reaching it; inlining the check into install_gpk would
/// make future parse call sites (e.g. add_mod_from_file which does
/// its own parse + sandbox check) forget to repeat the guard.
#[test]
fn is_safe_gpk_container_filename_is_pub_crate_helper() {
    let src = read_tmm_rs();
    assert!(
        src.contains("pub(crate) fn is_safe_gpk_container_filename(name: &str) -> bool"),
        "PRD §3.1.4: tmm.rs must export \
         `pub(crate) fn is_safe_gpk_container_filename(name: &str) -> bool`. \
         Inlining the check into install_gpk makes other call sites \
         (add_mod_from_file, preview paths) forget to repeat the \
         sandbox guard."
    );
}

// --------------------------------------------------------------------
// Iter 203 structural pins — PACKAGE_MAGIC constant value +
// sandbox-predicate rejection-branch inventory + uninstall_gpk
// defence-in-depth + binary-safe read + corpus small-fixture retention.
// --------------------------------------------------------------------
//
// The pins above cover parse_mod_file's two underflow guards,
// install_gpk's four fail-closed gates + ordering, and the
// `is_safe_gpk_container_filename` helper's visibility. They do NOT
// pin: (a) the magic constant's literal value — a silent rename to a
// different u32 would leave every test "passing" while reading
// attacker-controlled .gpks as if they were TMM; (b) each rejection
// branch inside `is_safe_gpk_container_filename` — if any single
// branch regresses (e.g. `name.contains("..")` deletion), a hostile
// mod can slip through the sandbox; (c) `uninstall_gpk`'s symmetric
// sandbox call — a stored-but-hostile container could be replayed
// into a path-traversal uninstall otherwise; (d) install_gpk's use
// of `fs::read` (binary-safe) vs `fs::read_to_string` (would fail
// on any byte > 0x7F in the footer); (e) the adversarial corpus's
// specific empty-buffer + 3-byte fixtures, which are the ONLY
// inputs that exercise the `end < 4` / empty-slice branches in
// parse_mod_file.

/// `PACKAGE_MAGIC` must remain `0x9E2A83C1`. A silent rename to a
/// different u32 would break every sanity-check on legit .gpks (all
/// would hit the fallback) AND would leave the magic-mismatch assertion
/// in the adversarial corpus test vacuously satisfied — the
/// `parse_mod_file_rejects_non_tmm_gpks` corpus would still "pass"
/// because no fixture would match the new magic.
#[test]
fn package_magic_constant_has_canonical_value() {
    let src = read_tmm_rs();
    assert!(
        src.contains("const PACKAGE_MAGIC: u32 = 0x9E2A83C1;"),
        "PRD §5.3 adv.bogus-gpk-footer: tmm.rs must retain \
         `const PACKAGE_MAGIC: u32 = 0x9E2A83C1;` verbatim. A rename \
         (e.g. to `0xDEADBEEF`) would silently change which bytes are \
         treated as 'legitimate TMM footer' and nothing in the test \
         corpus would catch it — the fixtures are all non-matching \
         either way."
    );
}

/// `is_safe_gpk_container_filename` must retain ALL SEVEN rejection
/// branches. If any single branch regresses to unconditional accept,
/// a crafted .gpk can slip through one specific vector. Each branch
/// maps to a distinct adversary class — they are not redundant.
#[test]
fn sandbox_predicate_retains_all_rejection_branches() {
    let src = read_tmm_rs();
    let fn_pos = src
        .find("pub(crate) fn is_safe_gpk_container_filename(name: &str) -> bool")
        .expect("is_safe_gpk_container_filename must exist");
    // Predicate body is compact — 800 chars comfortably covers it.
    let window = &src[fn_pos..fn_pos.saturating_add(800)];

    for (branch, rationale) in [
        ("name.is_empty()", "empty string must return false"),
        ("name.contains('/')", "forward-slash separator (POSIX traversal)"),
        ("name.contains('\\\\')", "backslash separator (Windows traversal)"),
        ("name.contains('\\0')", "null byte (interior-NUL filename attacks)"),
        ("name == \".\"", "current-directory shorthand"),
        ("name == \"..\"", "parent-directory shorthand"),
        ("name.contains(\"..\")", "embedded .. (e.g. `foo..bar` on buggy normalisers)"),
    ] {
        assert!(
            window.contains(branch),
            "PRD §3.1.4 gpk-deploy-sandbox: \
             is_safe_gpk_container_filename must keep `{branch}` branch \
             ({rationale}). Dropping it would let a crafted TMM \
             container escape CookedPC through that specific vector.\n\
             Window:\n{window}"
        );
    }

    // Drive-letter check is structured differently (bytes[1] == b':').
    assert!(
        window.contains("bytes[1] == b':'"),
        "PRD §3.1.4: drive-letter rejection branch `bytes[1] == b':'` \
         must remain — otherwise `C:evil.gpk` silently passes on Windows."
    );
}

/// `uninstall_gpk` must call `is_safe_gpk_container_filename` on the
/// incoming container argument. Defence-in-depth: even though install
/// rejected the hostile container, a stale registry row or a tampered
/// `registry.json` could still drive an uninstall with a malicious
/// container — without this gate, `game_root.join(COOKED_PC_DIR)
/// .join(container)` would resolve a traversal through Path::join.
#[test]
fn uninstall_gpk_calls_sandbox_predicate_first() {
    let src = read_tmm_rs();
    let fn_pos = src
        .find("pub fn uninstall_gpk(")
        .expect("uninstall_gpk must exist");
    let window = &src[fn_pos..fn_pos.saturating_add(600)];
    assert!(
        window.contains("is_safe_gpk_container_filename(container)"),
        "PRD §3.1.4: uninstall_gpk must call \
         `is_safe_gpk_container_filename(container)` before any \
         filesystem touch. Install-side rejection is not enough — a \
         tampered registry.json could drive uninstall with a hostile \
         container that slipped in through a rollback.\n\
         Window:\n{window}"
    );
    // The gate must fire BEFORE `backup_path(game_root)` or any `fs::`
    // call. Position check: rejection must come first.
    let gate_pos = window
        .find("is_safe_gpk_container_filename(container)")
        .expect("sandbox gate must be present");
    let backup_pos = window
        .find("backup_path(game_root)")
        .unwrap_or(usize::MAX);
    assert!(
        gate_pos < backup_pos,
        "PRD §3.1.4: sandbox gate must fire BEFORE `backup_path(game_root)` \
         — otherwise a hostile container could still touch backup state."
    );
}

/// `install_gpk` must read the source .gpk via `fs::read` (binary), not
/// `fs::read_to_string` (UTF-8 validated). TMM footers contain raw
/// little-endian u32 slots — a random 0x80+ byte is statistically
/// near-certain, which would make `read_to_string` return Err on every
/// legitimate .gpk. Equally critical: a silent switch to `read_to_string`
/// would make malformed-UTF-8 the first failure mode and hide the
/// surgical "Mod file is too small to contain metadata" / footer-parse
/// errors the adversarial corpus pins rely on.
#[test]
fn install_gpk_reads_source_as_raw_bytes() {
    let src = read_tmm_rs();
    let fn_pos = src
        .find("pub fn install_gpk")
        .expect("install_gpk must exist");
    let window = &src[fn_pos..fn_pos.saturating_add(600)];
    assert!(
        window.contains("fs::read(source_gpk)"),
        "PRD §5.3: install_gpk must call `fs::read(source_gpk)` — \
         switching to `fs::read_to_string` would fail on every real \
         .gpk (footers contain non-UTF-8 bytes) AND would change the \
         error surface so the adversarial corpus pins stop holding."
    );
    assert!(
        !window.contains("fs::read_to_string(source_gpk)"),
        "PRD §5.3: install_gpk must NOT use `fs::read_to_string` on the \
         source .gpk — binary footer bytes will trip UTF-8 validation."
    );
}

/// The adversarial corpus (parse_mod_file_rejects_non_tmm_gpks) must
/// keep the EMPTY-buffer and 3-BYTE fixtures. These two inputs are the
/// ONLY ones that exercise the `if end < 4` / empty-slice branches in
/// parse_mod_file (pinned by
/// `parse_mod_file_guards_against_tiny_input_underflow`). If the
/// corpus drops them, the underflow-guard branch could regress to a
/// no-op and no fixture would catch it.
#[test]
fn adversarial_corpus_retains_small_buffer_fixtures() {
    let src = read_tmm_rs();
    let pos = src
        .find("fn parse_mod_file_rejects_non_tmm_gpks")
        .expect("adversarial corpus test must exist");
    let window = &src[pos..pos.saturating_add(4000)];
    assert!(
        window.contains("parse_mod_file(&[])"),
        "PRD §5.3: adversarial corpus must retain the empty-buffer \
         fixture (`parse_mod_file(&[])`). It's the only fixture that \
         exercises the zero-length branch of `if end < 4`."
    );
    assert!(
        window.contains("parse_mod_file(&[0, 0, 0])"),
        "PRD §5.3: adversarial corpus must retain the 3-byte fixture \
         (`parse_mod_file(&[0, 0, 0])`). It's the only fixture that \
         exercises the nonzero-but-below-threshold branch of `if end < 4` \
         — without it, a silent regression (e.g. `if end < 2`) would \
         leave every fixture passing."
    );
}
