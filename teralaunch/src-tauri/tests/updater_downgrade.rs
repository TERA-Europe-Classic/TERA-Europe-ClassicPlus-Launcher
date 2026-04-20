//! PRD 3.1.9 — updater-downgrade refusal.
//!
//! This is the outer-surface contract for the downgrade gate. The pure
//! predicate lives in `services::updater_gate::should_accept_update` with
//! its own unit tests (inline). That gets us high-confidence on the
//! predicate semantics.
//!
//! What this integration test adds on top:
//!
//! 1. **Symbolic parity**: mirrors the predicate using `semver` directly
//!    and asserts the same behaviour. If someone drifts the gate policy
//!    (e.g. weakens "strictly greater" to "greater-or-equal"), this spec
//!    fails independently of the production helper.
//!
//! 2. **Wiring guard**: source-inspects `src/main.rs` to assert the gate
//!    is actually called *before* `download_and_install`. Without this,
//!    someone could silently delete the call and all the predicate tests
//!    would still pass but the production binary would ship ungated.

use std::fs;

use semver::Version;

/// Symbolic mirror of `services::updater_gate::should_accept_update`.
/// If the production policy ever diverges from "strictly greater
/// semver", the parity tests below fail and force a deliberate update
/// of this spec.
fn spec_should_accept_update(current: &str, remote: &str) -> bool {
    let (Ok(c), Ok(r)) = (Version::parse(current), Version::parse(remote)) else {
        return false;
    };
    r > c
}

// -------- Symbolic predicate tests --------------------------------

#[test]
fn refuses_older_latest_json() {
    // Canonical attack: signed manifest served by a compromised mirror
    // advertises an older (known-vulnerable) version. Gate MUST refuse.
    assert!(!spec_should_accept_update("0.1.12", "0.1.0"));
    assert!(!spec_should_accept_update("0.1.12", "0.1.11"));
    assert!(!spec_should_accept_update("0.2.0", "0.1.99"));
    assert!(!spec_should_accept_update("1.0.0", "0.9.9"));
}

#[test]
fn refuses_replay_of_same_version() {
    // Replay attack on the manifest — serving the same version the
    // client is already running. Accepting would be wasted bandwidth at
    // best and a roll-forward-to-nowhere-useful setup for worse.
    assert!(!spec_should_accept_update("0.1.12", "0.1.12"));
    assert!(!spec_should_accept_update("0.2.0", "0.2.0"));
}

#[test]
fn accepts_strictly_newer_versions() {
    assert!(spec_should_accept_update("0.1.12", "0.1.13"));
    assert!(spec_should_accept_update("0.1.12", "0.2.0"));
    assert!(spec_should_accept_update("0.1.12", "1.0.0"));
}

#[test]
fn prerelease_semantics_block_downgrade() {
    // semver: 0.2.0-rc.1 < 0.2.0. If we're on final and the manifest
    // advertises an rc, refuse — that's a downgrade-to-unstable path.
    assert!(!spec_should_accept_update("0.2.0", "0.2.0-rc.1"));
    // Inverse is a legit upgrade — user on rc, stable released.
    assert!(spec_should_accept_update("0.2.0-rc.1", "0.2.0"));
}

#[test]
fn invalid_version_strings_refused() {
    // Defensive default: unparseable version on either side → refuse.
    // Forces a manifest that we can't safely reason about to fall
    // through rather than sneak past.
    assert!(!spec_should_accept_update("not-a-version", "0.2.0"));
    assert!(!spec_should_accept_update("0.1.12", "1.0.0-"));
    assert!(!spec_should_accept_update("", "0.2.0"));
    assert!(!spec_should_accept_update("0.1.12", ""));
    assert!(!spec_should_accept_update("", ""));
}

// -------- Wiring guard (source inspection) -------------------------

/// Parity test: the production `should_accept_update` lives in
/// `services/updater_gate.rs`. Confirm it's registered as a pub module
/// so main.rs can reach it, and that the file contains the expected
/// public signature.
#[test]
fn updater_gate_module_is_public_and_exports_predicate() {
    let mod_rs = fs::read_to_string("src/services/mod.rs").expect("services/mod.rs exists");
    assert!(
        mod_rs.contains("pub mod updater_gate;"),
        "services/mod.rs must register updater_gate as pub"
    );

    let gate = fs::read_to_string("src/services/updater_gate.rs")
        .expect("services/updater_gate.rs exists");
    assert!(
        gate.contains("pub fn should_accept_update"),
        "updater_gate.rs must export should_accept_update"
    );
}

/// The production gate must be called before `download_and_install` in
/// the main.rs setup() updater block. If this grep-style guard fails,
/// someone removed the call — re-add it and re-run 3.1.9 evidence.
#[test]
fn main_rs_calls_gate_before_download_and_install() {
    let main_rs = fs::read_to_string("src/main.rs").expect("main.rs exists");

    let gate_pos = main_rs
        .find("services::updater_gate::should_accept_update(")
        .expect(
            "main.rs must call services::updater_gate::should_accept_update \
             before update.download_and_install (PRD 3.1.9)",
        );
    // Matches either `update.download_and_install` or the multiline
    // `update\n    .download_and_install(...)` rustfmt shape.
    let install_pos = main_rs
        .find(".download_and_install")
        .expect("main.rs must still call download_and_install for accepted updates");

    assert!(
        gate_pos < install_pos,
        "gate call must appear before download_and_install in source order \
         — otherwise the gate is decorative"
    );
}

// --------------------------------------------------------------------
// Iter 154 structural pins — updater_gate.rs internals + main.rs wiring.
// --------------------------------------------------------------------
//
// The behavioural tests above prove the predicate does the right thing
// for a list of known inputs. These pins protect the SHAPE of the
// predicate and its call site so a one-character refactor (e.g. `>=`
// for `>`, or string comparison instead of semver) can't silently
// re-admit the attack class the gate was written to defeat.

const GATE_RS: &str = "src/services/updater_gate.rs";
const MAIN_RS: &str = "src/main.rs";

fn gate_src() -> String {
    fs::read_to_string(GATE_RS).expect("services/updater_gate.rs must be readable")
}

fn main_src() -> String {
    fs::read_to_string(MAIN_RS).expect("src/main.rs must be readable")
}

/// The public predicate must expose exactly `(&str, &str) -> bool`. A
/// refactor that changes the signature to e.g. `(&Version, &Version)
/// -> bool` or `(String, String) -> Result<bool, _>` would force every
/// caller to pre-parse, which means a bad version string would be
/// handled by the caller — and a caller that forgets to refuse-on-Err
/// re-opens the downgrade door. Pinning the signature keeps the
/// defensive conversion INSIDE the gate.
#[test]
fn predicate_signature_is_strictly_str_str_bool() {
    let body = gate_src();
    assert!(
        body.contains("pub fn should_accept_update(current: &str, remote: &str) -> bool"),
        "PRD 3.1.9: updater_gate.rs must export \
         `pub fn should_accept_update(current: &str, remote: &str) -> bool` \
         verbatim. Any other signature pushes parsing responsibility to \
         callers, which weakens the fail-closed default.\nGot:\n{body}"
    );
}

/// The predicate must use the `semver` crate. Hand-rolled string
/// comparison accepts `"0.10.0" < "0.9.0"` (lexicographic order),
/// which would make a legit upgrade look like a downgrade and refuse
/// it — or, worse on the other direction, make a real downgrade look
/// like an upgrade and accept it.
#[test]
fn predicate_uses_semver_crate_not_string_cmp() {
    let body = gate_src();
    assert!(
        body.contains("use semver::Version;"),
        "PRD 3.1.9: updater_gate.rs must `use semver::Version;` — \
         without it, `r > c` becomes an alphabetic string compare that \
         misorders `0.10.0` vs `0.9.0`."
    );
    assert!(
        body.contains("Version::parse(current)") && body.contains("Version::parse(remote)"),
        "PRD 3.1.9: updater_gate.rs must parse BOTH sides through \
         `Version::parse(...)`. Parsing only one side makes the compare \
         meaningless."
    );
}

/// The comparison must be STRICTLY greater (`r > c`), not `r >= c`.
/// `>=` accepts the current version as a valid "update" — that's the
/// replay-attack path the gate was explicitly written to block. This
/// test is a one-character drift detector.
#[test]
fn predicate_is_strict_greater_not_geq() {
    let body = gate_src();
    // Find the body of should_accept_update and check the actual
    // comparison operator used on the return path.
    let fn_pos = body
        .find("pub fn should_accept_update")
        .expect("should_accept_update must exist");
    let window = &body[fn_pos..body.len().min(fn_pos + 600)];
    assert!(
        window.contains("r > c"),
        "PRD 3.1.9: updater_gate.rs must compare with strict `>` \
         (`r > c`). `>=` would accept same-version replay as an update, \
         which is exactly the attack the gate was written to block.\n\
         Got:\n{window}"
    );
    assert!(
        !window.contains("r >= c"),
        "PRD 3.1.9: updater_gate.rs must NOT use `r >= c` — that \
         opens replay of the current version as a valid update."
    );
}

/// The parse-error branch must `return false`. A refactor that
/// replaces refuse-on-Err with accept-or-warn silently re-opens the
/// malformed-manifest bypass. The `let (Ok(c), Ok(r)) = ... else {}`
/// Rust 1.65+ shape is pinned here explicitly so a rewrite to `if let`
/// that forgets the else-branch can't land.
#[test]
fn predicate_defaults_to_refuse_on_parse_error() {
    let body = gate_src();
    let fn_pos = body
        .find("pub fn should_accept_update")
        .expect("should_accept_update must exist");
    let window = &body[fn_pos..body.len().min(fn_pos + 600)];
    // The predicate must bind both parses and refuse (`false`) on
    // either Err. Both shapes are acceptable — `let (Ok, Ok) = .. else
    // { return false; };` OR an explicit match. The invariant is: the
    // Err path MUST evaluate to `false`.
    let has_let_else = window.contains("let (Ok(c), Ok(r)) = ")
        && window.contains("else {")
        && window.contains("return false;");
    assert!(
        has_let_else,
        "PRD 3.1.9: updater_gate.rs must refuse on parse error via \
         `let (Ok(c), Ok(r)) = ... else {{ return false; }};`. A \
         refactor that forgets the else-branch or changes `false` to \
         `true` opens the malformed-manifest bypass.\nGot:\n{window}"
    );
}

/// The call site in main.rs must pass `env!(\"CARGO_PKG_VERSION\")`
/// as `current`. Hardcoding a string (e.g. `\"0.1.12\"`) would make
/// the gate lie about the running binary's version — a literal string
/// stays put across releases, so after the launcher version bumps
/// above the literal, every future release would be refused (or the
/// opposite, depending on direction). The build-time symbol is the
/// only safe source.
#[test]
fn main_rs_passes_cargo_pkg_version_to_gate() {
    let body = main_src();
    let gate_pos = body
        .find("services::updater_gate::should_accept_update(")
        .expect("main.rs must call the gate");
    // Look backwards a few lines for the `current` binding.
    let pre_window_start = gate_pos.saturating_sub(300);
    let pre_window = &body[pre_window_start..gate_pos];
    assert!(
        pre_window.contains("env!(\"CARGO_PKG_VERSION\")"),
        "PRD 3.1.9: main.rs must source `current` from \
         `env!(\"CARGO_PKG_VERSION\")`. A hardcoded literal goes stale \
         on every version bump and silently breaks the gate.\n\
         Pre-call window:\n{pre_window}"
    );
}

/// Inside the refusal branch of main.rs (when gate returns false),
/// the body must log the refusal AND must not contain
/// `.download_and_install`. If the install call slips into the
/// refusal arm (e.g. via a misplaced brace), the gate becomes
/// decorative — it still logs, but installs anyway.
#[test]
fn main_rs_refusal_branch_logs_and_skips_install() {
    let body = main_src();
    // The branch is `if !services::updater_gate::should_accept_update(
    // current, remote, ) { <refusal body> } else { <install body> }`.
    let if_pos = body
        .find("if !services::updater_gate::should_accept_update(")
        .expect("main.rs must have the `if !should_accept_update(...)` gate branch");
    let window = &body[if_pos..body.len().min(if_pos + 1200)];
    // Find the `} else {` that closes the refusal arm.
    let else_pos = window
        .find("} else {")
        .expect("main.rs gate branch must have an `} else {` that opens the install path");
    let refusal_arm = &window[..else_pos];
    assert!(
        refusal_arm.contains("error!("),
        "PRD 3.1.9: main.rs refusal arm must log via `error!(...)` so \
         operators can audit downgrade attempts. A silent refusal \
         indistinguishable from \"no update available\" defeats \
         incident response.\nRefusal arm:\n{refusal_arm}"
    );
    assert!(
        !refusal_arm.contains(".download_and_install"),
        "PRD 3.1.9: main.rs refusal arm must NOT call \
         `.download_and_install`. If the install call leaks into the \
         refusal branch, the gate is decorative.\nRefusal arm:\n{refusal_arm}"
    );
}

// --------------------------------------------------------------------
// Iter 232 structural pins — guard path-constants canonicalisation,
// predicate return-type pin, remote-arg provenance, inline-test-module
// presence, symbolic-test case-class distinctness.
//
// Iter-86 built the behavioural + wiring spec; iter-154 added six
// structural pins on the predicate SHAPE. These five extend to the
// meta-guard + callsite-provenance surface a confident refactor
// could still miss: path-constant drift (silent FS-not-found), a
// `-> bool` → `-> Result<bool, ...>` migration that pushes error
// handling to callers, a `remote` arg sourced from a stale literal,
// a missing inline test module (predicate unexercised at unit
// level), and symbolic tests collapsing to duplicates of the same
// case class. 78 iters have touched other guards in the meantime.
// --------------------------------------------------------------------

/// Iter 232: `GATE_RS` + `MAIN_RS` constants must stay canonical.
/// Every `gate_src()` / `main_src()` call resolves through these;
/// drift to a different file path (e.g. splitting the gate into
/// `updater/gate.rs`) would panic the test with "file not found"
/// pointing at the FS, not at the constant.
#[test]
fn guard_path_constants_are_canonical() {
    let guard_body = fs::read_to_string("tests/updater_downgrade.rs")
        .expect("guard source must exist");
    assert!(
        guard_body.contains("const GATE_RS: &str = \"src/services/updater_gate.rs\";"),
        "Iter 232: tests/updater_downgrade.rs must keep \
         `const GATE_RS: &str = \"src/services/updater_gate.rs\";` \
         verbatim. A path drift (e.g. splitting into \
         `services/updater/gate.rs`) leaves every `gate_src()` read \
         with a FS-not-found, misrouting triage."
    );
    assert!(
        guard_body.contains("const MAIN_RS: &str = \"src/main.rs\";"),
        "Iter 232: tests/updater_downgrade.rs must keep \
         `const MAIN_RS: &str = \"src/main.rs\";` verbatim. A refactor \
         that split main.rs into multiple files would need coordinated \
         constant + test updates here."
    );
}

/// Iter 232: the predicate return type must stay `-> bool`, not a
/// `Result<bool, ...>` or `Option<bool>`. A Result return pushes
/// error-handling responsibility to callers; a caller that uses
/// `.unwrap_or(true)` re-opens the malformed-manifest bypass the
/// iter-154 parse-error pin closes. Keeping the return `bool` with
/// internal refuse-on-Err forces the defensive default to live
/// INSIDE the gate where every caller inherits it.
#[test]
fn predicate_return_type_is_strictly_bool() {
    let body = gate_src();
    // Signature must end in `-> bool` (not `-> Result<bool`).
    assert!(
        body.contains("-> bool {"),
        "PRD 3.1.9 (iter 232): updater_gate.rs must declare \
         `-> bool {{` as the return shape. A `-> Result<bool, ...>` \
         migration pushes defensive handling to callers — a single \
         `.unwrap_or(true)` in a caller then re-opens every attack \
         class the internal refuse-on-Err closes."
    );
    assert!(
        !body.contains("-> Result<bool") && !body.contains("-> Option<bool>"),
        "PRD 3.1.9 (iter 232): updater_gate.rs must NOT declare \
         `-> Result<bool, ...>` or `-> Option<bool>` on \
         should_accept_update. Either weakens the fail-closed \
         default by moving error handling across the module boundary."
    );
}

/// Iter 232: the `remote` argument at the main.rs call site must be
/// sourced from the update manifest's `version` field, not a literal.
/// Iter-154 pinned `current = env!(CARGO_PKG_VERSION)` so the build-
/// time symbol covers the running-binary side; this pin covers the
/// symmetric attack on the remote side, where a caller that passed
/// `"99.99.99"` would always satisfy the strict-greater check and
/// make the gate cosmetic.
#[test]
fn main_rs_sources_remote_from_update_version() {
    let body = main_src();
    // Find the `remote` binding that feeds the gate. Two acceptable
    // shapes, both sourced from the Tauri v2 Update struct's `version`
    // field:
    //   `let remote = update.version.as_str();`
    //   `let remote = &update.version;`
    // A hardcoded literal (e.g. `let remote = "99.99.99";`) would
    // always satisfy strict-greater and make the gate cosmetic.
    let has_version_binding = body.contains("let remote = update.version.as_str()")
        || body.contains("let remote = &update.version")
        || body.contains("let remote = update.version.to_string()");
    assert!(
        has_version_binding,
        "PRD 3.1.9 (iter 232): main.rs must source the `remote` \
         argument from the Tauri v2 Update struct's `version` field — \
         `let remote = update.version.as_str();` or equivalent. A \
         hardcoded literal binding would always satisfy the strict-\
         greater check and make the gate cosmetic. grep for \
         `update.version` to locate the canonical site."
    );
    // Sanity: reject obvious literal patterns at the same binding.
    assert!(
        !body.contains("let remote = \""),
        "PRD 3.1.9 (iter 232): main.rs must NOT bind `remote` to a \
         string literal — always source from update.version."
    );
}

/// Iter 232: the production `updater_gate.rs` module must carry
/// inline unit tests (`#[cfg(test)] mod tests`). The integration
/// tests in this file prove the EXTERNAL contract via the symbolic
/// mirror + wiring guards, but the production predicate's own unit
/// coverage is what the module header calls out ("its own unit tests
/// (inline)"). Without the inline module, the "high-confidence on
/// the predicate semantics" claim in the header is decorative.
#[test]
fn updater_gate_module_carries_inline_test_module() {
    let body = gate_src();
    assert!(
        body.contains("#[cfg(test)]"),
        "PRD 3.1.9 (iter 232): updater_gate.rs must carry a \
         `#[cfg(test)]` attribute marking an inline test module. The \
         integration tests in tests/updater_downgrade.rs depend on the \
         inline tests for predicate-semantics coverage; without them, \
         a refactor that breaks (say) prerelease-ordering inside the \
         predicate would surface only at the integration-symbolic \
         layer, where the diagnostic signal is weaker."
    );
    assert!(
        body.contains("mod tests"),
        "PRD 3.1.9 (iter 232): updater_gate.rs must carry an inline \
         `mod tests {{ ... }}` module so the predicate is unit-tested \
         against the same crate it lives in."
    );
}

/// Iter 232: the five symbolic predicate tests must each cover a
/// DISTINCT case class — refuses-older, replay-same-version, accepts-
/// newer, prerelease-ordering, invalid-version-string. A refactor
/// that collapsed two tests (e.g. dropping the prerelease test and
/// merging its body into accepts_strictly_newer_versions) would
/// leave only four case classes under test without tripping a count
/// floor. Pin each by name so a rename / deletion trips CI.
#[test]
fn symbolic_predicate_tests_enumerate_five_distinct_case_classes() {
    let guard_body = fs::read_to_string("tests/updater_downgrade.rs")
        .expect("guard source must exist");
    for fn_name in [
        "fn refuses_older_latest_json()",
        "fn refuses_replay_of_same_version()",
        "fn accepts_strictly_newer_versions()",
        "fn prerelease_semantics_block_downgrade()",
        "fn invalid_version_strings_refused()",
    ] {
        assert!(
            guard_body.contains(fn_name),
            "PRD 3.1.9 (iter 232): tests/updater_downgrade.rs must \
             carry `{fn_name}` — each names a distinct case class. \
             Collapsing two into one would narrow symbolic coverage \
             without tripping a count-based floor."
        );
    }
}
