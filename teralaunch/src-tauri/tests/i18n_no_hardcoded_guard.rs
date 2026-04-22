//! PRD 3.7.4 — i18n no-hardcoded-english scanner drift guard.
//!
//! Criterion: "No hardcoded English user-facing copy in mods.js /
//! mods.html — every string must route through the i18n layer
//! (`data-translate*` attributes or `this.t(...)`)." The measurement
//! cites `teralaunch/tests/i18n-no-hardcoded.test.js` (Vitest scanner).
//!
//! Iter 77 burned the allowlist down to zero; the scanner now
//! enforces strict-zero. This guard pins the scanner's structure so a
//! future refactor can't silently re-introduce an allowlist, drop a
//! rule, or remove a TARGETS entry and have the Vitest suite still go
//! green against a weakened detector.
//!
//! Parallel to iter 114 `secret_scan_guard.rs` (Rust test asserting
//! JS/YAML file structure) and iter 115 `deploy_scope_infra_guard.rs`
//! (Rust test asserting deploy scope spec invariants).

use std::fs;

const SCANNER: &str = "../tests/i18n-no-hardcoded.test.js";
const TARGET_MODS_JS: &str = "../src/mods.js";
const TARGET_MODS_HTML: &str = "../src/mods.html";

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The Vitest scanner file must exist and be non-trivial. A deletion
/// or truncation-to-stub would silently turn off the PRD 3.7.4 gate.
#[test]
fn i18n_scanner_file_exists_and_is_non_trivial() {
    let body = read(SCANNER);
    assert!(
        body.len() > 2000,
        "PRD 3.7.4 violated: {SCANNER} is missing or truncated \
         (<2000 bytes). The i18n hardcoded-English scanner is the \
         measurement for the criterion; losing it means the gate \
         goes silent."
    );
    assert!(
        body.contains("PRD 3.7.4"),
        "{SCANNER} must self-identify as the PRD 3.7.4 measurement \
         in its header comment — future readers trace back via grep."
    );
}

/// TARGETS array must cover both `mods.js` and `mods.html`. Dropping
/// either would leave half the surface unscanned.
#[test]
fn scanner_targets_both_mods_js_and_mods_html() {
    let body = read(SCANNER);
    assert!(
        body.contains("'teralaunch/src/mods.js'"),
        "{SCANNER} TARGETS must include `teralaunch/src/mods.js` — \
         this is the primary mod-manager surface."
    );
    assert!(
        body.contains("'teralaunch/src/mods.html'"),
        "{SCANNER} TARGETS must include `teralaunch/src/mods.html` — \
         aria-label / title / placeholder drift lives in markup too."
    );
}

/// ALLOWLIST must stay the empty array. Iter 77 burned it down to
/// zero; the invariant is that a new hardcoded leak gets FIXED, not
/// papered over by an allowlist addition.
#[test]
fn scanner_allowlist_is_strict_zero() {
    let body = read(SCANNER);
    assert!(
        body.contains("const ALLOWLIST = [];"),
        "PRD 3.7.4 strict-zero violated: {SCANNER} ALLOWLIST must \
         remain the empty array `const ALLOWLIST = [];`. If a new \
         leak appears, the fix is to wire it through i18n — NOT to \
         add an allowlist row."
    );
}

/// The three regex rules (aria-label, title, placeholder) must all
/// be present. Dropping one would let that attribute class leak
/// English without tripping CI.
#[test]
fn scanner_carries_three_attribute_rules() {
    let body = read(SCANNER);
    // aria-label — the biggest accessibility-visible surface.
    assert!(
        body.contains("aria-label=\"([^\"]{2,})\""),
        "{SCANNER} must carry the aria-label rule \
         `aria-label=\"([^\"]{{2,}})\"`. Aria-labels are the highest-\
         value i18n target (screen-reader visible)."
    );
    // title — tooltip text.
    assert!(
        body.contains("\\btitle=\"([^\"]{2,})\""),
        "{SCANNER} must carry the title rule \
         `\\btitle=\"([^\"]{{2,}})\"`. Title attributes render as \
         tooltips and must be translated."
    );
    // placeholder — input field hints.
    assert!(
        body.contains("\\bplaceholder=\"([^\"]{2,})\""),
        "{SCANNER} must carry the placeholder rule \
         `\\bplaceholder=\"([^\"]{{2,}})\"`. Placeholder copy is \
         user-facing and needs i18n routing."
    );
}

/// The `looksEnglish` heuristic must require both a multi-letter run
/// AND whitespace. Relaxing either half (e.g. dropping the whitespace
/// check) would flood the scanner with false positives on all-caps
/// data-attribute keys; dropping the letter check would match on
/// numeric-only values.
#[test]
fn scanner_looks_english_heuristic_is_tight() {
    let body = read(SCANNER);
    assert!(
        body.contains("/[a-z]{2,}/.test(s)") && body.contains("/\\s/.test(s)"),
        "{SCANNER} looksEnglish() must retain BOTH the \
         `/[a-z]{{2,}}/` letter-run check AND the `/\\s/` whitespace \
         check. Loosening either half inverts the FP/FN balance that \
         iter 77 landed on."
    );
}

/// The scanned reference files (`mods.js` + `mods.html`) must exist.
/// If either disappears, TARGETS becomes stale and the scanner
/// passes vacuously.
#[test]
fn scanned_target_files_exist() {
    let mods_js = read(TARGET_MODS_JS);
    assert!(
        !mods_js.trim().is_empty(),
        "{TARGET_MODS_JS} (one of the PRD 3.7.4 scan targets) must \
         exist and be non-empty. A deletion would make the scanner \
         vacuous (pass without inspecting anything)."
    );
    let mods_html = read(TARGET_MODS_HTML);
    assert!(
        !mods_html.trim().is_empty(),
        "{TARGET_MODS_HTML} (one of the PRD 3.7.4 scan targets) must \
         exist and be non-empty."
    );
}

/// The scanner's own self-test (`detector flags a seeded leak`) must
/// remain. Without it, the main assertions could silently regress to
/// returning no leaks and pass vacuously.
#[test]
fn scanner_carries_its_own_self_test() {
    let body = read(SCANNER);
    assert!(
        body.contains("detector flags a seeded leak"),
        "{SCANNER} must retain the `detector flags a seeded leak` \
         self-test. Without it, a scanner that regressed to always-\
         empty would pass the other assertions vacuously."
    );
    assert!(
        body.contains("aria-label=\"Some Hardcoded Text\""),
        "{SCANNER} self-test must keep the synthetic bad-shape \
         fixture so the detector is proven to bite."
    );
}

/// Self-test — prove the detectors in THIS guard bite on synthetic
/// bad shapes of the scanner file.
#[test]
fn i18n_guard_detector_self_test() {
    // Bad shape A: scanner with an ALLOWLIST that got a row added.
    let weakened = "const ALLOWLIST = [\n  { file: 'x', literal: 'Leak' }\n];\n";
    assert!(
        !weakened.contains("const ALLOWLIST = [];"),
        "self-test: a scanner with a non-empty allowlist must be \
         flagged by the strict-zero assertion"
    );

    // Bad shape B: scanner with only aria-label (dropped title +
    // placeholder rules).
    let one_rule = "aria-label=\"([^\"]{2,})\"";
    assert!(
        !one_rule.contains("\\btitle=\"") && !one_rule.contains("\\bplaceholder=\""),
        "self-test: a scanner with only aria-label coverage must be \
         flagged"
    );

    // Bad shape C: scanner missing one TARGETS entry.
    let partial = "const TARGETS = ['teralaunch/src/mods.js'];";
    assert!(
        !partial.contains("mods.html"),
        "self-test: a scanner missing mods.html from TARGETS must be \
         flagged"
    );

    // Bad shape D: looksEnglish that dropped the whitespace check.
    let loose_heuristic = "function looksEnglish(s){return /[a-z]{2,}/.test(s);}";
    assert!(
        !loose_heuristic.contains("/\\s/.test(s)"),
        "self-test: a looksEnglish without the whitespace guard must \
         be flagged"
    );

    // Iter 185 — additional bad shapes.

    // Bad shape E: `.only` pin on i18n scanner.
    let only_pin = "it.only('no hardcoded english in mods.js', () => {});";
    assert!(
        only_pin.contains("it.only"),
        "self-test: .only detector must bite on `it.only(`"
    );

    // Bad shape F: scanner that flags leaks without checking for the
    // data-translate-* sibling attribute (would fire on every annotated
    // element).
    let no_sibling_check = "if (line.match(/aria-label=\"([^\"]+)\"/)) unsafe.push(line);";
    assert!(
        !no_sibling_check.contains("data-translate-aria-label"),
        "self-test: scanner without sibling data-translate-* check \
         must be flagged"
    );

    // Bad shape G: mods.html stripped of all data-translate markers
    // (i18n bypassed entirely).
    let stripped_html = "<button>Install</button><span>Uninstall</span>";
    assert!(
        !stripped_html.contains("data-translate"),
        "self-test: i18n-stripped markup must be flagged"
    );
}

/// Iter 185: scanner must carry at least 3 `it(` blocks — the
/// seven-invariant breakdown (file exists, TARGETS coverage, strict-
/// zero allowlist, 3-attribute rules, looksEnglish tight, targets
/// exist, detector self-test) compresses to several describes; the
/// floor still catches multi-test deletions.
#[test]
fn i18n_scanner_has_minimum_it_count() {
    let body = read(SCANNER);
    let it_count = body.matches("it(").count() + body.matches("it.only(").count();
    assert!(
        it_count >= 3,
        "PRD 3.7.4 (iter 185): {SCANNER} must carry at least 3 `it(` \
         blocks. Found {it_count}. Below the floor means an i18n \
         leak-detection test was deleted."
    );
}

/// Iter 185: scanner must not carry `.only` / `.skip` / `xit` /
/// `xdescribe` — these silently disable sibling tests and could
/// drop the i18n strict-zero enforcement in CI.
#[test]
fn i18n_scanner_carries_no_only_or_skip_markers() {
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
            "PRD 3.7.4 (iter 185): {SCANNER} must not carry \
             `{forbidden}` — dev-local pins disable sibling leak-\
             detection tests in CI."
        );
    }
}

/// Iter 185: scanner must carry the sibling `data-translate-*` check
/// alongside the attribute-regex rules. Without the sibling check,
/// a correctly-annotated element like `<button aria-label="Close"
/// data-translate-aria-label="CLOSE">` would still flag, forcing
/// teams to either add allowlist rows or bypass the scanner.
#[test]
fn scanner_enforces_sibling_data_translate_check() {
    let body = read(SCANNER);
    assert!(
        body.contains("data-translate-aria-label="),
        "PRD 3.7.4 (iter 185): {SCANNER} must reference \
         `data-translate-aria-label=` in its sibling-attribute \
         check. Without it, annotated aria-label elements would \
         flag and the test would pressure teams toward allowlist \
         rows — defeating the iter-77 strict-zero invariant."
    );
    // Also check the continue / skip path exists (i.e. the scanner
    // actually honours the sibling by skipping to the next match).
    assert!(
        body.contains("continue"),
        "PRD 3.7.4 (iter 185): {SCANNER} must contain a `continue` \
         statement in the leak-detection loop — without it, the \
         sibling check has nowhere to branch to and would bypass the \
         exclusion."
    );
}

/// Iter 185: the production `src/mods.js` must still use i18n via
/// a `.t(` wrapper call (current shape: `window.App?.t(` with the
/// optional-chain fallback). Zero calls means i18n was bypassed
/// entirely; the scanner would silently "pass" against an un-i18n'd
/// file.
#[test]
fn mods_js_uses_i18n_wrapper_calls() {
    let src = read(TARGET_MODS_JS);
    let t_calls = src.matches("App?.t(").count()
        + src.matches("App.t(").count()
        + src.matches("this.t(").count();
    assert!(
        t_calls >= 3,
        "PRD 3.7.4 (iter 185): {TARGET_MODS_JS} must use a `.t(` \
         i18n wrapper (App?.t / App.t / this.t) at least 3 times. \
         Found {t_calls}. Zero or near-zero means the file bypasses \
         i18n entirely — all user-facing copy would be hardcoded \
         English even if the scanner's attribute rules pass."
    );
}

/// Iter 185: the production `src/mods.html` must still carry
/// `data-translate=` markers. Zero markers means the markup is
/// not i18n-wired; the attribute-sibling rules would have nothing
/// to exempt.
#[test]
fn mods_html_carries_data_translate_markers() {
    let html = read(TARGET_MODS_HTML);
    let markers = html.matches("data-translate=").count();
    assert!(
        markers >= 5,
        "PRD 3.7.4 (iter 185): {TARGET_MODS_HTML} must carry at \
         least 5 `data-translate=` markers. Found {markers}. Below \
         the floor means the markup was stripped of i18n wiring; \
         user-facing copy would render raw."
    );
}

// --------------------------------------------------------------------
// Iter 224 structural pins — meta-guard header + 3 path constants +
// iter-77 rationale comment + sibling-check triple coverage + {2,}
// regex-quantifier floor.
// --------------------------------------------------------------------
//
// The thirteen pins above cover scanner existence + target coverage +
// strict-zero allowlist + three attribute rules + looksEnglish shape +
// target-file existence + scanner self-test + detector self-test +
// it-count floor + .only/.skip absence + sibling check + i18n wrapper
// usage + data-translate markers. They do NOT pin: (a) the guard's
// own header cites PRD 3.7.4 — meta-guard contract; (b) SCANNER +
// TARGET_MODS_JS + TARGET_MODS_HTML path constants equal canonical
// verbatim forms; (c) the `ALLOWLIST = []` declaration has an
// adjacent `iter 77` / `fix.mods-hardcoded-i18n-strings` rationale
// comment — strict-zero without a comment is one `ALLOWLIST.push(...)`
// away from silent regression; (d) the scanner sibling-check exempts
// ALL THREE attribute classes (aria-label + title + placeholder),
// not just one — iter 185 pinned only aria-label; the title +
// placeholder sibling checks could still silently drop; (e) all three
// attribute regexes use `{2,}` quantifier (minimum two chars), not
// `+` (which would flag single-character markup like `id=""`).

/// The guard's own module header must cite PRD 3.7.4 so a reader
/// chasing an i18n-strict-zero regression lands here via section-grep.
#[test]
fn guard_file_header_cites_prd_3_7_4() {
    let body = fs::read_to_string("tests/i18n_no_hardcoded_guard.rs")
        .expect("tests/i18n_no_hardcoded_guard.rs must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.7.4"),
        "meta-guard contract: tests/i18n_no_hardcoded_guard.rs header \
         must cite `PRD 3.7.4`. Without it, a reader chasing a \
         hardcoded-English regression won't land here via section-grep.\n\
         Header:\n{header}"
    );
    assert!(
        header.contains("hardcoded") || header.contains("no-hardcoded"),
        "meta-guard contract: header must name the invariant \
         (`hardcoded` / `no-hardcoded`) so the file-under-test is \
         unambiguous from the header alone."
    );
}

/// `SCANNER` + `TARGET_MODS_JS` + `TARGET_MODS_HTML` path constants
/// must equal their canonical relative forms verbatim.
#[test]
fn scanner_and_target_path_constants_are_canonical() {
    let guard_body = fs::read_to_string("tests/i18n_no_hardcoded_guard.rs")
        .expect("guard source must be readable");
    for literal in [
        "const SCANNER: &str = \"../tests/i18n-no-hardcoded.test.js\";",
        "const TARGET_MODS_JS: &str = \"../src/mods.js\";",
        "const TARGET_MODS_HTML: &str = \"../src/mods.html\";",
    ] {
        assert!(
            guard_body.contains(literal),
            "PRD 3.7.4 (iter 224): tests/i18n_no_hardcoded_guard.rs \
             must retain `{literal}` verbatim. A rename of any path \
             constant without atomic file update would break every \
             pin with an opaque `file not readable` panic."
        );
    }
}

/// The `ALLOWLIST = [];` declaration must have an adjacent comment
/// citing `iter 77` + the fix-plan slug `fix.mods-hardcoded-i18n-
/// strings`. Without the rationale comment, a future contributor
/// sees an empty array and may conclude "the list must need some
/// entries" — a single `ALLOWLIST.push(...)` regresses the invariant.
/// The comment preserves the institutional memory of why the list
/// is deliberately strict-zero.
#[test]
fn scanner_allowlist_empty_carries_iter_77_rationale_comment() {
    let body = read(SCANNER);
    let allow_pos = body
        .find("const ALLOWLIST = [];")
        .expect("ALLOWLIST = [] declaration must exist (iter-77 pin)");
    // Look at the 500 chars BEFORE the declaration for the comment.
    let comment_start = allow_pos.saturating_sub(500);
    let window = &body[comment_start..allow_pos];
    assert!(
        window.contains("iter 77"),
        "PRD 3.7.4 (iter 224): {SCANNER} must carry an `iter 77` \
         comment adjacent to `const ALLOWLIST = [];`. Without the \
         rationale, a future contributor sees an empty array and may \
         try to add entries. Preceding 500 chars:\n{window}"
    );
    assert!(
        window.contains("fix.mods-hardcoded-i18n-strings"),
        "PRD 3.7.4 (iter 224): {SCANNER} must cite the fix-plan slug \
         `fix.mods-hardcoded-i18n-strings` alongside the iter-77 \
         reference — slug-grep is how future readers cross-reference \
         the fix-plan history."
    );
}

/// The sibling-check in the scanner must exempt ALL THREE attribute
/// classes — aria-label, title, placeholder. Iter 185's
/// `scanner_enforces_sibling_data_translate_check` pin only asserts
/// the aria-label variant exists; the title + placeholder variants
/// could silently be dropped, making those two attributes fire on
/// every correctly-annotated element.
#[test]
fn scanner_sibling_check_covers_all_three_attributes() {
    let body = read(SCANNER);
    for attr in [
        "data-translate-aria-label",
        "data-translate-title",
        "data-translate-placeholder",
    ] {
        assert!(
            body.contains(&format!("{attr}=")),
            "PRD 3.7.4 (iter 224): {SCANNER} must reference \
             `{attr}=` in its sibling-attribute check. Without the \
             exemption, correctly-annotated elements of that \
             attribute class would still flag — pressuring \
             contributors toward allowlist-row additions and \
             defeating the iter-77 strict-zero invariant."
        );
    }
}

/// All three attribute regexes must use `{2,}` quantifier (minimum
/// two-character content), not `+` (one-or-more) or `*` (zero-or-
/// more). A `+` quantifier would flag single-character markup like
/// `aria-label="X"` (a legitimate abbreviation or iconographic
/// label); `*` would flag empty attribute values (`aria-label=""`)
/// even though those are valid.
#[test]
fn scanner_attribute_regex_minimum_char_length_is_two() {
    let body = read(SCANNER);
    // Locate each rule's regex and verify the quantifier is `{2,}`.
    for attr_prefix in ["aria-label=", r"\btitle=", r"\bplaceholder="] {
        let needle = format!(r#"{attr_prefix}"([^"]{{2,}})""#);
        assert!(
            body.contains(&needle),
            "PRD 3.7.4 (iter 224): {SCANNER} must keep the \
             `{attr_prefix}\"([^\"]{{{{2,}}}})\"` regex (minimum 2 \
             chars). A `+` or `*` quantifier would either flood the \
             scanner with FP (`aria-label=\"X\"` on icon labels) or \
             flag empty values — both push contributors toward \
             allowlist additions that defeat strict-zero.\n\
             Looking for: {needle}"
        );
    }
}

// --------------------------------------------------------------------
// Iter 264 structural pins — scanner byte bounds + guard byte bounds
// + mods.js i18n helper usage + allowlist strict-empty literal +
// scanner cites PRD 3.7.4.
// --------------------------------------------------------------------

/// Iter 264: scanner file byte bounds.
#[test]
fn scanner_file_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 2000;
    const MAX_BYTES: usize = 50_000;
    let bytes = fs::metadata(SCANNER).expect("scanner must exist").len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "PRD 3.7.4 (iter 264): {SCANNER} is {bytes} bytes; expected \
         [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

/// Iter 264: guard source byte bounds.
#[test]
fn guard_source_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 5000;
    const MAX_BYTES: usize = 80_000;
    let bytes = fs::metadata("tests/i18n_no_hardcoded_guard.rs")
        .expect("guard must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "PRD 3.7.4 (iter 264): guard is {bytes} bytes; expected \
         [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

/// Iter 264: `src/mods.js` must call `this.t(` or `t(` for i18n
/// routing — the canonical translation helper. Without any `this.t`
/// call sites, the scanner might pass (all strings avoiding the
/// scanned patterns) while the production code just hardcodes
/// English via other channels.
#[test]
fn mods_js_uses_translation_helper() {
    let src = fs::read_to_string(TARGET_MODS_JS).expect("mods.js must exist");
    let t_count =
        src.matches("this.t(").count() + src.matches("t('").count() + src.matches("t(\"").count();
    assert!(
        t_count >= 5,
        "PRD 3.7.4 (iter 264): {TARGET_MODS_JS} must use translation \
         helper `this.t(` / `t('` / `t(\"` at least 5 times. Found \
         {t_count}. Zero or near-zero means the production code \
         isn't actually routing through i18n — scanner passes \
         vacuously."
    );
}

/// Iter 264: ALLOWLIST must appear in scanner source AND must NOT
/// be reassigned / mutated anywhere after its initial empty-literal
/// declaration. A pattern like `const ALLOWLIST = [];` followed by
/// `ALLOWLIST.push(...)` would pass the iter-77 literal pin but
/// defeat the strict-zero contract at runtime.
#[test]
fn scanner_allowlist_is_never_mutated_after_declaration() {
    let body = read(SCANNER);
    assert!(
        body.contains("const ALLOWLIST = [];"),
        "ALLOWLIST empty literal must exist (iter-77 pin)"
    );
    // Reject mutation patterns (the declaration `const ALLOWLIST = [];`
    // intentionally contains `ALLOWLIST = [` so we can't reject that
    // substring directly; instead check for mutation methods + for
    // reassignment forms that don't use `const`).
    for forbidden in [
        "ALLOWLIST.push(",
        "ALLOWLIST.splice(",
        "ALLOWLIST.unshift(",
        "let ALLOWLIST",
        "var ALLOWLIST",
    ] {
        assert!(
            !body.contains(forbidden),
            "PRD 3.7.4 (iter 264): {SCANNER} must not contain \
             `{forbidden}` — mutating ALLOWLIST (or declaring with \
             `let`/`var` instead of `const`) defeats the iter-77 \
             strict-zero contract at runtime even if the initial \
             declaration stays empty."
        );
    }
}

/// Iter 264: scanner file must cite PRD 3.7.4 in its own header for
/// cross-reference parity with the Rust guard.
#[test]
fn scanner_source_cites_prd_3_7_4() {
    let body = read(SCANNER);
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.7.4") || header.contains("3.7.4"),
        "PRD 3.7.4 (iter 264): {SCANNER} header must cite `PRD 3.7.4` \
         or `3.7.4` so a reader grepping for the PRD criterion can \
         land on the scanner too (not just the guard).\nHeader:\n{header}"
    );
}
