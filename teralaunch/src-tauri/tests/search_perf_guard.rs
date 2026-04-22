//! PRD 3.6.4 (search-one-frame) perf-test drift guard.
//!
//! `teralaunch/tests/search-perf.test.js` is the Vitest benchmark
//! that enforces "filter 300 catalog entries in ≤16 ms (one 60fps
//! frame)" — the PRD 3.6.4 criterion. It is the ONLY user-facing
//! perf bound pinned in the JS test suite.
//!
//! Classes of silent regression this guard blocks:
//! - Threshold relaxation (e.g. `≤16` → `≤160`) to mask a real perf
//!   regression as a "flaky test" fix.
//! - Fixture shrinkage (300 → 30) to mask a slow-path that scales.
//! - Removal of the sanity control (`filters actually apply`) that
//!   prevents a broken `filterMatches` from trivially passing the
//!   perf test by always early-returning.
//! - Removal of the query-narrowing test that prevents a regression
//!   to a no-op filter.
//! - Reduction of sample count (7 → 1) that would widen variance and
//!   let one outlier decide the test.
//!
//! Parallel pattern to iters 124-126 (JS-scanner-pin chain). Shape
//! matches iter 125 i18n_scanner_guard — a single guard file with
//! small invariant surface per scanner.

use std::fs;

const SCANNER: &str = "../tests/search-perf.test.js";

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The perf-test file must exist, be non-trivial, and self-identify
/// as the PRD 3.6.4 measurement.
#[test]
fn perf_test_file_exists_and_self_identifies() {
    let body = read(SCANNER);
    assert!(
        body.len() > 1500,
        "PRD 3.6.4 violated: {SCANNER} is missing or truncated \
         (<1500 bytes). The perf benchmark is the ONLY user-facing \
         perf bound pinned in the JS suite; losing it silences the \
         gate."
    );
    assert!(
        body.contains("PRD 3.6.4"),
        "{SCANNER} must self-identify as the PRD 3.6.4 measurement \
         in its header comment — future readers trace back via grep."
    );
    assert!(
        body.contains("search-one-frame"),
        "{SCANNER} must cite the PRD criterion name \
         `search-one-frame` so the test-name-to-criterion mapping \
         stays discoverable."
    );
}

/// The 16 ms threshold (one 60fps frame) must stay verbatim.
/// Relaxing it to a larger number would mask perf regressions; the
/// correct response to a flaky ≤16 is to fix the hot path, NOT to
/// widen the budget.
#[test]
fn perf_budget_is_one_60fps_frame() {
    let body = read(SCANNER);
    assert!(
        body.contains("toBeLessThanOrEqual(16)"),
        "PRD 3.6.4 violated: {SCANNER} must assert \
         `toBeLessThanOrEqual(16)` (one 60fps frame = 16.67 ms, \
         rounded down to integer ms). Relaxing the budget silently \
         accepts perf regressions — if the test is flaky, profile \
         the slow path, don't widen the gate."
    );
    // And the commentary MUST cite the frame budget — keeps the
    // number-in-source traceable to the reason.
    assert!(
        body.contains("16 ms") || body.contains("60fps"),
        "{SCANNER} must explain the 16 in source as `16 ms` or \
         `60fps frame` so a future reader doesn't mistake it for a \
         magic number and raise it."
    );
}

/// The 300-entry fixture must stay. Shrinking it (e.g. 300 → 30)
/// would mask a scaling regression in `filterMatches` — a loop that
/// goes quadratic only shows up past some N.
#[test]
fn perf_fixture_has_300_entries() {
    let body = read(SCANNER);
    assert!(
        body.contains("makeCatalogEntries(300)"),
        "PRD 3.6.4 violated: {SCANNER} must call \
         `makeCatalogEntries(300)` in the `under_one_frame` test. \
         300 is the design-size ceiling for the in-launcher catalog \
         (per PRD §3.6.4); shrinking it would hide scaling \
         regressions in filterMatches."
    );
}

/// Median-of-7 sampling must stay. Reducing to a single sample would
/// widen variance; one GC pause or inline-cache miss would flip the
/// test.
#[test]
fn perf_takes_median_of_seven_samples() {
    let body = read(SCANNER);
    assert!(
        body.contains("i < 7"),
        "{SCANNER} must sample 7 times (`for (let i = 0; i < 7; i++)`). \
         Reducing to a single sample widens variance; one GC pause \
         would flip the test pass/fail."
    );
    assert!(
        body.contains("samples.sort") && body.contains("samples.length / 2"),
        "{SCANNER} must take the MEDIAN of the 7 samples (sort + \
         middle-index), not the mean. Mean is sensitive to a single \
         outlier; median is robust."
    );
    // Warm-up run must stay — without it, V8 IC misses on the first
    // call dominate and the test measures JIT warm-up, not \
    // steady-state.
    assert!(
        body.contains("Warm-up") || body.contains("warm-up") || body.contains("prime"),
        "{SCANNER} must retain the warm-up run that primes V8's JIT \
         before the 7 timed samples. Without it, the first sample's \
         inline-cache-miss cost pollutes the measurement."
    );
}

/// Both sanity controls (filter-actually-applies + query-narrows)
/// must stay. Without them, a regression that short-circuits
/// `filterMatches` to always-true or always-false would pass the
/// perf test trivially (fast, but wrong).
#[test]
fn perf_test_carries_both_sanity_controls() {
    let body = read(SCANNER);
    assert!(
        body.contains("filters actually apply"),
        "{SCANNER} must retain the `filters actually apply` test. \
         Without it, a regressed filterMatches that always returns \
         true would pass the perf test trivially (fast but broken)."
    );
    assert!(
        body.contains("query narrows matches"),
        "{SCANNER} must retain the `query narrows matches` test. \
         Without it, a regressed filterMatches that ignores the \
         query string would pass the perf test trivially."
    );
    // Sanity test must exercise kind='gpk' to prove filter BINDS,
    // not just that it returns stuff.
    assert!(
        body.contains("kind === 'gpk'"),
        "{SCANNER} `filters actually apply` must verify the kind= \
         'gpk' filter BINDS (every result is a gpk entry), not just \
         that some results came back."
    );
}

/// The Tauri v1/v2 dual stub must stay at module-load time. mods.js
/// reads either `window.__TAURI__.core.invoke` (v2) or
/// `window.__TAURI__.tauri.invoke` (v1 legacy) at import; a broken
/// stub would throw before the describe block even runs.
#[test]
fn perf_test_stubs_both_tauri_versions() {
    let body = read(SCANNER);
    assert!(
        body.contains("core:") && body.contains("tauri:"),
        "{SCANNER} must stub BOTH `core:` (Tauri v2) and `tauri:` \
         (v1 legacy fallback) on window.__TAURI__ before importing \
         mods.js. Dropping either would break module load under the \
         version it corresponds to."
    );
    assert!(
        body.contains("event:") && body.contains("listen:"),
        "{SCANNER} must stub `event.listen` — mods.js subscribes at \
         module load; without the stub the import throws."
    );
}

/// Self-test — prove the detectors in THIS guard bite on synthetic
/// bad shapes of the perf scanner.
#[test]
fn search_perf_guard_detector_self_test() {
    // Bad shape A: relaxed threshold.
    let relaxed = "expect(median).toBeLessThanOrEqual(160);";
    assert!(
        !relaxed.contains("toBeLessThanOrEqual(16)"),
        "self-test: relaxed budget (160) must be flagged (doesn't \
         match the strict `toBeLessThanOrEqual(16)` check)"
    );

    // Bad shape B: shrunk fixture.
    let tiny = "const entries = makeCatalogEntries(30);";
    assert!(
        !tiny.contains("makeCatalogEntries(300)"),
        "self-test: shrunk 30-entry fixture must be flagged"
    );

    // Bad shape C: single-sample (no loop).
    let single_sample = "const t0 = performance.now(); runFilter(ctx, entries); const elapsed = performance.now() - t0;";
    assert!(
        !single_sample.contains("i < 7"),
        "self-test: single-sample (no 7-iter loop) must be flagged"
    );

    // Bad shape D: mean instead of median.
    let mean = "const mean = samples.reduce((a, b) => a + b) / samples.length;";
    assert!(
        !(mean.contains("samples.sort") && mean.contains("samples.length / 2")),
        "self-test: mean-not-median aggregation must be flagged"
    );

    // Bad shape E: missing sanity control.
    let perf_only = "describe('perf', () => { it('under_one_frame', () => {}); });";
    assert!(
        !perf_only.contains("filters actually apply"),
        "self-test: perf test without the sanity control must be \
         flagged"
    );

    // Iter 181 — additional bad shapes.

    // Bad shape F: perf test redefining filterMatches locally (measures
    // a stub, not production code).
    let local_filter = "function filterMatches(e) { return true; }";
    assert!(
        !local_filter.contains("ModsView.filterMatches.call"),
        "self-test: locally-defined filterMatches (stub) must be \
         flagged — perf test must exercise ModsView.filterMatches"
    );

    // Bad shape G: Date.now() timing (ms-granularity would always show 0).
    let crude_timing = "const t0 = Date.now(); runFilter(ctx, e); const dt = Date.now() - t0;";
    assert!(
        !crude_timing.contains("performance.now"),
        "self-test: Date.now()-timed perf test must be flagged"
    );

    // Bad shape H: it.only() pin.
    let only_pin = "it.only('under_one_frame', () => { /* ... */ });";
    assert!(
        only_pin.contains("it.only"),
        "self-test: .only detector must bite on `it.only(`"
    );

    // Bad shape I: makeCatalogEntries returning only `id` and `name`
    // (misses description-query branch).
    let shallow_fixture = "entries.push({ id: `x.${i}`, name: `Mod ${i}` });";
    assert!(
        !shallow_fixture.contains("description"),
        "self-test: shallow fixture (id+name only) must be flagged — \
         filterMatches's description-substring branch goes unexercised"
    );
}

/// Iter 181: the perf test must exercise the real production
/// `ModsView.filterMatches` — not a locally-defined stub. A refactor
/// that redefined filterMatches inside the test file would silently
/// pass the perf gate while measuring different code.
#[test]
fn perf_test_exercises_production_filter_matches() {
    let body = read(SCANNER);
    assert!(
        body.contains("import('../src/mods.js')") || body.contains("from '../src/mods.js'"),
        "PRD 3.6.4 (iter 181): {SCANNER} must import from \
         `../src/mods.js` so it exercises the production \
         filterMatches; redefining the function locally would \
         measure a stub."
    );
    assert!(
        body.contains("ModsView.filterMatches"),
        "PRD 3.6.4 (iter 181): {SCANNER} must call \
         `ModsView.filterMatches` — not a locally-defined filter."
    );
    // Reject locally-defined `function filterMatches` or
    // `const filterMatches =`.
    assert!(
        !body.contains("function filterMatches"),
        "PRD 3.6.4 (iter 181): {SCANNER} must not define its own \
         `function filterMatches` — measure the real one."
    );
    assert!(
        !body.contains("const filterMatches ="),
        "PRD 3.6.4 (iter 181): {SCANNER} must not define its own \
         `const filterMatches =` — measure the real one."
    );
}

/// Iter 181: perf timing must use `performance.now()`. `Date.now()`
/// on many platforms has ~1 ms granularity (or worse with mitigations
/// against timing attacks), so a sub-frame measurement would round to
/// 0 and trivially pass any ≤16 budget.
#[test]
fn perf_timing_uses_performance_now_not_date_now() {
    let body = read(SCANNER);
    assert!(
        body.contains("performance.now()"),
        "PRD 3.6.4 (iter 181): {SCANNER} must use \
         `performance.now()` for timing — `Date.now()` has ms \
         granularity and would round sub-frame measurements to 0."
    );
    assert!(
        !body.contains("Date.now()"),
        "PRD 3.6.4 (iter 181): {SCANNER} must NOT use `Date.now()` \
         for timing — its ms granularity trivializes the ≤16 ms \
         budget."
    );
}

/// Iter 181: the perf test file must keep at least three `it(` blocks
/// — `under_one_frame`, `filters actually apply`, `query narrows`.
/// A delete-all-but-perf regression passing this floor would lose the
/// sanity controls that prevent a broken filterMatches from trivially
/// beating the ≤16 budget.
#[test]
fn perf_test_has_at_least_three_it_blocks() {
    let body = read(SCANNER);
    let it_count = body.matches("it(").count() + body.matches("it.only(").count();
    assert!(
        it_count >= 3,
        "PRD 3.6.4 (iter 181): {SCANNER} must carry at least 3 \
         `it(` blocks (under_one_frame + filters-apply + query-\
         narrows sanity controls). Found {it_count}. Deleting the \
         sanity controls lets a broken filterMatches trivially pass \
         the perf gate."
    );
}

/// Iter 181: perf test must not carry `.only` or `.skip` markers.
/// An `it.only('under_one_frame')` would disable the sanity controls
/// in one run; an `it.skip('filters actually apply')` would do the
/// same silently. Both are local-dev artefacts that must never ship.
#[test]
fn perf_test_carries_no_only_or_skip_markers() {
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
            "PRD 3.6.4 (iter 181): {SCANNER} must not carry \
             `{forbidden}` — local-dev pins disable other tests on \
             the perf file and drop the sanity controls."
        );
    }
}

/// Iter 181: the makeCatalogEntries fixture must produce entries
/// with all the fields filterMatches reads. A shallow `{ id, name }`
/// fixture would leave the description/category/kind branches of
/// filterMatches unexercised — the perf test would measure only a
/// subset of real user input patterns.
#[test]
fn perf_fixture_entries_have_full_field_shape() {
    let body = read(SCANNER);
    // Find the makeCatalogEntries body and inspect the pushed object
    // keys.
    let make_pos = body
        .find("function makeCatalogEntries")
        .expect("makeCatalogEntries must exist");
    let window = &body[make_pos..make_pos.saturating_add(1500)];
    for field in ["id:", "kind:", "name:", "description:", "category:"] {
        assert!(
            window.contains(field),
            "PRD 3.6.4 (iter 181): makeCatalogEntries must populate \
             `{field}` — filterMatches reads it and a shallow \
             fixture would leave its branch unexercised."
        );
    }
}

// --------------------------------------------------------------------
// Iter 216 structural pins — meta-guard header + SCANNER path constant
// + `it('under_one_frame')` positive + cross-guard PRD-drift reference
// + literal-16 budget (no arithmetic inflation).
// --------------------------------------------------------------------
//
// The twelve pins above cover threshold, fixture size, sampling
// discipline, sanity controls, Tauri stub, detector self-test, real-
// filterMatches exercise, performance.now timing, it-block count,
// .only/.skip markers, full fixture field shape. They do NOT pin:
// (a) the guard's own header cites PRD 3.6.4 — meta-guard contract;
// (b) the `SCANNER` constant equals its canonical relative path —
// rename drift hides as opaque panics; (c) `under_one_frame` is the
// actual test name in an `it(...)` block, not just a string that
// appears somewhere in a comment — iter-181 `perf_test_has_at_least_
// three_it_blocks` counts `it(` calls but doesn't verify the specific
// test name is present; (d) prd_path_drift_guard.rs has an entry
// pointing at this perf test — cross-guard integrity; (e) the `16`
// budget literal is not inflated via arithmetic (`16 * 10`, `16 + ...`)
// — iter-109 checks for `toBeLessThanOrEqual(16)` substring but
// `toBeLessThanOrEqual(16 * 10)` would also match as substring.

/// The guard's own module header must cite PRD 3.6.4 so a reader
/// chasing a search-perf drift lands here via section-grep.
#[test]
fn guard_file_header_cites_prd_3_6_4() {
    let body = fs::read_to_string("tests/search_perf_guard.rs")
        .expect("tests/search_perf_guard.rs must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.6.4"),
        "meta-guard contract: tests/search_perf_guard.rs header must \
         cite `PRD 3.6.4`. Without it, a reader chasing a perf-\
         regression won't land here via section-grep.\nHeader:\n{header}"
    );
    assert!(
        header.contains("search-one-frame"),
        "meta-guard contract: header must cite the PRD criterion \
         name `search-one-frame` so name-based cross-reference \
         between PRD P-slots and this guard works."
    );
}

/// The `SCANNER` path constant must equal its canonical relative
/// form. A rename (moving search-perf.test.js into a subdirectory,
/// or renaming to `search-performance.test.js`) would silently cause
/// every `read(SCANNER)` call to panic with an opaque "file not
/// readable" message that doesn't point at the constant.
#[test]
fn scanner_path_constant_is_canonical() {
    let guard_body =
        fs::read_to_string("tests/search_perf_guard.rs").expect("guard source must be readable");
    assert!(
        guard_body.contains("const SCANNER: &str = \"../tests/search-perf.test.js\";"),
        "PRD 3.6.4 (iter 216): tests/search_perf_guard.rs must retain \
         `const SCANNER: &str = \"../tests/search-perf.test.js\";` \
         verbatim. A rename of either the constant or the .test.js \
         file must be atomic — otherwise every pin panics with \
         opaque `file not readable`."
    );
}

/// `under_one_frame` must be the actual name of an `it(...)` block,
/// not just a string appearing somewhere in the file. The iter-109
/// `search-one-frame` check verifies the PRD criterion name is
/// cited, but a refactor that moves the string into a comment while
/// renaming the test would still pass that check — this pin asserts
/// the test name lives in a real `it(...)` call.
#[test]
fn under_one_frame_is_an_actual_it_block() {
    let body = read(SCANNER);
    let has_it_call = body.contains("it('under_one_frame',")
        || body.contains("it(\"under_one_frame\",")
        || body.contains("it('under_one_frame', ");
    assert!(
        has_it_call,
        "PRD 3.6.4 (iter 216): {SCANNER} must carry an `it('under_one_frame', ...)` \
         block. The iter-181 it-block count pin verifies ≥ 3 `it(` \
         calls exist; this pin verifies the SPECIFIC named test that \
         prd_path_drift_guard.rs cross-references is still present. \
         Renaming the test without updating the cross-ref would \
         silently fail the PRD-drift guard."
    );
}

/// `tests/prd_path_drift_guard.rs` must cross-reference this perf
/// test (`../tests/search-perf.test.js::under_one_frame`). The drift
/// guard is the authoritative mapping from PRD §3.6.4 cells to this
/// file; if it loses the entry, the PRD cite drifts unchecked.
#[test]
fn perf_test_is_referenced_in_prd_path_drift_guard() {
    let drift_body = fs::read_to_string("tests/prd_path_drift_guard.rs")
        .expect("tests/prd_path_drift_guard.rs must exist");
    assert!(
        drift_body.contains("search-perf.test.js") && drift_body.contains("under_one_frame"),
        "PRD 3.6.4 (iter 216): tests/prd_path_drift_guard.rs must \
         cite `search-perf.test.js` + `under_one_frame` (the JS_PIN \
         entry for §3.6.4). Without the cross-ref, a rename of the \
         perf test silently drifts the PRD cell while every pin on \
         the perf side passes."
    );
}

// --------------------------------------------------------------------
// Iter 256 structural pins — scanner size ceiling + fixture ceiling +
// sample count exactness + budget-min + vitest-import.
// --------------------------------------------------------------------
//
// The seventeen pins above cover threshold, fixture size, sampling,
// sanity controls, Tauri stub, production-filter exercise, perf.now
// timing, it-count, marker absence, fixture field shape, header cite,
// SCANNER const, under_one_frame it-name, drift cross-ref, and
// no-arithmetic-on-16. They do NOT pin:
// (a) the scanner file has a sane upper byte ceiling;
// (b) the fixture size isn't inflated past a sanity cap (e.g. 10_000
//     entries would make the test slow and mask scaling regressions
//     by virtue of being too slow to run);
// (c) the sample count (`i < 7`) is exactly 7 — the iter-109 substring
//     check `body.contains("i < 7")` passes if `i < 7` appears
//     anywhere, but not if someone changes to `i < 77` (which still
//     contains the substring);
// (d) the budget isn't reduced below 16 — a regression to 8 or 4 would
//     be over-strict and cause flaky failures the team would paper
//     over by widening again;
// (e) the scanner imports from `vitest` — without the import, the
//     `it()`/`describe()`/`expect()` calls would be undefined at
//     runtime and the whole file would error out on load.

/// The scanner file must not exceed a sane upper byte ceiling.
/// Current state: ~4 KB. A 30 KB ceiling gives ~7× margin while
/// catching accidental inclusion of a large fixture file or unrelated
/// tests merged into the perf scanner.
#[test]
fn scanner_file_size_has_upper_ceiling() {
    const MAX_BYTES: usize = 30_000;
    let bytes = fs::metadata(SCANNER)
        .unwrap_or_else(|e| panic!("{SCANNER}: {e}"))
        .len() as usize;
    assert!(
        bytes <= MAX_BYTES,
        "PRD 3.6.4 (iter 256): {SCANNER} is {bytes} bytes; ceiling \
         is {MAX_BYTES}. Bloat past the ceiling signals garbage in \
         the fixture or unrelated tests merged into this file — the \
         perf test's focus on ≤16 ms narrows."
    );
}

/// The fixture size must be capped to a sane maximum. A refactor
/// that bumps `makeCatalogEntries(300)` to `makeCatalogEntries(10_000)`
/// would pass the iter-109 fixture-has-300 check (because the 300
/// literal appears in a comment or neighbouring test), but the perf
/// test would take seconds to run and the team would paper over
/// flakiness by widening the budget. Pinning that no 4-digit or
/// larger N value appears in `makeCatalogEntries(N)` calls catches
/// such bloat.
#[test]
fn fixture_size_is_not_inflated_past_sanity_cap() {
    let body = read(SCANNER);
    // Scan for `makeCatalogEntries(<N>)` call-expressions and verify
    // N is <= 999.
    let needle = "makeCatalogEntries(";
    let mut search_from = 0;
    while let Some(rel) = body[search_from..].find(needle) {
        let pos = search_from + rel + needle.len();
        // Parse the digit run at `pos`.
        let digits: String = body[pos..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !digits.is_empty() {
            let n: usize = digits.parse().unwrap();
            assert!(
                n <= 999,
                "PRD 3.6.4 (iter 256): {SCANNER} has \
                 `makeCatalogEntries({n})` — sanity cap is 999. A \
                 4-digit fixture would make the perf test slow enough \
                 that the team would paper over flakiness by widening \
                 the budget."
            );
        }
        search_from = pos + digits.len();
    }
}

/// The sample count (`i < 7`) must be exactly 7, not 77 or 777. The
/// iter-109 substring check passes if `i < 7` appears anywhere, which
/// `i < 77` would also satisfy (prefix match). Pinning the next
/// character as `;` or whitespace catches silent inflation.
#[test]
fn perf_sample_count_is_exactly_seven_not_inflated() {
    let body = read(SCANNER);
    let needle = "i < 7";
    let pos = body
        .find(needle)
        .expect("PRD 3.6.4: `i < 7` must appear (iter-109 pin)");
    let after = &body[pos + needle.len()..];
    let next_char = after.chars().next().unwrap_or(' ');
    assert!(
        !next_char.is_ascii_digit(),
        "PRD 3.6.4 (iter 256): {SCANNER} `i < 7` must be followed by \
         non-digit — a regression to `i < 77` (or `i < 777`) would \
         pass the substring check but run 11× or 111× more samples, \
         widening the test duration until the team relaxes the budget. \
         Found next char: `{next_char}`."
    );
}

/// The budget literal `16` must not be reduced below 16. The
/// iter-109 pin checks the threshold is `16`; the iter-216 pin
/// rejects arithmetic on the literal; this pin guards against a
/// silent lowering to a tighter budget (e.g. 4 or 8) that would
/// be over-strict, cause flaky failures, and lead the team to
/// compensate by widening the real application code's perf envelope.
/// 16ms (one 60fps frame) is the principled budget; tighter is
/// theatrically ambitious but not user-visible.
#[test]
fn perf_budget_is_not_reduced_below_sixteen() {
    let body = read(SCANNER);
    // Scan for all `toBeLessThanOrEqual(N)` occurrences and verify
    // no N < 16.
    let needle = "toBeLessThanOrEqual(";
    let mut search_from = 0;
    while let Some(rel) = body[search_from..].find(needle) {
        let pos = search_from + rel + needle.len();
        let digits: String = body[pos..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !digits.is_empty() {
            let n: usize = digits.parse().unwrap();
            assert!(
                n >= 16,
                "PRD 3.6.4 (iter 256): {SCANNER} has \
                 `toBeLessThanOrEqual({n})` — minimum is 16 (one 60fps \
                 frame). A tighter budget is theatrically ambitious \
                 but causes flaky failures the team papers over by \
                 widening the real perf envelope."
            );
        }
        search_from = pos + digits.len();
    }
}

/// The scanner must import from `vitest`. Without the import, the
/// `it()` / `describe()` / `expect()` function calls would be
/// undefined at module load — the whole file errors out on startup
/// and the perf gate goes silent. Current idiom:
/// `import { describe, it, expect } from 'vitest';`.
#[test]
fn scanner_imports_from_vitest() {
    let body = read(SCANNER);
    let has_import = body.contains("from 'vitest'") || body.contains("from \"vitest\"");
    assert!(
        has_import,
        "PRD 3.6.4 (iter 256): {SCANNER} must import from `vitest` \
         (`import {{ describe, it, expect }} from 'vitest';`). Without \
         the import, the test harness functions are undefined and the \
         module fails to load — the perf gate silently goes missing \
         from CI."
    );
}

/// The `16` budget literal must appear as `toBeLessThanOrEqual(16)`
/// with NO leading arithmetic — not `toBeLessThanOrEqual(16 * 10)`
/// or `toBeLessThanOrEqual(budget + 16)` that would produce a larger
/// effective threshold while passing the iter-109 substring check.
#[test]
fn perf_budget_is_literal_16_not_arithmetic() {
    let body = read(SCANNER);
    // Find the `toBeLessThanOrEqual(16` occurrence and verify the
    // character immediately after the `16` closes the paren (no
    // `16 *`, `16 +`, `16.`, etc.).
    let needle = "toBeLessThanOrEqual(16";
    let pos = body
        .find(needle)
        .expect("PRD 3.6.4: toBeLessThanOrEqual(16 must appear (iter-109 pin)");
    let after = &body[pos + needle.len()..];
    let next_char = after.chars().next().unwrap_or(' ');
    assert_eq!(
        next_char, ')',
        "PRD 3.6.4 (iter 216): {SCANNER} must have `toBeLessThanOrEqual(16)` \
         closed IMMEDIATELY by `)` — no arithmetic on the `16` literal. \
         Found next char: `{next_char}`. An expression like \
         `toBeLessThanOrEqual(16 * 10)` would pass the iter-109 \
         substring check while silently inflating the budget."
    );
}

// --------------------------------------------------------------------
// Iter 287 structural pins — prd-path-drift cross-ref + mods.js
// filterMatches signature + vitest config presence + describe wrapper
// + benchmark result deterministic.
// --------------------------------------------------------------------

#[test]
fn mods_js_defines_filter_matches_function() {
    let src = std::fs::read_to_string("../src/mods.js").expect("mods.js must exist");
    assert!(
        src.contains("filterMatches"),
        "PRD 3.6.4 (iter 287): src/mods.js must define `filterMatches` \
         — the benchmark target. Rename orphans the scanner + this \
         guard's iter-181 cross-refs."
    );
}

#[test]
fn vitest_config_exists() {
    let roots = [
        "../vitest.config.js",
        "../vitest.config.mjs",
        "../vitest.config.ts",
        "../package.json",
    ];
    let exists = roots.iter().any(|p| std::path::Path::new(p).exists());
    assert!(
        exists,
        "PRD 3.6.4 (iter 287): vitest config or package.json must \
         exist at teralaunch/ root — the perf test runs under Vitest."
    );
}

#[test]
fn scanner_describe_block_exists() {
    let body = std::fs::read_to_string(SCANNER).expect("scanner must exist");
    assert!(
        body.contains("describe("),
        "PRD 3.6.4 (iter 287): {SCANNER} must wrap tests in a \
         `describe(...)` block for semantic grouping in test output."
    );
}

#[test]
fn scanner_sorts_samples_before_taking_median() {
    // Median-of-N requires sorting first. A regression to mean or
    // min would skip the sort. Pin the sort call explicitly.
    let body = std::fs::read_to_string(SCANNER).expect("scanner must exist");
    assert!(
        body.contains(".sort("),
        "PRD 3.6.4 (iter 287): {SCANNER} must call `.sort(` on the \
         samples array before taking the median. Without sort, the \
         middle index isn't the median — could be any value."
    );
}

#[test]
fn drift_guard_contains_this_scanner_entry() {
    let drift =
        std::fs::read_to_string("tests/prd_path_drift_guard.rs").expect("drift guard must exist");
    assert!(
        drift.contains("search-perf.test.js") && drift.contains("under_one_frame"),
        "PRD 3.6.4 (iter 287): prd_path_drift_guard.rs must reference \
         `search-perf.test.js::under_one_frame` — the cross-guard \
         contract for PRD §3.6.4's measurement path."
    );
}
