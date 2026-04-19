//! PRD 3.1.11.self-integrity — integration-level pin.
//!
//! Bin crates can't export modules to integration tests, so the in-module
//! tests under `src/services/self_integrity.rs::tests` own the
//! IntegrityResult-specific assertions. This file pins the external
//! behaviour on the algorithm we depend on (sha256 over arbitrary bytes)
//! so a sha2 crate bump can't break the contract under us.

use std::io::Write;

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[test]
fn detects_tampered_exe() {
    // External-level counterpart to services::self_integrity::tests::detects_tampered_exe.
    // Builds a fake "launcher binary", records its baseline hash, tampers
    // it, and asserts the hash changes. If this test ever passes when it
    // shouldn't (hash unchanged after bytes changed), the integrity check
    // in main.rs is structurally broken regardless of how the in-module
    // tests look.
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(b"release-v0.1.12-launcher.exe").unwrap();
    f.flush().unwrap();

    let baseline = {
        let bytes = std::fs::read(f.path()).unwrap();
        hex(&Sha256::digest(&bytes))
    };
    assert_eq!(baseline.len(), 64, "sha256 hex must be 64 chars");

    // Tamper.
    f.as_file_mut().write_all(b"attacker-appendix").unwrap();
    f.as_file_mut().flush().unwrap();

    let after = {
        let bytes = std::fs::read(f.path()).unwrap();
        hex(&Sha256::digest(&bytes))
    };
    assert_ne!(
        baseline, after,
        "tampered file must produce a different hash"
    );
}

#[test]
fn identical_bytes_produce_identical_hash() {
    // Positive control: sha256 is deterministic, so two separate files with
    // the same contents hash to the same value. If this ever breaks, the
    // baseline comparison in self_integrity.rs silently accepts mismatches.
    let a = Sha256::digest(b"the-same-bytes");
    let b = Sha256::digest(b"the-same-bytes");
    assert_eq!(hex(&a), hex(&b));
}

// --------------------------------------------------------------------
// Iter 153 structural pins — main.rs wiring.
// --------------------------------------------------------------------
//
// The behavioural test above proves sha256 works. These pins prove the
// self-integrity check is actually CALLED at startup, BEFORE Tauri
// sets up. A refactor that removes the call, moves it inside the Tauri
// setup callback, or swallows the Mismatch exit would leave a
// tampered exe running — but the sha256 behavioural test still passes.
//
// Source-inspection style: we can't link main.rs as a library (it's
// a bin), so we read it as text.

use std::fs;

const MAIN_RS: &str = "src/main.rs";

fn main_rs() -> String {
    fs::read_to_string(MAIN_RS).expect("src/main.rs must be readable")
}

/// The self-integrity check fn must exist and must call
/// `verify_self(expected)` inside its body.
#[test]
fn run_self_integrity_check_invokes_verify_self() {
    let body = main_rs();
    let fn_pos = body
        .find("fn run_self_integrity_check()")
        .expect("main.rs must carry `fn run_self_integrity_check()`");
    let window_end = body[fn_pos..]
        .find("\n}\n")
        .map(|i| fn_pos + i)
        .unwrap_or(fn_pos + 3000);
    let window = &body[fn_pos..window_end];
    assert!(
        window.contains("verify_self(expected)"),
        "PRD 3.1.11: run_self_integrity_check must call \
         `verify_self(expected)`. Without this call, the integrity \
         check is a no-op and a tampered exe runs unchecked."
    );
}

/// On `IntegrityResult::Mismatch`, the body must call
/// `std::process::exit(` — a graceful continue or early return would
/// let the tampered exe launch Tauri anyway.
#[test]
fn mismatch_branch_exits_process() {
    let body = main_rs();
    let fn_pos = body.find("fn run_self_integrity_check()").unwrap();
    let window = &body[fn_pos..fn_pos.saturating_add(3000)];
    // Look for the Mismatch arm + an exit() inside it (within the
    // next 400 chars of the match arm).
    let mismatch_idx = window
        .find("IntegrityResult::Mismatch")
        .expect("Mismatch match arm must exist");
    let arm_window = &window[mismatch_idx..mismatch_idx.saturating_add(400)];
    assert!(
        arm_window.contains("std::process::exit(") || arm_window.contains("process::exit("),
        "PRD 3.1.11: Mismatch branch must call `std::process::exit(...)` \
         so the tampered exe cannot proceed to Tauri setup. A graceful \
         continue would let the modified binary run. Arm window:\n{arm_window}"
    );
}

/// The sidecar filename must stay `self_hash.sha256`. A rename
/// would desync the release pipeline (which signs and ships this
/// specific filename) from the launcher (which reads it).
#[test]
fn sidecar_filename_is_self_hash_sha256() {
    let body = main_rs();
    assert!(
        body.contains("self_hash.sha256"),
        "PRD 3.1.11: main.rs must reference the `self_hash.sha256` \
         sidecar filename. A rename breaks the release pipeline ↔ \
         launcher contract — the sidecar wouldn't be found and the \
         integrity check would silently skip."
    );
}

/// The expected-hex validation must require exactly 64 chars (SHA-256
/// hex length). A shorter or longer digest slipping past validation
/// would be used as `expected` and always mismatch, which (because
/// Unreadable → warn-and-continue) would silently disable the check.
#[test]
fn sidecar_validation_requires_64_char_hex() {
    let body = main_rs();
    let fn_pos = body.find("fn run_self_integrity_check()").unwrap();
    let window = &body[fn_pos..fn_pos.saturating_add(3000)];
    assert!(
        window.contains("expected.len() != 64"),
        "PRD 3.1.11: sidecar validation must check \
         `expected.len() != 64` — any other length is not a valid \
         sha256 hex digest. Accepting other lengths would feed a \
         malformed baseline to verify_self."
    );
    // The char-class check must also be present so non-hex content
    // (e.g. base64) is rejected.
    assert!(
        window.contains("is_ascii_hexdigit"),
        "PRD 3.1.11: sidecar validation must call \
         `is_ascii_hexdigit()` so non-hex content is rejected. Got \
         windowed body:\n...{}...",
        &window[..200.min(window.len())]
    );
}

/// `run_self_integrity_check()` must be called from `main()` BEFORE
/// `tauri::Builder::default()`. If the call moves to inside the
/// Tauri setup callback (which runs AFTER the window creates), the
/// check becomes advisory rather than blocking — a tampered binary
/// gets a chance to render UI or handle IPC before the exit fires.
#[test]
fn integrity_check_called_before_tauri_builder() {
    let body = main_rs();
    let check_idx = body
        .find("run_self_integrity_check();")
        .expect("main() must call run_self_integrity_check()");
    let tauri_idx = body
        .find("tauri::Builder::default()")
        .expect("main() must construct tauri::Builder");
    assert!(
        check_idx < tauri_idx,
        "PRD 3.1.11: run_self_integrity_check() must be called \
         BEFORE tauri::Builder::default() (check_idx={check_idx}, \
         tauri_idx={tauri_idx}). Running the check after Tauri \
         setup lets a tampered binary render UI or handle IPC \
         before the process exits."
    );
}

/// The REINSTALL_PROMPT constant must be shown via the native
/// Windows dialog on mismatch — otherwise the user sees only a log
/// line they never read. `show_integrity_failure_dialog` is the
/// MessageBoxW wrapper.
#[test]
fn mismatch_branch_shows_native_dialog() {
    let body = main_rs();
    let fn_pos = body.find("fn run_self_integrity_check()").unwrap();
    let window = &body[fn_pos..fn_pos.saturating_add(3000)];
    let mismatch_idx = window.find("IntegrityResult::Mismatch").unwrap();
    let arm_window = &window[mismatch_idx..mismatch_idx.saturating_add(400)];
    assert!(
        arm_window.contains("show_integrity_failure_dialog(REINSTALL_PROMPT)"),
        "PRD 3.1.11: Mismatch branch must call \
         `show_integrity_failure_dialog(REINSTALL_PROMPT)`. A log \
         line alone is invisible to end users; the native dialog is \
         the user-visible signal that a tampered binary was \
         detected."
    );
}

// --------------------------------------------------------------------
// Iter 198 structural pins — guard traceability + non-zero exit +
// Unreadable-is-advisory + sidecar-IO fallbacks + module import
// provenance.
// --------------------------------------------------------------------

const GUARD_SOURCE: &str = "tests/self_integrity.rs";

/// Iter 198: guard source header must cite `PRD 3.1.11` so the
/// criterion is reachable via grep. Without it, a maintainer might
/// relax a pin thinking this is a generic integrity test.
#[test]
fn guard_file_header_cites_prd_3_1_11() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.1.11"),
        "PRD 3.1.11 (iter 198): {GUARD_SOURCE} header must cite \
         `PRD 3.1.11` so the self-integrity criterion is reachable \
         via grep."
    );
    assert!(
        header.contains("self-integrity"),
        "PRD 3.1.11 (iter 198): {GUARD_SOURCE} header must cite \
         `self-integrity` so the fix-plan nomenclature is \
         reachable via grep."
    );
}

/// Iter 198: the `Mismatch` branch must call
/// `std::process::exit(<non-zero>)`. A zero exit code signals
/// success to any wrapping script / orchestrator — defeating the
/// tampered-exe-detection signal. Pin a specific non-zero value
/// (`exit(2)`) so any drift to `exit(0)` trips the pin.
#[test]
fn mismatch_branch_exits_with_nonzero_code() {
    let body = main_rs();
    let fn_pos = body.find("fn run_self_integrity_check()").unwrap();
    let window = &body[fn_pos..fn_pos.saturating_add(3000)];
    let mismatch_idx = window.find("IntegrityResult::Mismatch").unwrap();
    let arm_window = &window[mismatch_idx..mismatch_idx.saturating_add(400)];
    // Must contain exit(2) or another explicitly non-zero literal.
    assert!(
        arm_window.contains("std::process::exit(2)")
            || arm_window.contains("process::exit(2)"),
        "PRD 3.1.11 (iter 198): Mismatch branch must call \
         `std::process::exit(2)`. Zero exit signals success; an \
         arbitrary non-specific integer makes the tamper-detected \
         signal ambiguous to wrappers. Arm window:\n{arm_window}"
    );
    // And must NOT be exit(0).
    assert!(
        !arm_window.contains("std::process::exit(0)")
            && !arm_window.contains("process::exit(0)"),
        "PRD 3.1.11 (iter 198): Mismatch branch must NOT call \
         `std::process::exit(0)` — zero exit signals success to \
         wrapping scripts/orchestrators, defeating the tamper-\
         detected signal."
    );
}

/// Iter 198: the `IntegrityResult::Unreadable` arm must warn and
/// CONTINUE, not exit. Unreadable means "we couldn't read the
/// launcher's own bytes" — which is a runtime condition (locked
/// file, AV quarantine in progress) not a tampering signal. A
/// hard exit here would brick legitimate launchers under transient
/// filesystem conditions.
#[test]
fn unreadable_result_arm_warns_and_continues() {
    let body = main_rs();
    let fn_pos = body.find("fn run_self_integrity_check()").unwrap();
    let window_end = body[fn_pos..]
        .find("\n}\n")
        .map(|i| fn_pos + i)
        .unwrap_or(fn_pos + 3000);
    let window = &body[fn_pos..window_end];
    let unreadable_idx = window
        .find("IntegrityResult::Unreadable")
        .expect("Unreadable arm must exist in run_self_integrity_check");
    // Take the next 200 chars as the arm window.
    let arm = &window[unreadable_idx..unreadable_idx.saturating_add(200)];
    assert!(
        arm.contains("log::warn!"),
        "PRD 3.1.11 (iter 198): Unreadable arm must call \
         `log::warn!` — this is a runtime condition (file lock, AV \
         quarantine), not a tampering signal.\nArm:\n{arm}"
    );
    assert!(
        !arm.contains("std::process::exit") && !arm.contains("process::exit"),
        "PRD 3.1.11 (iter 198): Unreadable arm must NOT call \
         `process::exit` — it would brick legitimate launchers when \
         a transient filesystem condition prevents reading the \
         exe. Match and Unreadable are both advisory; only \
         Mismatch exits."
    );
}

/// Iter 198: the three pre-verify bailouts (missing exe parent,
/// missing sidecar file, malformed sidecar content) must EACH log a
/// specific `log::warn!` and `return` early. Without early-return,
/// a missing sidecar in dev would try to read `""` as hex and
/// panic / false-mismatch — breaking every dev run of the
/// launcher.
#[test]
fn sidecar_bailouts_warn_and_return_early() {
    let body = main_rs();
    let fn_pos = body.find("fn run_self_integrity_check()").unwrap();
    let window = &body[fn_pos..fn_pos.saturating_add(3000)];
    // Each bailout must be signalled with a distinct warn message
    // fragment so logs are actionable.
    for needle in [
        "exe has no parent dir",
        "sidecar",
        "skipping",
    ] {
        assert!(
            window.contains(needle),
            "PRD 3.1.11 (iter 198): run_self_integrity_check must \
             log a warn containing `{needle}` so the bailout is \
             discoverable from logs. Without distinct messages, a \
             dev debugging a missing sidecar can't tell if the \
             check ran at all."
        );
    }
    // And the bailout branches must return (not continue into
    // verify_self with garbage input).
    let match_pos = window
        .find("match verify_self(expected)")
        .expect("verify_self must be called inside match");
    // All `return;` must appear before the match verify_self call.
    // Count returns before match_pos.
    let before_match = &window[..match_pos];
    let return_count = before_match.matches("return;").count();
    assert!(
        return_count >= 3,
        "PRD 3.1.11 (iter 198): run_self_integrity_check must have \
         at least 3 early `return;` statements before \
         `match verify_self` — one for each bailout (no parent dir, \
         missing sidecar, malformed hex). Found {return_count}. \
         Without early returns, malformed sidecars feed garbage into \
         verify_self."
    );
}

/// Iter 198: `run_self_integrity_check` must import `verify_self`
/// from `services::self_integrity` (the production module), not
/// from a local stub or an alternate module. A refactor that
/// renamed the module or pointed the import elsewhere could
/// silently swap in a no-op stub while preserving the call shape.
#[test]
fn verify_self_is_imported_from_services_module() {
    let body = main_rs();
    let fn_pos = body.find("fn run_self_integrity_check()").unwrap();
    // The `use services::self_integrity::...` import is typically at
    // the top of the fn body (per current shape).
    let window = &body[fn_pos..fn_pos.saturating_add(600)];
    assert!(
        window.contains("use services::self_integrity::"),
        "PRD 3.1.11 (iter 198): run_self_integrity_check must \
         import from `services::self_integrity::` — the production \
         module. A refactor pointing to a stub elsewhere would \
         preserve call shape but silently disable the check.\n\
         Window:\n{window}"
    );
    // All three symbols the fn depends on must be named.
    for sym in ["verify_self", "IntegrityResult", "REINSTALL_PROMPT"] {
        assert!(
            window.contains(sym),
            "PRD 3.1.11 (iter 198): run_self_integrity_check's \
             `use services::self_integrity::...` must bring `{sym}` \
             into scope."
        );
    }
}
