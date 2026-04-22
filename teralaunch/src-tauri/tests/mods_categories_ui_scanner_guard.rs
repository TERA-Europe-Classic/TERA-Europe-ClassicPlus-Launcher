//! fix.mods-categories-ui (iter 85) scanner drift guard.
//!
//! `teralaunch/tests/mods-categories-ui.test.js` pins the iter-85
//! UX fix for the mods-modal filter strip. Before iter 85, kind
//! chips (All/External/GPK) were segmented-control rectangles and
//! category chips (All categories/Cosmetic/Effects/…) were pills,
//! creating a stylistically inconsistent "L-shape". Iter 85 unified
//! both into `.mods-filter-chip` pill geometry inside
//! `.mods-filters-row` separated by a thin divider.
//!
//! Classes of silent regression this guard blocks:
//! - Dropping the kind-first / divider / category-last DOM order
//!   assertion (would let a future refactor re-shuffle the groups).
//! - Dropping the "legacy class absent" assertion (would let the
//!   two-class world creep back in through a copy-paste).
//! - Weakening the scoped-click assertion (would let a global
//!   `.mods-filter-chip` click handler double-bind category chips
//!   to `setFilter(undefined)`).
//! - Removing the CSS-unification assertions (pill shape + active
//!   teal border) — visual regression.
//!
//! Seventh in the iter-124-to-131 JS-scanner-pin chain.

use std::fs;

const SCANNER: &str = "../tests/mods-categories-ui.test.js";
const MODS_HTML: &str = "../src/mods.html";
const MODS_JS: &str = "../src/mods.js";
const MODS_CSS: &str = "../src/mods.css";

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Scanner file must exist, be non-trivial, and self-identify as
/// fix.mods-categories-ui.
#[test]
fn mods_categories_scanner_exists_and_self_identifies() {
    let body = read(SCANNER);
    assert!(
        body.len() > 3000,
        "{SCANNER} is missing or truncated (<3000 bytes). The \
         iter-85 filter-strip fix invariants live here."
    );
    assert!(
        body.contains("fix.mods-categories-ui"),
        "{SCANNER} must cite fix-plan P-slot `fix.mods-categories-ui` \
         so grep finds the test-to-fix mapping."
    );
    assert!(
        body.contains("iter 85"),
        "{SCANNER} must cite `iter 85` (the iteration that shipped \
         the fix) so the test's existence traces back."
    );
}

/// The DOM order assertion (kind-group → divider → category-row)
/// must stay. A presence-only check would pass even if the divider
/// appeared before the kind group.
#[test]
fn scanner_pins_kind_divider_category_dom_order() {
    let body = read(SCANNER);
    assert!(
        body.contains("mods-filters-row"),
        "{SCANNER} must assert against the `.mods-filters-row` \
         container class — that's the unified strip shipped in \
         iter 85."
    );
    assert!(
        body.contains("mods-filters-divider"),
        "{SCANNER} must assert the `mods-filters-divider` element \
         is present — visually separates the two groups."
    );
    assert!(
        body.contains("id=\"mods-category-row\"") || body.contains("mods-category-row"),
        "{SCANNER} must assert the `#mods-category-row` container \
         is present inside the filters row."
    );
    // ORDER assertions (two .toBeLessThan calls for kind < divider
    // and divider < category).
    let lessthan_count = body.matches(".toBeLessThan").count();
    assert!(
        lessthan_count >= 2,
        "{SCANNER} must use at least 2 `.toBeLessThan` calls to \
         enforce ORDER (kind < divider < category). Found \
         {lessthan_count}. A presence-only check would pass against \
         a shuffled strip."
    );
}

/// The `.mods-category-chip` legacy-class absence assertion must
/// stay. This is the forward-compat guard: a copy-paste or revert
/// that brings the legacy class back would fail the test.
#[test]
fn scanner_asserts_legacy_class_absent() {
    let body = read(SCANNER);
    assert!(
        body.contains("legacy .mods-category-chip class is gone"),
        "{SCANNER} must assert the legacy `.mods-category-chip` \
         class is gone from ALL source files (HTML + JS + CSS). \
         Losing this assertion lets the old two-class split creep \
         back via copy-paste or revert."
    );
    // Must iterate over all three file types — HTML, JS, CSS.
    assert!(
        body.contains("MODS_HTML") && body.contains("MODS_JS") && body.contains("MODS_CSS"),
        "{SCANNER} must check all three source file types for the \
         legacy class (HTML + JS + CSS). Dropping any lets the \
         class linger in the unchecked file."
    );
}

/// The scoped-click assertion must stay. Without it, a global
/// `.mods-filter-chip` click handler would double-bind category
/// chips to `setFilter(undefined)` and break the category filter.
#[test]
fn scanner_pins_scoped_click_handler() {
    let body = read(SCANNER);
    assert!(
        body.contains(".mods-filter-group .mods-filter-chip"),
        "{SCANNER} must verify the kind-filter click handler is \
         scoped to `.mods-filter-group .mods-filter-chip`. An \
         unscoped selector would double-bind category chips."
    );
}

/// CSS unification assertions (pill shape + active teal border)
/// must stay. Visual regression otherwise — active state would \
/// revert to the old segmented-control look.
#[test]
fn scanner_pins_unified_css_styling() {
    let body = read(SCANNER);
    assert!(
        body.contains("border-radius: 999px"),
        "{SCANNER} must assert `border-radius: 999px` on the base \
         .mods-filter-chip — that's the pill geometry shared by \
         kind + category chips."
    );
    assert!(
        body.contains("padding: 4px 10px"),
        "{SCANNER} must assert `padding: 4px 10px` on the base chip \
         — losing this relaxes the spec and lets visual drift creep \
         in."
    );
    assert!(
        body.contains("font-size: 11px"),
        "{SCANNER} must assert `font-size: 11px` on the base chip."
    );
    // Active state teal border (rgba(34, 211, 238) per Tailwind
    // teal-400). Critical: this is what distinguishes "unified" from
    // "reverted to segmented-control".
    assert!(
        body.contains("border-color:")
            && body.contains("34")
            && body.contains("211")
            && body.contains("238"),
        "{SCANNER} must assert the active state's teal \
         `border-color: rgba(34, 211, 238, ...)`. Losing this lets \
         the active chip revert to the old border style."
    );
}

/// The one-active-chip-per-group seed-state assertion must stay.
/// Without it, an accidental `class=\"active\"` on a non-default
/// chip would still render but with wrong selection state.
#[test]
fn scanner_pins_single_active_chip_per_group() {
    let body = read(SCANNER);
    assert!(
        body.contains("exactly 1 kind chip starts active"),
        "{SCANNER} must assert exactly 1 kind chip starts active \
         in the HTML seed (prevents multi-active bug where two \
         chips visually look selected simultaneously)."
    );
    assert!(
        body.contains("exactly 1 category chip starts active"),
        "{SCANNER} must assert exactly 1 category chip starts \
         active in the HTML seed (same reason for the category \
         row)."
    );
    // Must emit exactly 3 kind chips (All/External/GPK).
    assert!(
        body.contains("three kind chips (All/External/GPK)"),
        "{SCANNER} must assert exactly 3 kind chips exist \
         (All/External/GPK). Dropping this lets a regression shrink \
         the kind-filter set without detection."
    );
}

/// Scanned reference files must exist and carry key markers.
#[test]
fn scanned_reference_files_carry_required_markers() {
    let html = read(MODS_HTML);
    assert!(
        html.contains("mods-filters-row"),
        "{MODS_HTML} must carry the `.mods-filters-row` container — \
         the unified filter strip from iter 85."
    );
    assert!(
        !html.contains("mods-category-chip"),
        "{MODS_HTML} must NOT contain the legacy class \
         `mods-category-chip` — its absence is the proof the \
         iter-85 refactor wasn't partial."
    );
    let js = read(MODS_JS);
    assert!(
        !js.contains("mods-category-chip"),
        "{MODS_JS} must NOT contain the legacy class."
    );
    let css = read(MODS_CSS);
    assert!(
        !css.contains("mods-category-chip"),
        "{MODS_CSS} must NOT contain the legacy class."
    );
    assert!(
        css.contains(".mods-filter-chip {"),
        "{MODS_CSS} must declare `.mods-filter-chip {{` — the \
         unified chip rule."
    );
}

/// Self-test — prove the detectors in THIS guard bite on synthetic
/// bad shapes of the scanner file.
#[test]
fn mods_categories_ui_scanner_guard_detector_self_test() {
    // Bad shape A: scanner with only one .toBeLessThan (presence-
    // only ordering check).
    let one_order = "expect(kindIdx).toBeLessThan(dividerIdx);";
    let count = one_order.matches(".toBeLessThan").count();
    assert!(
        count < 2,
        "self-test: scanner with only {count} .toBeLessThan must \
         be flagged (need ≥2 for kind < divider < category)"
    );

    // Bad shape B: scanner without legacy-class-absent assertion.
    let no_legacy_check = "expect(src.includes('mods-filter-chip')).toBe(true);";
    assert!(
        !no_legacy_check.contains("legacy .mods-category-chip class is gone"),
        "self-test: scanner without legacy-absent check must be \
         flagged"
    );

    // Bad shape C: scanner dropped the scoped-click assertion.
    let unscoped = "const clickHandler = js.includes('.mods-filter-chip');";
    assert!(
        !unscoped.contains(".mods-filter-group .mods-filter-chip"),
        "self-test: scanner without scoped-click assertion must \
         be flagged"
    );

    // Bad shape D: scanner without active-teal-border assertion.
    let no_teal = "expect(activeSlice).toContain('background: red');";
    assert!(
        !(no_teal.contains("34") && no_teal.contains("211") && no_teal.contains("238")),
        "self-test: scanner without teal-border assertion must be \
         flagged"
    );

    // Iter 186 — additional bad shapes.

    // Bad shape E: `.only` pin on the scanner.
    let only_pin = "it.only('kind chip order in mods.html', () => {});";
    assert!(
        only_pin.contains("it.only"),
        "self-test: .only detector must bite on `it.only(`"
    );

    // Bad shape F: mods.html with only 1 kind chip.
    let shrunken_html = r#"<button class="mods-filter-chip" data-filter="all">All</button>"#;
    let count_filters = shrunken_html.matches(r#"data-filter=""#).count();
    assert!(
        count_filters < 3,
        "self-test: shrunken-filter HTML must be flagged by a 3-chip \
         floor (found {count_filters})"
    );

    // Bad shape G: mods.css with a different chip rule.
    let broken_css = ".mods-filter-chip { border-radius: 4px; padding: 8px; }";
    assert!(
        !broken_css.contains("border-radius: 999px"),
        "self-test: chip rule with non-pill border-radius must be \
         flagged"
    );
}

/// Iter 186: scanner must carry at least 5 `it(` blocks. Below the
/// floor means one of the fix.mods-categories-ui invariants (kind
/// order + legacy absent + scoped click + unified CSS + single-
/// active) was deleted.
#[test]
fn mods_categories_scanner_has_minimum_it_count() {
    let body = read(SCANNER);
    let it_count = body.matches("it(").count() + body.matches("it.only(").count();
    assert!(
        it_count >= 5,
        "fix.mods-categories-ui (iter 186): {SCANNER} must carry at \
         least 5 `it(` blocks. Found {it_count}. Below the floor \
         means a fix-invariant test was deleted."
    );
}

/// Iter 186: reject dev-local `.only` / `.skip` / `xit` / `xdescribe`
/// markers — these silently disable sibling UX invariants in CI.
#[test]
fn mods_categories_scanner_carries_no_only_or_skip_markers() {
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
            "fix.mods-categories-ui (iter 186): {SCANNER} must not \
             carry `{forbidden}` — dev-local pins disable sibling \
             UX invariant tests in CI."
        );
    }
}

/// Iter 186: the production `src/mods.html` must carry exactly 3
/// `data-filter=` attributes on `.mods-filter-chip` buttons — the
/// All / External / GPK kind-chip trio. A shrink to 1 or 2 would
/// break the "all kinds visible" UX and the scanner's current
/// assertion ("three kind chips (All/External/GPK)") would still
/// pass against its internal fixture.
#[test]
fn mods_html_has_three_kind_chip_data_filter_attrs() {
    let html = read(MODS_HTML);
    let kind_chip_count = html.matches(r#"data-filter=""#).count();
    assert_eq!(
        kind_chip_count, 3,
        "fix.mods-categories-ui (iter 186): {MODS_HTML} must carry \
         exactly 3 `data-filter=` attributes (All / External / GPK). \
         Found {kind_chip_count}. A shrink breaks the kind-filter \
         UX; a growth means a kind was added without updating this \
         pin or the scanner's fixture."
    );
}

/// Iter 186: the production `src/mods.css` must keep the unified
/// `.mods-filter-chip` rule verbatim — pill border-radius, fixed
/// padding, fixed font-size. Drift in any of these three lets the
/// active chip visually drift back toward the old segmented-control
/// look even if the scanner's fixture stays current.
#[test]
fn mods_css_keeps_unified_chip_rule_verbatim() {
    let css = read(MODS_CSS);
    let decl_start = css
        .find(".mods-filter-chip {")
        .expect("mods.css must declare `.mods-filter-chip {`");
    let decl_end = css[decl_start..]
        .find('}')
        .map(|offset| decl_start + offset)
        .expect("mods-filter-chip declaration must close with `}`");
    let rule = &css[decl_start..decl_end];
    for needle in [
        "border-radius: 999px",
        "padding: 4px 10px",
        "font-size: 11px",
    ] {
        assert!(
            rule.contains(needle),
            "fix.mods-categories-ui (iter 186): {MODS_CSS} \
             .mods-filter-chip rule must carry `{needle}`. Missing \
             means the pill geometry drifted. Rule body:\n{rule}"
        );
    }
}

/// Iter 186: the production `src/mods.js` must keep the scoped-click
/// selector `.mods-filter-group .mods-filter-chip` — the unscoped
/// form (`.mods-filter-chip`) would double-bind category chips to
/// `setFilter(undefined)` and break the category filter. Real-file
/// pin complements the scanner's fixture check.
#[test]
fn mods_js_keeps_scoped_filter_click_selector() {
    let js = read(MODS_JS);
    assert!(
        js.contains(".mods-filter-group .mods-filter-chip"),
        "fix.mods-categories-ui (iter 186): {MODS_JS} must keep the \
         scoped click selector `.mods-filter-group .mods-filter-\
         chip`. An unscoped `.mods-filter-chip` selector double-\
         binds category chips to the kind-filter click handler."
    );
}

/// Iter 225: module header must self-identify as `fix.mods-categories-ui`
/// (iter 85) + "scanner drift guard" + enumerate "Classes of silent
/// regression". Without this meta-guard contract, a guard file rename
/// or purpose drift (e.g. narrowing to one invariant) could happen
/// silently and future authors would lose the map of what the guard
/// protects against.
#[test]
fn guard_module_header_cites_fix_slot_and_scanner_drift_contract() {
    let src = fs::read_to_string("tests/mods_categories_ui_scanner_guard.rs")
        .expect("tests/mods_categories_ui_scanner_guard.rs must exist");
    let header_end = src.find("use std::fs;").expect("file must `use std::fs;`");
    let header = &src[..header_end];
    for needle in [
        "fix.mods-categories-ui",
        "iter 85",
        "scanner drift guard",
        "Classes of silent regression",
    ] {
        assert!(
            header.contains(needle),
            "meta-guard contract: mods_categories_ui_scanner_guard.rs \
             header must carry `{needle}` — without it, a rename or \
             purpose drift happens silently."
        );
    }
}

/// Iter 225: the four path constants (SCANNER + MODS_HTML + MODS_JS +
/// MODS_CSS) must be pinned verbatim. A rename of any target file
/// (mods.html → mods-page.html, mods.test.js → mods.spec.js, etc.)
/// would surface as a generic "file not readable" panic and drift
/// diagnostics, not a clear drift signal. Pinning the canonical
/// literal gives the diff a loud anchor.
#[test]
fn scanner_and_target_path_constants_are_canonical() {
    let src = fs::read_to_string("tests/mods_categories_ui_scanner_guard.rs")
        .expect("tests/mods_categories_ui_scanner_guard.rs must exist");
    for line in [
        r#"const SCANNER: &str = "../tests/mods-categories-ui.test.js";"#,
        r#"const MODS_HTML: &str = "../src/mods.html";"#,
        r#"const MODS_JS: &str = "../src/mods.js";"#,
        r#"const MODS_CSS: &str = "../src/mods.css";"#,
    ] {
        assert!(
            src.contains(line),
            "canonical path constant missing: `{line}`. A rename of \
             any of these targets must surface as a guard update, not \
             a generic file-not-readable panic."
        );
    }
}

/// Iter 225: the scanner byte-floor literal `body.len() > 3000` must
/// stay. A relaxed floor (>100, >500) would let a stubbed-out scanner
/// pass `mods_categories_scanner_exists_and_self_identifies` while
/// silently dropping the iter-85 filter-strip invariants. The 3000
/// floor reflects current scanner size (~5kB) with headroom.
#[test]
fn scanner_byte_floor_literal_is_three_thousand() {
    let src = fs::read_to_string("tests/mods_categories_ui_scanner_guard.rs")
        .expect("tests/mods_categories_ui_scanner_guard.rs must exist");
    assert!(
        src.contains("body.len() > 3000"),
        "scanner byte-floor literal `body.len() > 3000` must stay. A \
         weaker floor lets a stubbed scanner pass the existence \
         check."
    );
}

/// Iter 225: the forbidden-markers list in
/// `mods_categories_scanner_carries_no_only_or_skip_markers` must
/// cover exactly six vitest disable variants: `it.only(`,
/// `describe.only(`, `it.skip(`, `describe.skip(`, `xit(`,
/// `xdescribe(`. Dropping any one (e.g. `xit` / `xdescribe`) would
/// silently allow that syntax to land in CI and disable sibling
/// scanner tests.
#[test]
fn forbidden_markers_list_covers_six_disable_variants() {
    let src = fs::read_to_string("tests/mods_categories_ui_scanner_guard.rs")
        .expect("tests/mods_categories_ui_scanner_guard.rs must exist");
    for marker in [
        r#""it.only(""#,
        r#""describe.only(""#,
        r#""it.skip(""#,
        r#""describe.skip(""#,
        r#""xit(""#,
        r#""xdescribe(""#,
    ] {
        assert!(
            src.contains(marker),
            "forbidden-markers list must contain {marker} — dropping \
             it lets that vitest disable-variant silently land in CI \
             and disable sibling scanner tests."
        );
    }
}

/// Iter 225: the detector self-test must carry BOTH the iter-85-era
/// synthetic bad shapes (labelled A–D: one-.toBeLessThan / no-legacy-
/// check / unscoped-click / no-teal-border) AND the iter-186-era
/// shapes (labelled E–G: .only-pin / shrunken-HTML / broken-CSS). If
/// a refactor deletes one era's shapes, the detector stops biting on
/// that class of regression silently.
#[test]
fn detector_self_test_carries_both_era_synthetic_shapes() {
    let src = fs::read_to_string("tests/mods_categories_ui_scanner_guard.rs")
        .expect("tests/mods_categories_ui_scanner_guard.rs must exist")
        .replace("\r\n", "\n");
    let fn_pos = src
        .find("fn mods_categories_ui_scanner_guard_detector_self_test()")
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
    for era_marker in [
        "Bad shape A",
        "Bad shape B",
        "Bad shape C",
        "Bad shape D",
        "Iter 186 — additional bad shapes",
        "Bad shape E",
        "Bad shape F",
        "Bad shape G",
    ] {
        assert!(
            body.contains(era_marker),
            "detector self-test must carry era marker `{era_marker}` \
             — dropping it silently stops the detector from biting on \
             that synthetic shape."
        );
    }
}

// --------------------------------------------------------------------
// Iter 265 structural pins — scanner/guard byte bounds + iter-85
// provenance + mods.css has .mods-filter-chip rule + scanner cites
// fix-plan slot.
// --------------------------------------------------------------------

/// Iter 265: scanner file byte bounds.
#[test]
fn scanner_file_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 2000;
    const MAX_BYTES: usize = 50_000;
    let bytes = fs::metadata(SCANNER).expect("scanner must exist").len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "fix.mods-categories-ui (iter 265): {SCANNER} is {bytes} \
         bytes; expected [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

/// Iter 265: guard source byte bounds.
#[test]
fn guard_source_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 5000;
    const MAX_BYTES: usize = 80_000;
    let bytes = fs::metadata("tests/mods_categories_ui_scanner_guard.rs")
        .expect("guard must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "fix.mods-categories-ui (iter 265): guard is {bytes} bytes; \
         expected [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

/// Iter 265: guard header must cite `iter 85` as the iteration that
/// shipped the UX unification fix.
#[test]
fn guard_header_cites_iter_85_provenance() {
    let body =
        fs::read_to_string("tests/mods_categories_ui_scanner_guard.rs").expect("guard must exist");
    let header = &body[..body.len().min(500)];
    assert!(
        header.contains("iter 85") || header.contains("iter-85"),
        "fix.mods-categories-ui (iter 265): guard header must cite \
         `iter 85` — the iteration that shipped the pill-unification \
         fix. Readers chasing the iter history need this.\n\
         Header:\n{header}"
    );
}

/// Iter 265: `src/mods.css` must carry a `.mods-filter-chip` rule —
/// the canonical pill class the whole fix uses. Without the rule,
/// the HTML carries the class but no styling applies.
#[test]
fn mods_css_carries_filter_chip_rule() {
    let css = fs::read_to_string(MODS_CSS).expect("mods.css must exist");
    assert!(
        css.contains(".mods-filter-chip"),
        "fix.mods-categories-ui (iter 265): {MODS_CSS} must carry a \
         `.mods-filter-chip` rule. Without the CSS, the HTML markup \
         gets the class but no pill styling applies — regression to \
         visually-unstyled chips."
    );
}

/// Iter 265: scanner must cite the fix-plan slot name in its own
/// header (not just the guard header). Cross-reference parity.
#[test]
fn scanner_source_cites_fix_mods_categories_ui_slot() {
    let body = fs::read_to_string(SCANNER).expect("scanner must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("fix.mods-categories-ui") || header.contains("mods-categories-ui"),
        "fix.mods-categories-ui (iter 265): {SCANNER} header must \
         cite `fix.mods-categories-ui` or `mods-categories-ui` for \
         slot-grep discoverability.\nHeader:\n{header}"
    );
}
