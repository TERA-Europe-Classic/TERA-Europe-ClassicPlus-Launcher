//! PRD 3.1.5 (CVE-2025-31477 shell-scope) call-site scanner drift guard.
//!
//! The shell-scope defence has two halves:
//! - **Scope half** — `tauri.conf.json` pins `shell.open` allow entries
//!   to a concrete regex list (iter 86 `shell_scope_pinned.rs`).
//! - **Call-site half** — `teralaunch/tests/shell-open-callsite.test.js`
//!   enforces that every JS call-site of `shell.open()` /
//!   `App.openExternal()` passes a string-literal / allowlisted
//!   identifier / URLS.external.* reference, NOT an arbitrary
//!   fetch-derived or DOM-derived string (iter 82).
//!
//! Either half going silent would re-open the CVE-2025-31477 RCE door
//! (arbitrary protocol URIs → OS-registered handlers). The scope half
//! already has a Rust pin; this guard adds the call-site half to the
//! Rust suite so a refactor can't weaken one half without the other.
//!
//! Parallel to iter 124 `i18n_no_hardcoded_guard` + iter 125
//! `i18n_scanner_guard`: Rust test asserting JS-scanner structure.

use std::fs;

const SCANNER: &str = "../tests/shell-open-callsite.test.js";
const APP_JS: &str = "../src/app.js";
const CAPABILITIES: &str = "capabilities/migrated.json";

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The scanner file must exist, be non-trivial, and self-identify as
/// the sec.shell-open-call-sites-pinned measurement.
#[test]
fn callsite_scanner_file_exists_and_self_identifies() {
    let body = read(SCANNER);
    assert!(
        body.len() > 3000,
        "sec.shell-open-call-sites-pinned violated: {SCANNER} is \
         missing or truncated (<3000 bytes). The call-site scanner \
         is half of the CVE-2025-31477 defence-in-depth; losing it \
         silences that half of the gate."
    );
    assert!(
        body.contains("CVE-2025-31477"),
        "{SCANNER} must cite CVE-2025-31477 in its header comment — \
         future readers need the connection to trace back WHY this \
         scanner exists."
    );
    assert!(
        body.contains("sec.shell-open-call-sites-pinned"),
        "{SCANNER} must self-identify with the fix-plan P-slot id \
         `sec.shell-open-call-sites-pinned` so grep finds it."
    );
}

/// The scanner must assert against BOTH call-site shapes:
/// `window.__TAURI__.shell.open(X)` (direct Tauri API call) and
/// `App.openExternal(X)` / `this.openExternal(X)` (our app wrapper).
/// Dropping either would leave a class of sinks unscanned.
#[test]
fn callsite_scanner_covers_both_sink_shapes() {
    let body = read(SCANNER);
    assert!(
        body.contains("window.__TAURI__.shell.open(X)"),
        "{SCANNER} must assert against `window.__TAURI__.shell.open(X)` \
         — the direct Tauri API sink."
    );
    assert!(
        body.contains("App.openExternal(X)"),
        "{SCANNER} must assert against `App.openExternal(X)` / \
         `this.openExternal(X)` — the app wrapper that feeds into \
         shell.open at app.js:2253."
    );
    // The regex sources for both sinks must appear verbatim. The
    // on-disk JS form has double-backslash (`\\.`) because JS string
    // literals need to escape each `\` for the RegExp constructor.
    assert!(
        body.contains(r"window\\.__TAURI__\\.shell\\.open"),
        "{SCANNER} must carry the escaped regex \
         `window\\\\.__TAURI__\\\\.shell\\\\.open` (JS source form). \
         A missing backslash would make the regex match-nothing and \
         pass vacuously."
    );
    assert!(
        body.contains(r"App\\.openExternal") && body.contains(r"this\\.openExternal"),
        "{SCANNER} must carry both `App\\\\.openExternal` and \
         `this\\\\.openExternal` regexes (JS source form) — both \
         caller shapes flow into the same sink."
    );
}

/// The SAFE_IDENTIFIERS allowlist must carry provenance comments for
/// every entry. The invariant is that every allowed identifier has a
/// documented reason why it's not attacker-controllable. Absent that
/// discipline the list becomes a graveyard of speculative entries.
#[test]
fn callsite_scanner_safe_identifiers_are_documented() {
    let body = read(SCANNER);
    // Extract the SAFE_IDENTIFIERS block and inspect entry shape.
    let after = body.split("const SAFE_IDENTIFIERS = [").nth(1).unwrap_or_else(
        || panic!("{SCANNER} missing SAFE_IDENTIFIERS declaration"),
    );
    let block = after.split("];").next().unwrap_or_else(
        || panic!("{SCANNER} SAFE_IDENTIFIERS not closed with `];`"),
    );
    // Count quoted entries (crude but enough — every entry must have
    // at least one `,` separating it from a provenance comment on
    // the same line).
    let quoted_entries = block.matches('\'').count() / 2;
    assert!(
        quoted_entries >= 3,
        "{SCANNER} SAFE_IDENTIFIERS must carry at least 3 entries \
         (localizedUrl, URLS/PROFILE_URL-like consts, DOM-anchor \
         href). Found {quoted_entries} quoted strings. A shrunk \
         list would mean call sites started failing the scanner — \
         the fix is to investigate and maybe re-add with provenance, \
         not to blank-rubber-stamp."
    );
    // Each entry line should carry a `//` comment (provenance). Split
    // by newlines and check every line that contains a quoted string
    // also contains `//`.
    let comma_separated_lines: Vec<&str> = block
        .lines()
        .filter(|l| l.contains('\''))
        .collect();
    for line in &comma_separated_lines {
        assert!(
            line.contains("//"),
            "{SCANNER} SAFE_IDENTIFIERS entry must carry provenance \
             comment on the same line. Offending line: `{}`. The \
             invariant is every allowed identifier cites WHY it is \
             not attacker-controllable (constant / enum / DOM-anchor \
             from app-authored HTML).",
            line.trim()
        );
    }
}

/// The classifier must carry all four safe-shape branches:
/// string literal, allowlisted identifier, URLS.external.*, and
/// safe template literal. Collapsing it to fewer branches would
/// force every currently-legal call site to fail or would widen
/// the accept set beyond the CVE-2025-31477 boundary.
#[test]
fn callsite_classifier_carries_four_safe_shapes() {
    let body = read(SCANNER);
    // String-literal regex — must match single/double-quoted.
    assert!(
        body.contains("^(['\"])[^'\"]*\\1$"),
        "{SCANNER} classifier must retain the string-literal regex \
         `^(['\\\"])[^'\\\"]*\\1$` (single/double-quoted strings \
         accepted)."
    );
    // Backtick (no-interpolation) literal regex.
    assert!(
        body.contains("^`[^`$]*`$"),
        "{SCANNER} classifier must retain the no-interp backtick \
         regex `^`[^`$]*`$`."
    );
    // URLS.external.<name> regex.
    assert!(
        body.contains("^URLS\\.external\\.[\\w]+$"),
        "{SCANNER} classifier must retain the URLS.external.* \
         regex `^URLS\\.external\\.[\\w]+$` — this is the primary \
         safe-sink surface for app-authored URLs."
    );
    // SAFE_IDENTIFIERS check.
    assert!(
        body.contains("SAFE_IDENTIFIERS.includes(trimmed)"),
        "{SCANNER} classifier must keep the \
         `SAFE_IDENTIFIERS.includes(trimmed)` branch — drops this \
         and every allowlisted identifier falls through to the \
         generic-reject path."
    );
    // Safe-template-literal interpolation check.
    assert!(
        body.contains("URLS\\.external\\.[\\w]+$") && body.contains("interpolations"),
        "{SCANNER} classifier must keep the template-literal \
         interpolation scan — allows `...${{URLS.external.foo}}...` \
         but rejects arbitrary `${{dangerous}}`."
    );
}

/// Both self-tests (negative + positive) must stay. Without the
/// negative, a regression to always-null classifyArg would pass the
/// real assertions vacuously. Without the positive, a regression to
/// always-reject would fail only on call-site data and leave the
/// classifier's own regression undetected.
#[test]
fn callsite_scanner_carries_positive_and_negative_self_tests() {
    let body = read(SCANNER);
    assert!(
        body.contains("classifier bites on seeded bad input"),
        "{SCANNER} must retain the negative self-test `classifier \
         bites on seeded bad input` — prevents always-null \
         classifyArg from rubber-stamping real call sites."
    );
    assert!(
        body.contains("classifier accepts every currently allowed shape"),
        "{SCANNER} must retain the positive self-test `classifier \
         accepts every currently allowed shape` — catches an \
         over-strict classifier that would reject legitimate call \
         sites."
    );
    // Positive fixture must include every of the 4 safe shapes.
    assert!(
        body.contains("\"https://example.com\""),
        "{SCANNER} positive self-test must exercise the string-\
         literal branch"
    );
    assert!(
        body.contains("URLS.external.forum"),
        "{SCANNER} positive self-test must exercise the \
         URLS.external.* branch"
    );
    assert!(
        body.contains("${URLS.external.register}"),
        "{SCANNER} positive self-test must exercise the safe-\
         template-literal branch"
    );
}

/// The scanned reference file (app.js) must exist. Without it the
/// scanner's call-site scan is vacuous.
#[test]
fn scanned_app_js_exists_and_is_non_empty() {
    let body = read(APP_JS);
    assert!(
        !body.trim().is_empty(),
        "{APP_JS} (the file the call-site scanner reads) must exist \
         and be non-empty. A deletion would make the scanner vacuous \
         (pass without inspecting anything)."
    );
    assert!(
        body.len() > 10_000,
        "{APP_JS} must be >10KB — smaller than that means the app \
         was gutted and the scanner's call-site surface is no longer \
         representative of the real attack surface."
    );
}

/// Self-test — prove the detectors in THIS guard bite on synthetic
/// bad shapes.
#[test]
fn shell_open_callsite_guard_detector_self_test() {
    // Bad shape A: scanner without CVE-2025-31477 citation.
    let uncited = "// some shell scanner\nconst X = 1;\n";
    assert!(
        !uncited.contains("CVE-2025-31477"),
        "self-test: scanner without CVE citation must be flagged"
    );

    // Bad shape B: scanner only covers direct sink (missing App.*).
    let direct_only = "findCallSites(src, 'window\\\\.__TAURI__\\\\.shell\\\\.open')";
    assert!(
        !direct_only.contains("App\\.openExternal"),
        "self-test: scanner missing App.openExternal coverage must \
         be flagged"
    );

    // Bad shape C: SAFE_IDENTIFIERS entry without provenance comment.
    let undocumented = "    'someVar',\n";
    assert!(
        undocumented.contains('\'') && !undocumented.contains("//"),
        "self-test: undocumented SAFE_IDENTIFIERS entry must be \
         flagged by the provenance check"
    );

    // Bad shape D: classifier without URLS.external.* branch.
    let narrow = "if (/^['\"]/.test(trimmed)) return null; return 'reject';";
    assert!(
        !narrow.contains("URLS\\.external\\.[\\w]+$"),
        "self-test: classifier without URLS.external.* branch must \
         be flagged"
    );

    // Iter 184 — additional bad shapes.

    // Bad shape E: SAFE_IDENTIFIERS containing `.value` (input-derived).
    let input_value = "'userInput.value',";
    assert!(
        input_value.contains(".value"),
        "self-test: .value detector must bite on the dangerous suffix"
    );

    // Bad shape F: capabilities with shell:allow-open scope widening.
    let widened = r#"{ "identifier": "shell:allow-open", "allow": [{ "url": "file://*" }] }"#;
    assert!(
        widened.contains("shell:allow-open") && widened.contains("file://"),
        "self-test: scope-widening capability must be flagged"
    );

    // Bad shape G: `.only` pin on shell-open scanner.
    let only_pin = "it.only('target file exists and is non-empty', () => {});";
    assert!(
        only_pin.contains("it.only"),
        "self-test: .only detector must bite on `it.only(`"
    );
}

/// Iter 184: scanner must carry at least 5 `it(` blocks — target-
/// exists + shell.open safety + App.openExternal safety + negative
/// self-test + positive self-test. Below the floor means one of the
/// CVE-2025-31477 defence-in-depth invariants was deleted.
#[test]
fn shell_open_scanner_has_minimum_it_count() {
    let body = read(SCANNER);
    let it_count = body.matches("it(").count() + body.matches("it.only(").count();
    assert!(
        it_count >= 5,
        "sec.shell-open-call-sites-pinned (iter 184): {SCANNER} must \
         carry at least 5 `it(` blocks. Found {it_count}. Five is \
         the minimum set: target-file + shell.open + openExternal + \
         negative + positive self-tests. Below the floor means a \
         CVE-2025-31477 defence-in-depth test was deleted."
    );
}

/// Iter 184: scanner must not carry `.only` / `.skip` / `xit` /
/// `xdescribe` — these silently disable sibling tests and could
/// drop the shell.open / openExternal safety checks in CI while
/// leaving the file intact.
#[test]
fn shell_open_scanner_carries_no_only_or_skip_markers() {
    let body = read(SCANNER);
    for forbidden in [
        "it.only(",
        "describe.only(",
        "it.skip(",
        "describe.skip(",
        "xit(",
        "xdescribe(",
    ] {
        assert!(
            !body.contains(forbidden),
            "sec.shell-open-call-sites-pinned (iter 184): {SCANNER} \
             must not carry `{forbidden}` — dev-local pins disable \
             CVE-2025-31477 defence tests in CI."
        );
    }
}

/// Iter 184: the real `src/app.js` must still contain `shell.open(`
/// or `openExternal(` call sites. Without any call sites the scanner
/// trivially passes (nothing to inspect) — a refactor that moved all
/// shell-open use to a helper elsewhere would go undetected.
#[test]
fn app_js_has_shell_open_call_sites_under_scanner_coverage() {
    let src = read(APP_JS);
    let shell_open_count = src.matches("window.__TAURI__.shell.open(").count();
    let app_external_count = src.matches("App.openExternal(").count();
    let this_external_count = src.matches("this.openExternal(").count();
    let total = shell_open_count + app_external_count + this_external_count;
    assert!(
        total >= 3,
        "sec.shell-open-call-sites-pinned (iter 184): {APP_JS} must \
         carry at least 3 shell.open / openExternal call sites for \
         the scanner to inspect. Found {total} (shell.open: \
         {shell_open_count}, App.openExternal: {app_external_count}, \
         this.openExternal: {this_external_count}). Zero means the \
         scanner trivially passes; near-zero means a refactor moved \
         shell-open use out of the scanned file."
    );
}

/// Iter 184: the Tauri capability `shell:allow-open` must remain a
/// BARE STRING in `capabilities/migrated.json`. The object-form
/// `{"identifier": "shell:allow-open", "allow": [...]}` would let a
/// custom scope be added that re-introduces `file://` / `smb://` /
/// `nfs://` — which is exactly the CVE-2025-31477 regression.
#[test]
fn capabilities_shell_allow_open_is_bare_string() {
    let cap = read(CAPABILITIES);
    assert!(
        cap.contains(r#""shell:allow-open""#),
        "sec.shell-open-call-sites-pinned (iter 184): {CAPABILITIES} \
         must carry `\"shell:allow-open\"` (the capability that \
         lets shell.open work at all). Missing means the feature \
         is broken in production."
    );
    // Reject the object form that would let a custom scope widen
    // defaults.
    assert!(
        !cap.contains(r#""identifier": "shell:allow-open""#),
        "sec.shell-open-call-sites-pinned (iter 184): {CAPABILITIES} \
         must NOT carry `shell:allow-open` in object form (with a \
         custom `allow` scope). The plugin's default scope is the \
         CVE-2025-31477 fix; a custom scope could re-introduce \
         dangerous protocols (file:// smb:// nfs://)."
    );
    // Also reject any scope entry that obviously names a dangerous
    // protocol under a shell-open context.
    for dangerous in ["\"file://", "\"smb://", "\"nfs://"] {
        // If the substring appears at all, fail — none of these
        // belong anywhere in a shell-open capability context.
        if cap.contains(dangerous) {
            // Only flag if within 200 chars of `shell:allow-open`
            // to avoid false positives on unrelated config.
            let shell_pos = cap.find("shell:allow-open").unwrap_or(0);
            let dang_pos = cap.find(dangerous).unwrap_or(usize::MAX);
            let distance = shell_pos.abs_diff(dang_pos);
            assert!(
                distance > 500,
                "sec.shell-open-call-sites-pinned (iter 184): \
                 {CAPABILITIES} carries `{dangerous}` within \
                 {distance} chars of `shell:allow-open` — \
                 that is the CVE-2025-31477 regression class."
            );
        }
    }
}

// --------------------------------------------------------------------
// Iter 219 structural pins — meta-guard header + 3 path constants +
// sister scope-guard presence + openExternal production wrapper +
// capabilities main-window scope.
// --------------------------------------------------------------------
//
// The twelve pins above cover scanner presence + both sink shapes +
// SAFE_IDENTIFIERS documented + 4 safe classifier branches + positive/
// negative self-tests + app.js existence + it-count + .only/.skip +
// call-site count in app.js + capability bare-string + DOM-input
// rejection. They do NOT pin: (a) the guard's own header cites
// `PRD 3.1.5` + `CVE-2025-31477` — meta-guard contract; (b) SCANNER +
// APP_JS + CAPABILITIES path constants equal canonical relative forms;
// (c) `tests/shell_scope_pinned.rs` (the SCOPE half of the CVE-2025-
// 31477 defence, per the header's self-documentation) still exists —
// deleting it would break the defence-in-depth chain without any
// single guard noticing; (d) `src/app.js` `openExternal(url)` wrapper
// exists AND calls `window.__TAURI__.shell.open(...)` inside — the
// wrapper is the funnel every app-side external-link call flows
// through; (e) `capabilities/migrated.json` windows scope is `"main"`
// (not `"*"` wildcard) — a wildcard would apply shell:allow-open to
// every future window including potentially untrusted ones.

/// The guard's own module header must cite both `PRD 3.1.5` and
/// `CVE-2025-31477` so a reader chasing either the PRD section or
/// the CVE-tracker reference lands here via grep.
#[test]
fn guard_file_header_cites_prd_3_1_5_and_cve_2025_31477() {
    let body = fs::read_to_string("tests/shell_open_callsite_guard.rs")
        .expect("tests/shell_open_callsite_guard.rs must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.1.5"),
        "meta-guard contract: tests/shell_open_callsite_guard.rs \
         header must cite `PRD 3.1.5`. Without it, a reader chasing \
         the shell-scope criterion won't land here via section-grep.\n\
         Header:\n{header}"
    );
    assert!(
        header.contains("CVE-2025-31477"),
        "meta-guard contract: header must cite `CVE-2025-31477`. \
         Without it, a future maintainer may not understand WHY this \
         call-site scanner exists."
    );
}

/// All three path constants must equal their canonical relative forms
/// verbatim. A rename (SCANNER / APP_JS / CAPABILITIES) would silently
/// cause `read(path)` to panic with opaque "file not readable"
/// messages that obscure the root cause.
#[test]
fn all_path_constants_are_canonical() {
    let guard_body = fs::read_to_string("tests/shell_open_callsite_guard.rs")
        .expect("guard source must be readable");
    for literal in [
        "const SCANNER: &str = \"../tests/shell-open-callsite.test.js\";",
        "const APP_JS: &str = \"../src/app.js\";",
        "const CAPABILITIES: &str = \"capabilities/migrated.json\";",
    ] {
        assert!(
            guard_body.contains(literal),
            "sec.shell-open-call-sites-pinned (iter 219): \
             tests/shell_open_callsite_guard.rs must retain \
             `{literal}` verbatim. A rename without atomic constant \
             update would break every pin with an opaque panic."
        );
    }
}

/// `tests/shell_scope_pinned.rs` (the SCOPE HALF of the CVE-2025-
/// 31477 defence) must still exist and be non-empty. The header of
/// this guard names it explicitly as the partner guard; deleting it
/// would break the defence-in-depth chain without any single guard
/// noticing. This pin catches such a cross-guard regression.
#[test]
fn sister_scope_guard_still_present() {
    let path = "tests/shell_scope_pinned.rs";
    let body = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!(
            "sec.shell-open-call-sites-pinned (iter 219): {path} \
             (the SCOPE half of the CVE-2025-31477 defence) must \
             still exist. Deleting it breaks defence-in-depth: the \
             call-site scanner (this guard's target) is only half \
             the story — the scope-level config pin is the other \
             half. Error: {e}"
        ));
    assert!(
        body.len() > 1000,
        "sec.shell-open-call-sites-pinned (iter 219): {path} must \
         carry substantive content (> 1000 bytes). A truncation to \
         a stub would break the scope-half of the CVE defence."
    );
    // It must still cite CVE-2025-31477 so the two guards stay in sync.
    assert!(
        body.contains("CVE-2025-31477"),
        "sec.shell-open-call-sites-pinned (iter 219): {path} must \
         still cite `CVE-2025-31477` — the two guards are paired by \
         this CVE reference; drift would mean someone touched one \
         without considering the other."
    );
}

/// `src/app.js` must define an `openExternal(url)` method AND call
/// `window.__TAURI__.shell.open(...)` inside. The wrapper is the
/// funnel every app-side external-link call flows through; renaming
/// it (e.g. to `launchExternal`) would orphan the call sites in
/// iter-184's count pin.
#[test]
fn app_js_openexternal_wrapper_exists_and_calls_shell_open() {
    let src = read(APP_JS);
    let fn_pos = src
        .find("openExternal(url) {")
        .expect(
            "sec.shell-open-call-sites-pinned (iter 219): src/app.js \
             must define `openExternal(url) {` method — the funnel \
             every app-side external-link call flows through.",
        );
    // Window covers the method body.
    let window = &src[fn_pos..fn_pos.saturating_add(800)];
    assert!(
        window.contains("window.__TAURI__.shell.open("),
        "sec.shell-open-call-sites-pinned (iter 219): src/app.js \
         `openExternal(url)` body must call \
         `window.__TAURI__.shell.open(...)`. Without it, the wrapper \
         would bypass the Tauri shell plugin (e.g. fall through to \
         `window.open(...)` unconditionally), defeating the \
         CVE-2025-31477 allowlist that lives in the plugin.\n\
         Window:\n{window}"
    );
}

/// `capabilities/migrated.json` must scope permissions to the main
/// window (`["main"]`), NOT a wildcard (`["*"]` or any pattern
/// matching additional windows). A wildcard would apply shell:allow-
/// open to every future window — including potentially untrusted
/// webviews (e.g. a future in-launcher browser pane) that would
/// inherit the permission.
#[test]
fn capabilities_windows_scope_is_main_not_wildcard() {
    let cap = read(CAPABILITIES);
    let conf: serde_json::Value =
        serde_json::from_str(&cap).expect("capabilities/migrated.json must parse as JSON");
    let windows = conf
        .pointer("/windows")
        .and_then(|v| v.as_array())
        .expect("capabilities/migrated.json must carry a `windows` array");
    let entries: Vec<String> = windows
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "sec.shell-open-call-sites-pinned (iter 219): \
         {CAPABILITIES} must scope to exactly 1 window entry. Found \
         {}. Expanding the list applies `shell:allow-open` to more \
         windows — each addition must be audited.",
        entries.len()
    );
    assert_eq!(
        entries[0], "main",
        "sec.shell-open-call-sites-pinned (iter 219): \
         {CAPABILITIES} windows entry must be `\"main\"` (not `\"*\"` \
         or a wildcard pattern). A wildcard applies shell:allow-open \
         to every future window including potentially untrusted \
         webviews."
    );
}

// --------------------------------------------------------------------
// Iter 257 structural pins — scanner size ceiling + capabilities JSON
// validity + SAFE_IDENTIFIERS ceiling + app.js call-site ceiling +
// scanner describe wrapper.
// --------------------------------------------------------------------
//
// The seventeen pins above cover scanner presence, sink shapes,
// SAFE_IDENTIFIERS provenance, classifier branches, self-tests,
// app.js existence, it-count, marker absence, call-site floor,
// capability bare-string + main-window scope, header cite, path
// constants, sister scope guard, openExternal wrapper, DOM-input
// rejection. They do NOT pin:
// (a) the scanner file has a sane upper byte ceiling;
// (b) `capabilities/migrated.json` parses as valid JSON (iter-219
//     `capabilities_windows_scope_is_main_not_wildcard` does
//     serde_json::from_str internally but doesn't PIN validity
//     explicitly — if someone removes the windows array, the test
//     fails with an opaque "must carry a windows array" panic);
// (c) SAFE_IDENTIFIERS has a sane upper ceiling — adding entries
//     widens the trusted surface and each addition should be audited;
// (d) `src/app.js` shell-open call-site count has a ceiling — security
//     surface pressure. Too many call sites means each is probably
//     not being audited;
// (e) the scanner carries a top-level `describe(` block — Vitest
//     idiom; without it failures land with only file:line context,
//     not the security-category grouping the scanner intends.

/// The scanner file must not exceed a sane upper byte ceiling.
/// Current state: ~8 KB. A 40 KB ceiling gives 5× margin while
/// catching bloat from unrelated tests merged into the scanner.
#[test]
fn scanner_file_size_has_upper_ceiling() {
    const MAX_BYTES: usize = 40_000;
    let bytes = fs::metadata(SCANNER)
        .unwrap_or_else(|e| panic!("{SCANNER}: {e}"))
        .len() as usize;
    assert!(
        bytes <= MAX_BYTES,
        "sec.shell-open-call-sites-pinned (iter 257): {SCANNER} is \
         {bytes} bytes; ceiling is {MAX_BYTES}. Bloat past the ceiling \
         signals unrelated tests merged into the scanner or a runaway \
         fixture — the CVE-2025-31477 focus narrows."
    );
}

/// `capabilities/migrated.json` must parse as valid JSON. The
/// iter-219 `capabilities_windows_scope_is_main_not_wildcard` pin
/// does a serde_json::from_str internally but panics with an opaque
/// message if the file structure changes (missing windows array).
/// This pin pins VALIDITY explicitly so a syntax regression surfaces
/// here, with a clear message, before the other pins misfire.
#[test]
fn capabilities_json_is_valid() {
    let cap = read(CAPABILITIES);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&cap);
    let value = parsed.unwrap_or_else(|e| {
        panic!(
            "sec.shell-open-call-sites-pinned (iter 257): {CAPABILITIES} \
             must parse as valid JSON. Parse error: {e}. An unterminated \
             string or trailing comma would cause the Tauri build to \
             fail at boot; pinning validity here surfaces the regression \
             at test time."
        )
    });
    assert!(
        value.is_object(),
        "sec.shell-open-call-sites-pinned (iter 257): {CAPABILITIES} \
         must parse as a JSON object (not array or scalar). Got: {value:?}"
    );
}

/// `SAFE_IDENTIFIERS` must carry a sane upper ceiling. Each added
/// entry widens the trusted surface — a list that grows to 30+
/// entries signals the allowlist has accumulated ad-hoc exceptions
/// instead of being maintained as a curated set. Current state:
/// ~5-10 entries; a ceiling of 20 gives generous room while catching
/// runaway additions.
#[test]
fn safe_identifiers_list_has_sane_ceiling() {
    const MAX_ENTRIES: usize = 20;
    let body = read(SCANNER);
    let start = body
        .find("SAFE_IDENTIFIERS = [")
        .expect("scanner SAFE_IDENTIFIERS missing");
    let remaining = &body[start..];
    let end_rel = remaining.find("];").expect("SAFE_IDENTIFIERS not closed");
    let block = &remaining[..end_rel];
    let quoted_entries = block.matches('\'').count() / 2;
    assert!(
        quoted_entries <= MAX_ENTRIES,
        "sec.shell-open-call-sites-pinned (iter 257): \
         SAFE_IDENTIFIERS has {quoted_entries} entries; ceiling is \
         {MAX_ENTRIES}. A list that grows past the ceiling signals \
         ad-hoc exceptions accumulating instead of a curated set — \
         each widening of the trusted surface deserves an audit."
    );
}

/// `src/app.js` shell-open call-site count must not exceed a sane
/// ceiling. Each call-site is a security surface that needs its
/// argument classified as safe by the scanner; a runaway count (50+)
/// signals the feature is leaking into codepaths that should probably
/// use a different sink (e.g. relative-link navigation via Router).
/// Current state: ~10-20 call sites; a ceiling of 50 gives margin.
#[test]
fn app_js_shell_open_call_site_count_has_sane_ceiling() {
    const MAX_CALLS: usize = 50;
    let src = read(APP_JS);
    let shell_open_count = src.matches("window.__TAURI__.shell.open(").count();
    let app_external_count = src.matches("App.openExternal(").count();
    let this_external_count = src.matches("this.openExternal(").count();
    let total = shell_open_count + app_external_count + this_external_count;
    assert!(
        total <= MAX_CALLS,
        "sec.shell-open-call-sites-pinned (iter 257): {APP_JS} has \
         {total} shell-open call sites (shell.open: \
         {shell_open_count}, App.openExternal: {app_external_count}, \
         this.openExternal: {this_external_count}); ceiling is \
         {MAX_CALLS}. A runaway count signals the feature is leaking \
         into codepaths that should use a different sink (Router, \
         in-launcher navigation)."
    );
}

/// The scanner must carry at least one top-level `describe(` wrapper
/// block. Vitest supports flat tests without a describe, but the
/// scanner's 5+ `it(` blocks need a describe wrapper to give the
/// suite a name in test output — without it, failures surface as
/// `shell-open-callsite.test.js:<line>` rather than the
/// CVE-2025-31477-category grouping the scanner is organizing.
#[test]
fn scanner_carries_describe_wrapper_block() {
    let body = read(SCANNER);
    let describe_count = body.matches("describe(").count();
    assert!(
        describe_count >= 1,
        "sec.shell-open-call-sites-pinned (iter 257): {SCANNER} has \
         {describe_count} `describe(` block(s); floor is 1. A flat \
         test file gives readers no semantic grouping when a failure \
         lands — the only context is file:line, not the CVE-2025-31477 \
         category the failure belongs to."
    );
}

/// Iter 184: the SAFE_IDENTIFIERS allowlist in the scanner must not
/// contain entries with DOM-input-derived suffixes. `.value`,
/// `.innerText`, `.textContent`, `.innerHTML` are attacker-
/// controllable (user-typed, untrusted innerHTML). `document.` and
/// `window.location` are similarly unsafe bases. Adding any of
/// these would pass untrusted strings to shell.open.
#[test]
fn scanner_safe_identifiers_reject_dom_input_patterns() {
    let body = read(SCANNER);
    let start = body
        .find("SAFE_IDENTIFIERS = [")
        .expect("scanner SAFE_IDENTIFIERS missing");
    let remaining = &body[start..];
    let end_rel = remaining.find("];").expect("SAFE_IDENTIFIERS not closed");
    let block = &remaining[..end_rel];
    // Scan each quoted entry; the block has `'entry', // comment` per
    // line. Split by newlines, look at only the `'...'` quoted literal.
    for line in block.lines() {
        // Extract the first `'...'` on the line if present.
        let Some(first_quote) = line.find('\'') else { continue };
        let rest = &line[first_quote + 1..];
        let Some(end_quote) = rest.find('\'') else { continue };
        let entry = &rest[..end_quote];
        for dangerous in [
            ".value",
            ".innerText",
            ".textContent",
            ".innerHTML",
            "document.",
            "window.location",
        ] {
            assert!(
                !entry.contains(dangerous),
                "sec.shell-open-call-sites-pinned (iter 184): \
                 {SCANNER} SAFE_IDENTIFIERS entry `{entry}` contains \
                 DOM-input pattern `{dangerous}` — that is \
                 attacker-controllable, not safe. Find a safer sink \
                 (URLS.external.* / explicit constant) or reject \
                 this call site entirely."
            );
        }
    }
}
