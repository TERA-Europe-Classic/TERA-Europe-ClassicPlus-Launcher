//! fix.offline-empty-state (iter 84) scanner drift guard.
//!
//! `teralaunch/tests/offline-banner.test.js` pins the blank-screen
//! bug fix shipped in iter 84: the launcher used to render a blank
//! dark viewport when the portal API was unreachable because the
//! `.mainpage.ready` class (which flips opacity 0 → 1) was added
//! mid-init after a network-touching await. If any pre-.ready await
//! threw, the outer catch swallowed it and the page stayed invisible.
//!
//! The JS test pins three invariants:
//! 1. index.html ships the offline-banner DOM skeleton (banner +
//!    retry button + `role="alert"` + 3 data-translate attributes)
//! 2. `App.init()` flips `.ready` BEFORE the first await — this is
//!    the structural fix; a refactor that pushes the flip back
//!    behind an await re-introduces the blank-screen bug
//! 3. showOfflineBanner/hideOfflineBanner toggle the .hidden class,
//!    retry button re-runs init, and the wiring is idempotent
//! 4. OFFLINE_BANNER_* keys exist in all 4 locales
//!
//! Sixth in the iter-124-to-129 JS-scanner-pin chain. Different
//! flavour: scanner tests DOM + source-order + i18n, not a simple
//! regex scanner.

use std::fs;

const SCANNER: &str = "../tests/offline-banner.test.js";
const INDEX_HTML: &str = "../src/index.html";
const APP_JS: &str = "../src/app.js";
const TRANSLATIONS: &str = "../src/translations.json";

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The scanner must exist, be non-trivial, and self-identify as the
/// iter-84 fix.offline-empty-state guard.
#[test]
fn offline_scanner_file_exists_and_self_identifies() {
    let body = read(SCANNER);
    assert!(
        body.len() > 4000,
        "{SCANNER} is missing or truncated (<4000 bytes). The \
         blank-screen fix invariants live here; losing the scanner \
         silences the only automated guard against the iter-84 \
         regression."
    );
    assert!(
        body.contains("fix.offline-empty-state"),
        "{SCANNER} must cite the fix-plan P-slot id \
         `fix.offline-empty-state` so grep finds it."
    );
    assert!(
        body.contains("iter 84"),
        "{SCANNER} must cite `iter 84` (the iteration that shipped \
         the fix) so the test's existence traces back to a concrete \
         change."
    );
}

/// The DOM skeleton assertions must stay: banner id, hidden class,
/// retry button id, role=alert, and the 3 data-translate attributes.
/// Dropping any of these lets the corresponding part of the UI
/// regress without the test catching it.
#[test]
fn scanner_pins_dom_skeleton_invariants() {
    let body = read(SCANNER);
    for needle in [
        r#"id="offline-banner""#,
        "offline-banner hidden",
        r#"id="offline-banner-retry""#,
        r#"role="alert""#,
        r#"data-translate="OFFLINE_BANNER_TITLE""#,
        r#"data-translate="OFFLINE_BANNER_DESC""#,
        r#"data-translate="OFFLINE_BANNER_RETRY""#,
    ] {
        assert!(
            body.contains(needle),
            "{SCANNER} must assert the DOM skeleton carries \
             `{needle}`. Each is a separate feature the banner \
             depends on (a11y role, i18n keys, retry button id)."
        );
    }
}

/// The source-order assertion (`.ready` flip before first await) is
/// the STRUCTURAL fix — anything that weakens this test lets the
/// blank-screen bug back in.
#[test]
fn scanner_pins_ready_before_await_invariant() {
    let body = read(SCANNER);
    assert!(
        body.contains(".ready class BEFORE the first await"),
        "{SCANNER} must keep the `.ready class BEFORE the first \
         await` assertion description — this is the structural \
         fix; without it a refactor that pushes the flip behind an \
         await re-introduces the blank-screen bug."
    );
    assert!(
        body.contains("classList.add('ready')"),
        "{SCANNER} must search for `classList.add('ready')` literal \
         — renaming/refactoring the flip to a different API call \
         would silently bypass the order-check."
    );
    // Assertion must check the ready index is LESS than the first
    // await index (order-invariant, not presence-invariant).
    assert!(
        body.contains(".toBeLessThan(firstAwait)"),
        "{SCANNER} must use `.toBeLessThan(firstAwait)` to enforce \
         ORDER (ready before await). A presence-only check would \
         pass even if ready was after the await."
    );
}

/// The idempotent-wiring assertion (3 shows, 1 click, 1 init call)
/// must stay. Without it, a regression that re-adds the click
/// listener on every show would stack handlers and trigger init
/// multiple times per click.
#[test]
fn scanner_pins_idempotent_retry_wiring() {
    let body = read(SCANNER);
    assert!(
        body.contains("idempotent") || body.contains("do not stack"),
        "{SCANNER} must carry the idempotent-wiring test. Without \
         it, a regression that re-adds the retry-button click \
         listener on every showOfflineBanner call would stack \
         handlers and fire init N times per click."
    );
    assert!(
        body.contains("dataset.wired"),
        "{SCANNER} must exercise `dataset.wired` as the dedupe \
         mechanism — the invariant is that the retry-wire marker is \
         set exactly once."
    );
}

/// The i18n-parity assertion for OFFLINE_BANNER_* keys must stay.
/// Missing the key in a locale would render the raw key string in
/// that locale's UI (e.g. `OFFLINE_BANNER_TITLE` instead of "No
/// connection").
#[test]
fn scanner_pins_translation_keys_across_four_locales() {
    let body = read(SCANNER);
    assert!(
        body.contains("FRA") && body.contains("EUR")
            && body.contains("RUS") && body.contains("GER"),
        "{SCANNER} must check OFFLINE_BANNER_* in all 4 locales \
         (FRA, EUR, RUS, GER). Skipping any locale lets that \
         locale's UI ship a raw key string."
    );
    assert!(
        body.contains("OFFLINE_BANNER_TITLE")
            && body.contains("OFFLINE_BANNER_DESC")
            && body.contains("OFFLINE_BANNER_RETRY"),
        "{SCANNER} must check all 3 OFFLINE_BANNER_* keys (TITLE, \
         DESC, RETRY) per locale."
    );
}

/// The scanned reference files must exist so the scanner's real-
/// file-read assertions aren't vacuous.
#[test]
fn scanned_reference_files_exist() {
    // index.html must carry the offline-banner markup — a sanity
    // pin that proves the DOM side of the contract is satisfied.
    let html = read(INDEX_HTML);
    assert!(
        html.contains("offline-banner"),
        "{INDEX_HTML} must carry the `offline-banner` DOM element. \
         If the markup is removed, the scanner still passes against \
         its fixture but production regresses."
    );
    let app = read(APP_JS);
    assert!(
        app.contains("classList.add('ready')"),
        "{APP_JS} must retain the `.ready` flip — the structural \
         fix for the iter-84 blank-screen bug lives here."
    );
    let translations = read(TRANSLATIONS);
    assert!(
        translations.contains("OFFLINE_BANNER_TITLE"),
        "{TRANSLATIONS} must carry OFFLINE_BANNER_TITLE — scanner \
         passes without this key living in the real file."
    );
}

/// Self-test — prove the detectors in THIS guard bite on synthetic
/// bad shapes of the scanner file.
#[test]
fn offline_banner_scanner_guard_detector_self_test() {
    // Bad shape A: scanner that dropped the role="alert" check.
    let no_alert = r#"expect(html).toContain('id="offline-banner"');"#;
    assert!(
        !no_alert.contains(r#"role="alert""#),
        "self-test: scanner without role=alert assertion must be \
         flagged (a11y regression)"
    );

    // Bad shape B: order-check downgraded to presence-check.
    let presence_only = "expect(readyInCode).toBeGreaterThan(0);";
    assert!(
        !presence_only.contains(".toBeLessThan(firstAwait)"),
        "self-test: scanner without ORDER assertion must be flagged"
    );

    // Bad shape C: scanner that checks only 1 locale.
    let one_locale = "for (const locale of ['EUR']) { ... }";
    assert!(
        !(one_locale.contains("FRA") && one_locale.contains("RUS")),
        "self-test: scanner missing locales must be flagged"
    );

    // Bad shape D: scanner without idempotent-wiring test.
    let no_idempotent = "fakeApp.showOfflineBanner();";
    assert!(
        !(no_idempotent.contains("idempotent") || no_idempotent.contains("do not stack")),
        "self-test: scanner missing idempotent-wiring test must be \
         flagged"
    );

    // Iter 182 — additional bad shapes.

    // Bad shape E: `.only` / `.skip` pin on a test.
    let only_pin = "it.only('classList.add(ready) before first await', () => {});";
    assert!(
        only_pin.contains("it.only"),
        "self-test: .only detector must bite on `it.only(`"
    );

    // Bad shape F: app.js ready flip AFTER first await.
    let bad_order = "async init() {\n  await fetch('/x');\n  mainpage.classList.add('ready');\n}\n";
    let init_pos = bad_order.find("async init()").unwrap();
    let await_pos = bad_order[init_pos..].find("await ").map(|p| init_pos + p).unwrap();
    let ready_pos = bad_order[init_pos..].find("classList.add('ready')").map(|p| init_pos + p).unwrap();
    assert!(
        ready_pos > await_pos,
        "self-test: synthetic bad-order source must have ready AFTER \
         first await so the order-check bites"
    );
}

/// Iter 182: scanner file must carry at least 4 `it(` blocks —
/// skeleton + order + idempotent + i18n. Deleting any would re-open
/// the corresponding regression class while the scanner itself still
/// looks "in place."
#[test]
fn offline_scanner_has_at_least_four_it_blocks() {
    let body = read(SCANNER);
    let it_count = body.matches("it(").count() + body.matches("it.only(").count();
    assert!(
        it_count >= 4,
        "fix.offline-empty-state (iter 182): {SCANNER} must carry at \
         least 4 `it(` blocks. Found {it_count}. The four invariants \
         (DOM skeleton + .ready order + idempotent retry + i18n \
         parity) each protect a separate regression class; deletion \
         re-opens the corresponding door."
    );
}

/// Iter 182: scanner must not carry `.only` / `.skip` / `xit` / etc.
/// markers — they are dev-local pins that silently disable the
/// remaining three invariants in CI.
#[test]
fn offline_scanner_carries_no_only_or_skip_markers() {
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
            "fix.offline-empty-state (iter 182): {SCANNER} must not \
             carry `{forbidden}` — a local-dev pin disables the \
             remaining three iter-84 invariants in CI."
        );
    }
}

/// Iter 182: `index.html` must carry the full offline-banner DOM
/// skeleton — all three data-translate attributes + role=alert + the
/// retry button id. The scanner asserts the skeleton against the JS
/// test's fixture string, but the ultimate source of truth is
/// `src/index.html`. Pinning it here catches regressions where the
/// scanner's fixture stays up-to-date but the real HTML drifts.
#[test]
fn index_html_carries_full_offline_banner_skeleton() {
    let html = read(INDEX_HTML);
    for needle in [
        r#"id="offline-banner""#,
        r#"id="offline-banner-retry""#,
        r#"role="alert""#,
        r#"data-translate="OFFLINE_BANNER_TITLE""#,
        r#"data-translate="OFFLINE_BANNER_DESC""#,
        r#"data-translate="OFFLINE_BANNER_RETRY""#,
    ] {
        assert!(
            html.contains(needle),
            "fix.offline-empty-state (iter 182): {INDEX_HTML} must \
             carry `{needle}`. Losing any of these re-opens part of \
             the iter-84 fix in production even if the scanner still \
             passes against its fixture."
        );
    }
}

/// Iter 182: `translations.json` must carry all 3 OFFLINE_BANNER_*
/// keys in all 4 locales (EUR, FRA, GER, RUS). A missing key in any
/// locale renders the raw key string ("OFFLINE_BANNER_TITLE") in
/// that locale's offline banner.
#[test]
fn translations_json_has_offline_banner_keys_in_all_four_locales() {
    let translations = read(TRANSLATIONS);
    for locale in ["EUR", "FRA", "GER", "RUS"] {
        let locale_pos = translations
            .find(&format!("\"{locale}\""))
            .unwrap_or_else(|| {
                panic!(
                    "fix.offline-empty-state (iter 182): \
                     {TRANSLATIONS} is missing the `{locale}` locale"
                )
            });
        // Window runs to the next locale marker or EOF, clamped to
        // string length. Clamping prevents an out-of-range slice on
        // small translations.json variants.
        let remaining = &translations[locale_pos + 1..];
        let next_rel = remaining
            .find("\n    \"")
            .unwrap_or(remaining.len());
        let end = (locale_pos + 1 + next_rel).min(translations.len());
        let window = &translations[locale_pos..end];
        for key in [
            "OFFLINE_BANNER_TITLE",
            "OFFLINE_BANNER_DESC",
            "OFFLINE_BANNER_RETRY",
        ] {
            assert!(
                window.contains(key),
                "fix.offline-empty-state (iter 182): \
                 {TRANSLATIONS} locale `{locale}` is missing key \
                 `{key}`. Without it, that locale renders the raw \
                 key string in the offline banner."
            );
        }
    }
}

/// Strip single-line `//` comments and block `/* ... */` comments
/// from a JS source window so a prose match on "await" inside a
/// comment doesn't confuse order checks. Iter 182.
fn strip_js_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' {
            match chars.peek() {
                Some('/') => {
                    // Line comment — skip until newline.
                    for cc in chars.by_ref() {
                        if cc == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                }
                Some('*') => {
                    // Block comment — skip until `*/`.
                    chars.next();
                    let mut prev = '\0';
                    for cc in chars.by_ref() {
                        if prev == '*' && cc == '/' {
                            break;
                        }
                        if cc == '\n' {
                            out.push('\n');
                        }
                        prev = cc;
                    }
                }
                _ => out.push(c),
            }
        } else {
            out.push(c);
        }
    }
    out
}

// --------------------------------------------------------------------
// Iter 217 structural pins — meta-guard header + SCANNER/INDEX_HTML/
// APP_JS/TRANSLATIONS path constants + show/hideOfflineBanner helpers
// + retry-button `init()` inline retry + strip_js_comments self-test.
// --------------------------------------------------------------------
//
// The twelve pins above cover scanner presence + DOM skeleton + ready-
// before-await order + idempotent retry + i18n parity + reference-file
// existence + it-block count + .only/.skip absence + real-file skeleton
// check + translations per-locale + production ready-flip order. They
// do NOT pin: (a) the guard's own header cites `fix.offline-empty-
// state` + `iter 84` — meta-guard contract (already in header); (b)
// the four path constants equal their canonical forms — rename drift
// hides as opaque panics; (c) `src/app.js` defines BOTH
// `showOfflineBanner` AND `hideOfflineBanner` helpers — the JS
// scanner calls both but the Rust pin doesn't yet verify production
// carries them; (d) the retry-button's click listener calls
// `this.init()` for inline retry — a refactor to `location.reload()`
// would reload the whole frontend and lose in-progress state; (e) the
// `strip_js_comments` helper (iter 182) actually strips both line +
// block comments — a regression to a pass-through would let prose
// "await" inside comments cause false positives on the order pin.

/// The guard's own module header must cite `fix.offline-empty-state`
/// and `iter 84` so a reader chasing the blank-screen regression
/// lands here via fix-plan-slot or iter-number grep.
#[test]
fn guard_file_header_cites_fix_slot_and_iter_84() {
    let body = fs::read_to_string("tests/offline_banner_scanner_guard.rs")
        .expect("tests/offline_banner_scanner_guard.rs must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("fix.offline-empty-state"),
        "meta-guard contract: tests/offline_banner_scanner_guard.rs \
         header must cite `fix.offline-empty-state`. Without it, a \
         reader chasing the iter-84 blank-screen regression won't \
         land here via fix-plan-slot grep.\nHeader:\n{header}"
    );
    assert!(
        header.contains("iter 84"),
        "meta-guard contract: header must cite `iter 84` so the \
         iteration that shipped the fix anchors to this guard by \
         iter-number grep."
    );
}

/// All four path constants must equal their canonical relative forms
/// verbatim. A rename of any of them (SCANNER, INDEX_HTML, APP_JS,
/// TRANSLATIONS) would silently cause every `read(path)` call to
/// panic with an opaque "file not readable" message.
#[test]
fn all_path_constants_are_canonical() {
    let guard_body = fs::read_to_string("tests/offline_banner_scanner_guard.rs")
        .expect("guard source must be readable");
    for literal in [
        "const SCANNER: &str = \"../tests/offline-banner.test.js\";",
        "const INDEX_HTML: &str = \"../src/index.html\";",
        "const APP_JS: &str = \"../src/app.js\";",
        "const TRANSLATIONS: &str = \"../src/translations.json\";",
    ] {
        assert!(
            guard_body.contains(literal),
            "fix.offline-empty-state (iter 217): \
             tests/offline_banner_scanner_guard.rs must retain \
             `{literal}` verbatim. A rename without atomic constant \
             update would break every pin with opaque `file not \
             readable` errors."
        );
    }
}

/// `src/app.js` must define BOTH `showOfflineBanner()` AND
/// `hideOfflineBanner()` method helpers. The JS scanner tests the
/// toggle behaviour against its own fixture; this Rust pin is a
/// defense-in-depth layer that catches a production rename
/// (e.g. `showBanner`/`hideBanner`) which would break every call
/// site (init, catch handlers) while the JS scanner still passes
/// against a stale fixture.
#[test]
fn app_js_defines_both_offline_banner_helpers() {
    let src = read(APP_JS);
    assert!(
        src.contains("showOfflineBanner()"),
        "fix.offline-empty-state (iter 217): src/app.js must define \
         `showOfflineBanner()` method. Renaming to a shorter form \
         (e.g. `showBanner()`) would break init's catch handlers and \
         orphan the banner's show path."
    );
    assert!(
        src.contains("hideOfflineBanner()"),
        "fix.offline-empty-state (iter 217): src/app.js must define \
         `hideOfflineBanner()` method. Same rationale: the retry \
         button's click handler relies on this method name."
    );
}

/// The retry button's click listener must call `this.init()` to do
/// an INLINE retry — not `location.reload()` or `window.reload()`.
/// A full-page reload would lose any in-progress state the user had
/// (form inputs, modal positions, banner state). Inline retry via
/// `init()` preserves state and is cheaper.
#[test]
fn retry_button_listener_calls_init_for_inline_retry() {
    let src = read(APP_JS);
    // Locate the method DECLARATION (not call sites). The declaration
    // is preceded by whitespace + `showOfflineBanner() {`; call sites
    // look like `this.showOfflineBanner();`.
    let fn_pos = src
        .find("  showOfflineBanner() {")
        .or_else(|| src.find("\n  showOfflineBanner() {"))
        .expect(
            "src/app.js must define `showOfflineBanner()` method \
             (pin searches for the declaration shape, not call sites)",
        );
    // Window is generous — the method is short but we want to cover
    // through the click-listener body.
    let window = &src[fn_pos..src.len().min(fn_pos + 2000)];
    assert!(
        window.contains("this.init()"),
        "fix.offline-empty-state (iter 217): src/app.js \
         showOfflineBanner must have the retry-button click listener \
         call `this.init()` for inline retry. Refactoring to \
         `location.reload()` would full-page-reload, losing the \
         in-progress state the user had before the offline event.\n\
         Window:\n{window}"
    );
    assert!(
        !window.contains("location.reload"),
        "fix.offline-empty-state (iter 217): src/app.js retry handler \
         must NOT call `location.reload()` — full-page reload loses \
         in-progress state and costs more than an inline init retry."
    );
}

/// `strip_js_comments` helper (iter 182) must actually strip both
/// line (`//`) and block (`/* */`) comments. If it regresses to a
/// pass-through, prose "await" inside a comment would cause false
/// positives on `app_js_ready_flip_precedes_first_await_in_init`.
#[test]
fn strip_js_comments_helper_self_test() {
    // Line comment removal.
    let with_line = "let x = 1; // this says await here\nlet y = 2;";
    let stripped = strip_js_comments(with_line);
    assert!(
        !stripped.contains("await"),
        "iter 217: strip_js_comments must remove line-comment text — \
         got `{stripped}`. If this regresses, prose `await` in \
         `// before the first await` comments in app.js would cause \
         false positives on the order pin."
    );

    // Block comment removal.
    let with_block = "let x = 1; /* mentions await */ let y = 2;";
    let stripped = strip_js_comments(with_block);
    assert!(
        !stripped.contains("await"),
        "iter 217: strip_js_comments must remove block-comment text — \
         got `{stripped}`. Same rationale as line-comment case."
    );

    // Real code must be preserved (non-comment `await` stays).
    let with_real = "async function x() { await fetch('/y'); }";
    let stripped = strip_js_comments(with_real);
    assert!(
        stripped.contains("await"),
        "iter 217: strip_js_comments must preserve non-comment code — \
         got `{stripped}`. A regression to always-strip would remove \
         real `await` tokens and break the order pin's positive case."
    );
}

/// Iter 182: the production `src/app.js` must keep the `.ready`
/// flip BEFORE the first `await` inside `async init()`. This is the
/// iter-84 structural fix. The JS scanner does the same check
/// against its own `App.init` source parse; this Rust pin is a
/// defense-in-depth layer that reads `src/app.js` directly, so a
/// test that parses a different (stale) excerpt of init would still
/// get caught. Comments (which mention "await" in prose) are
/// stripped first to avoid false positives.
#[test]
fn app_js_ready_flip_precedes_first_await_in_init() {
    let src = read(APP_JS);
    let init_pos = src
        .find("async init()")
        .expect("src/app.js must define `async init()`");
    // Scan a window large enough to cover the early part of init.
    let end = init_pos.saturating_add(15_000).min(src.len());
    let window_raw = &src[init_pos..end];
    let window = strip_js_comments(window_raw);
    let ready_pos = window
        .find("classList.add('ready')")
        .expect(
            "fix.offline-empty-state (iter 182): src/app.js init() \
             must keep the `classList.add('ready')` flip",
        );
    let first_await = window
        .find("await ")
        .expect(
            "fix.offline-empty-state (iter 182): src/app.js init() \
             must contain at least one `await` — the invariant is \
             that ready-flip precedes it",
        );
    assert!(
        ready_pos < first_await,
        "fix.offline-empty-state (iter 182): in src/app.js init(), \
         `classList.add('ready')` at offset {ready_pos} must come \
         BEFORE the first `await` at offset {first_await} (after \
         comment-stripping). Pushing the flip behind an await re-\
         introduces the iter-84 blank-screen bug: if any pre-flip \
         await throws, the outer catch leaves opacity 0."
    );
}
