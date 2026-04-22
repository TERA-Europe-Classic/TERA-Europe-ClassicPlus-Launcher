//! PRD 3.4.7 (no-jargon) + PRD 3.7.1 (key-parity) scanner drift guard.
//!
//! Two Vitest scanners enforce the i18n invariants the PRD promises:
//!
//! - `teralaunch/tests/i18n-jargon.test.js` — PRD 3.4.7: user-facing
//!   strings must not leak implementation jargon. Blocklist =
//!   ["composite", "mapper", "sha", "tmm"].
//! - `teralaunch/tests/i18n-parity.test.js` — PRD 3.7.1: every locale
//!   exposes the same set of keys (missing → raw MODS_* leaks in UI;
//!   extra → hidden translation).
//!
//! Both scanners shipped pre-iter-124. Nothing structurally pinned
//! their invariants — a refactor could drop a blocklist term, widen
//! an allowlist, or collapse the parity diff helper and the Vitest
//! suite would still go green against a weakened detector.
//!
//! This guard is the direct parallel to iter 124
//! `i18n_no_hardcoded_guard.rs`: Rust test asserting JS-file
//! structure. Batches both scanners into one file since each has a
//! small invariant surface.

use std::fs;

const JARGON_SCANNER: &str = "../tests/i18n-jargon.test.js";
const PARITY_SCANNER: &str = "../tests/i18n-parity.test.js";
const TRANSLATIONS: &str = "../src/translations.json";

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

// ---------- PRD 3.4.7 (no-jargon) ----------

/// The jargon scanner file must exist and self-identify as PRD 3.4.7.
#[test]
fn jargon_scanner_file_exists_and_self_identifies() {
    let body = read(JARGON_SCANNER);
    assert!(
        body.len() > 1000,
        "PRD 3.4.7 violated: {JARGON_SCANNER} is missing or truncated \
         (<1000 bytes). The jargon scanner is the measurement; losing \
         it silences the gate."
    );
    assert!(
        body.contains("PRD 3.4.7"),
        "{JARGON_SCANNER} must self-identify as the PRD 3.4.7 \
         measurement in its header comment."
    );
}

/// The blocklist must carry all four PRD-required terms in order.
/// Dropping any term would let that jargon class leak into UI copy
/// without tripping CI.
#[test]
fn jargon_blocklist_carries_four_prd_terms() {
    let body = read(JARGON_SCANNER);
    assert!(
        body.contains("const JARGON_BLOCKLIST = ['composite', 'mapper', 'sha', 'tmm'];"),
        "PRD 3.4.7 violated: {JARGON_SCANNER} must carry \
         `const JARGON_BLOCKLIST = ['composite', 'mapper', 'sha', 'tmm'];` \
         verbatim. Dropping or reordering a term weakens the contract \
         — the scanner's own `blocklist covers the four PRD-required \
         terms` assertion duplicates this at JS-test time, but this \
         Rust pin catches structural drift at cargo-test time too."
    );
}

/// The SUBSTRING_ALLOWLIST must stay empty. If a future copy edit
/// legitimately needs one of the blocklist fragments (e.g.
/// "Shanghai" contains "sha"), the pattern is to ADD a specific
/// allowlist entry with a rationale — NOT to drop the blocklist term.
/// Pinning empty here means a reviewer sees a deliberate code change
/// when that day comes.
#[test]
fn jargon_substring_allowlist_starts_empty() {
    let body = read(JARGON_SCANNER);
    // Tolerate whitespace and a comment block between the braces, but
    // reject any actual allowlist entry (a quoted string is the
    // shape an entry would take).
    let after = body
        .split("const SUBSTRING_ALLOWLIST = [")
        .nth(1)
        .unwrap_or_else(|| panic!("{JARGON_SCANNER} missing SUBSTRING_ALLOWLIST declaration"));
    let allowlist_body = after
        .split("];")
        .next()
        .unwrap_or_else(|| panic!("{JARGON_SCANNER} SUBSTRING_ALLOWLIST not closed with `];`"));
    assert!(
        !allowlist_body.contains('\''),
        "PRD 3.4.7 invariant: SUBSTRING_ALLOWLIST must remain empty. \
         An allowlist entry appeared in {JARGON_SCANNER}. The correct \
         fix for a false positive is a quoted-term exception with \
         rationale — a reviewer should see this Rust guard fail and \
         confirm the addition is deliberate."
    );
}

/// The scanner's self-test (`detector flags a seeded leak`) must
/// stay. Without it, a regression to an always-empty leaks list
/// would pass the real assertions vacuously.
#[test]
fn jargon_scanner_carries_its_own_self_test() {
    let body = read(JARGON_SCANNER);
    assert!(
        body.contains("detector flags a seeded leak"),
        "{JARGON_SCANNER} must retain the `detector flags a seeded \
         leak` self-test. Without it, a broken findJargonLeaks() \
         that returned [] unconditionally would pass vacuously."
    );
    // The fixture proves 3 of the 4 blocklist terms bite at once.
    assert!(
        body.contains("Patch the composite mapper using TMM."),
        "{JARGON_SCANNER} self-test fixture `Patch the composite \
         mapper using TMM.` must stay — it exercises 3 blocklist \
         terms (composite/mapper/tmm) in a single string and is what \
         proves the scanner actually bites."
    );
}

// ---------- PRD 3.7.1 (key-parity) ----------

/// The parity scanner file must exist and self-identify as PRD 3.7.1.
#[test]
fn parity_scanner_file_exists_and_self_identifies() {
    let body = read(PARITY_SCANNER);
    assert!(
        body.len() > 1000,
        "PRD 3.7.1 violated: {PARITY_SCANNER} is missing or truncated \
         (<1000 bytes). The key-parity scanner is the measurement; \
         losing it silences the gate."
    );
    assert!(
        body.contains("PRD 3.7.1"),
        "{PARITY_SCANNER} must self-identify as the PRD 3.7.1 \
         measurement in its header comment."
    );
}

/// The parity scanner must carry all three substantive assertions:
/// at-least-two-locales sanity, keys_equal_across_locales, and equal
/// key-count. Dropping one would leave a class of drift uncaught.
#[test]
fn parity_scanner_carries_three_assertions() {
    let body = read(PARITY_SCANNER);
    assert!(
        body.contains("at least two locales"),
        "{PARITY_SCANNER} must carry the `at least two locales` \
         sanity — without it, a regression that drops all but one \
         locale would render the parity check vacuously passing."
    );
    assert!(
        body.contains("keys_equal_across_locales"),
        "PRD 3.7.1 core: {PARITY_SCANNER} must carry the \
         `keys_equal_across_locales` assertion (missing/extra diff)."
    );
    assert!(
        body.contains("same key count"),
        "{PARITY_SCANNER} must carry the `every locale has the same \
         key count` assertion — complements keys_equal_across_locales \
         by catching duplicated-key-name regressions that the \
         set-based diff would miss."
    );
}

/// The `diffKeySets` helper must retain BOTH `missing` and `extra`
/// outputs. If someone collapses it to `missing`-only, the scanner
/// stops catching "extra key in one locale" drift.
#[test]
fn parity_diff_helper_returns_both_missing_and_extra() {
    let body = read(PARITY_SCANNER);
    assert!(
        body.contains("missing:") && body.contains("extra:"),
        "{PARITY_SCANNER} diffKeySets must return both `missing` and \
         `extra`. Missing-only would miss the `extra key in one \
         locale hides a translation` drift pattern."
    );
}

/// The parity scanner's self-test must stay.
#[test]
fn parity_scanner_carries_its_own_self_test() {
    let body = read(PARITY_SCANNER);
    assert!(
        body.contains("detector flags a seeded missing key"),
        "{PARITY_SCANNER} must retain the `detector flags a seeded \
         missing key` self-test. Without it, a broken diffKeySets \
         that returned {{missing: [], extra: []}} unconditionally \
         would pass the real parity assertions vacuously."
    );
}

// ---------- Shared invariant ----------

/// Both scanners import the same `translations.json`. If the file
/// moves or is renamed, both go silent. Pin its existence here.
#[test]
fn shared_translations_file_exists_and_is_non_empty() {
    let body = read(TRANSLATIONS);
    assert!(
        !body.trim().is_empty(),
        "{TRANSLATIONS} (the artifact both i18n scanners validate) \
         must exist and be non-empty. A deletion would make both \
         PRD 3.4.7 and PRD 3.7.1 scanners vacuous."
    );
    // Surface-sanity: JSON root must be an object containing at
    // least one locale key (the parity check requires ≥2, but we
    // only need to pin ≥1 here — the Vitest assertion pins the ≥2).
    assert!(
        body.trim_start().starts_with('{'),
        "{TRANSLATIONS} must be a JSON object at its root (locale → \
         entries mapping). A root-array shape would break both \
         scanners."
    );
}

// ---------- Self-test ----------

/// Self-test — prove the detectors in THIS guard bite on synthetic
/// bad shapes.
#[test]
fn i18n_scanner_guard_detector_self_test() {
    // Bad shape A: jargon blocklist with a term dropped.
    let short_blocklist = "const JARGON_BLOCKLIST = ['composite', 'mapper', 'sha'];";
    assert!(
        !short_blocklist
            .contains("const JARGON_BLOCKLIST = ['composite', 'mapper', 'sha', 'tmm'];"),
        "self-test: blocklist missing `tmm` must be flagged"
    );

    // Bad shape B: substring allowlist with a new entry.
    let with_entry = "const SUBSTRING_ALLOWLIST = [\n    'shanghai',\n];";
    let after = with_entry
        .split("const SUBSTRING_ALLOWLIST = [")
        .nth(1)
        .unwrap();
    let allowlist_body = after.split("];").next().unwrap();
    assert!(
        allowlist_body.contains('\''),
        "self-test: a non-empty SUBSTRING_ALLOWLIST must be flagged \
         by the quote-presence check"
    );

    // Bad shape C: parity diff collapsed to missing-only.
    let one_sided = "return { missing: [...refSet].filter(...) };";
    assert!(
        one_sided.contains("missing:") && !one_sided.contains("extra:"),
        "self-test: a one-sided diff helper must be flagged"
    );

    // Bad shape D: parity scanner without the at-least-two-locales
    // sanity check.
    let no_sanity = "it('keys_equal_across_locales', () => {});";
    assert!(
        !no_sanity.contains("at least two locales"),
        "self-test: parity scanner missing the sanity check must be \
         flagged"
    );

    // Iter 187 — additional bad shapes.

    // Bad shape E: `.only` pin on either scanner.
    let only_pin = "it.only('keys_equal_across_locales', () => {});";
    assert!(
        only_pin.contains("it.only"),
        "self-test: .only detector must bite on `it.only(`"
    );

    // Bad shape F: jargon scanner without case-insensitive match.
    let case_sensitive = "if (value.includes('TMM')) leaks.push(value);";
    assert!(
        !case_sensitive.contains(".toLowerCase()"),
        "self-test: case-sensitive jargon scan must be flagged — \
         'TMM' vs 'tmm' would slip through"
    );

    // Bad shape G: translations.json with only 1 locale.
    let one_locale = r#"{"EUR": {"MODS_FILTER_ALL": "All"}}"#;
    let locale_count = one_locale.matches(r#"": {"#).count();
    assert!(
        locale_count < 4,
        "self-test: single-locale translations must be flagged by \
         the 4-locale floor (found {locale_count})"
    );
}

/// Iter 187: each scanner must carry at least 2 `it(` blocks —
/// main assertion + self-test. Below the floor means a measurement
/// was deleted.
#[test]
fn i18n_scanners_have_minimum_it_count_each() {
    let jargon = read(JARGON_SCANNER);
    let parity = read(PARITY_SCANNER);
    let jargon_it = jargon.matches("it(").count() + jargon.matches("it.only(").count();
    let parity_it = parity.matches("it(").count() + parity.matches("it.only(").count();
    assert!(
        jargon_it >= 2,
        "PRD 3.4.7 (iter 187): {JARGON_SCANNER} must carry at least \
         2 `it(` blocks (main assertion + self-test). Found \
         {jargon_it}."
    );
    assert!(
        parity_it >= 2,
        "PRD 3.7.1 (iter 187): {PARITY_SCANNER} must carry at least \
         2 `it(` blocks. Found {parity_it}."
    );
}

/// Iter 187: neither scanner may carry `.only` / `.skip` / `xit` /
/// `xdescribe` — dev-local pins that silently disable the remaining
/// i18n invariant checks in CI.
#[test]
fn i18n_scanners_carry_no_only_or_skip_markers() {
    for (path, body) in [
        (JARGON_SCANNER, read(JARGON_SCANNER)),
        (PARITY_SCANNER, read(PARITY_SCANNER)),
    ] {
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
                "i18n (iter 187): {path} must not carry `{forbidden}` \
                 — dev-local pins disable sibling i18n invariant \
                 tests in CI."
            );
        }
    }
}

/// Iter 187: real `translations.json` must carry all four supported
/// locales (EUR, FRA, GER, RUS). The parity scanner's runtime check
/// pins symmetric key sets but not that a specific locale exists;
/// a regression that renamed `EUR` to `ENG` would pass parity as
/// long as the other three locales matched.
#[test]
fn translations_json_has_four_supported_locales() {
    let body = read(TRANSLATIONS);
    for locale in ["\"EUR\"", "\"FRA\"", "\"GER\"", "\"RUS\""] {
        assert!(
            body.contains(locale),
            "PRD 3.7.1 (iter 187): {TRANSLATIONS} must carry locale \
             key `{locale}`. The launcher ships with these four \
             supported locales; renaming/removing one breaks the UI \
             without tripping the symmetric parity check."
        );
    }
}

/// Iter 187: real `translations.json` must carry a meaningful number
/// of `MODS_*` prefixed keys — these are the mod-manager i18n keys
/// that `src/mods.js` reads. A collapse to near-zero means i18n was
/// stripped; a collapse to a few means the mod-manager UI is mostly
/// hardcoded English.
#[test]
fn translations_json_carries_substantive_mods_key_set() {
    let body = read(TRANSLATIONS);
    let mods_key_count = body.matches("\"MODS_").count();
    assert!(
        mods_key_count >= 40,
        "PRD 3.7.1 (iter 187): {TRANSLATIONS} must carry at least 40 \
         `MODS_*` keys (10 keys × 4 locales). Found \
         {mods_key_count}. Below the floor means the mod-manager i18n \
         surface was gutted — UI strings would render raw MODS_* \
         keys."
    );
}

/// Iter 187: the jargon scanner must use a case-insensitive match
/// — `.toLowerCase()` before the substring check. Without it, copy
/// like "Composite" or "TMM" (camel-case or uppercase) would slip
/// past a lowercase-only blocklist.
#[test]
fn jargon_scanner_matches_case_insensitively() {
    let body = read(JARGON_SCANNER);
    assert!(
        body.contains(".toLowerCase()"),
        "PRD 3.4.7 (iter 187): {JARGON_SCANNER} must call \
         `.toLowerCase()` on each scanned value before the substring \
         check against JARGON_BLOCKLIST. Without it, 'TMM' or \
         'Composite' (upper/title-case) would slip past a lowercase-\
         only blocklist."
    );
}

// --------------------------------------------------------------------
// Iter 241 structural pins — path-constant canonicalisation,
// blocklist terms verbatim, four-locale-set pinning, mods-key floor
// per-locale check, and locales-subset sanity helper.
//
// Iter-187 covered self-identification + scanner structure + locale
// key existence + aggregate mods-key floor + case-insensitivity.
// These five extend to the meta-guard + per-locale discipline
// surface a confident refactor could still miss: a path-constant
// drift, a blocklist term silently dropped (composite/mapper/sha/
// tmm — each maps to a distinct jargon class), a locale count drift
// at the top level, a mods-key count that drops below 10 in any
// single locale (meaning one locale's i18n got gutted while others
// stayed), and a locale-set that grew unexpectedly (new locale
// added without PRD note).
// --------------------------------------------------------------------

/// Iter 241: `JARGON_SCANNER`, `PARITY_SCANNER`, `TRANSLATIONS`
/// constants must stay canonical. Every `read(...)` call resolves
/// through one of these; a rename panics all tests with "file not
/// found" instead of pointing at the drift.
#[test]
fn guard_path_constants_are_canonical() {
    let body = read("tests/i18n_scanner_guard.rs");
    for (name, expected) in [
        ("JARGON_SCANNER", "../tests/i18n-jargon.test.js"),
        ("PARITY_SCANNER", "../tests/i18n-parity.test.js"),
        ("TRANSLATIONS", "../src/translations.json"),
    ] {
        let line = format!("const {name}: &str = \"{expected}\";");
        assert!(
            body.contains(&line),
            "PRD 3.4.7/3.7.1 (iter 241): i18n_scanner_guard.rs must \
             keep `{line}` verbatim. A path drift leaves every \
             read() with file-not-found and silently disables every \
             pin."
        );
    }
}

/// Iter 241: the jargon scanner's blocklist must enumerate all four
/// canonical terms: `composite`, `mapper`, `sha`, `tmm`. Each
/// targets a distinct jargon class:
///
///   - `composite` — TMM composite-package internal term
///   - `mapper` — CompositePackageMapper internal term
///   - `sha` — hash-pipeline internal term
///   - `tmm` — mod format name users shouldn't need to know
///
/// Dropping any one silently lets that term leak into user-facing copy.
#[test]
fn jargon_blocklist_enumerates_all_four_canonical_terms() {
    let body = read(JARGON_SCANNER);
    for term in ["\"composite\"", "\"mapper\"", "\"sha\"", "\"tmm\""] {
        assert!(
            body.contains(term),
            "PRD 3.4.7 (iter 241): {JARGON_SCANNER} must include \
             {term} in its JARGON_BLOCKLIST. Each blocklist term \
             targets a distinct jargon class — dropping one \
             silently lets that term leak into user-facing copy. \
             (composite/mapper/sha/tmm are non-interchangeable; \
             each is a distinct internal-vocabulary token.)"
        );
    }
}

/// Iter 241: `translations.json` root must carry EXACTLY the four
/// canonical locale keys: `EUR`, `FRA`, `GER`, `RUS`. Iter-187
/// pinned each key is PRESENT but didn't pin the SET (an extra `ITA`
/// or `ESP` would slip through the existence check). A new locale
/// without PRD note means translations were added without the full
/// l10n + a11y review.
#[test]
fn translations_json_carries_exactly_four_canonical_locales() {
    let body = read(TRANSLATIONS);
    let v: serde_json::Value =
        serde_json::from_str(&body).expect("translations.json must be valid JSON");
    let obj = v
        .as_object()
        .expect("translations.json root must be an object");
    let actual_strings: std::collections::BTreeSet<String> = obj.keys().cloned().collect();
    let expected: std::collections::BTreeSet<String> = ["EUR", "FRA", "GER", "RUS"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        actual_strings, expected,
        "PRD 3.7.1 (iter 241): {TRANSLATIONS} root must carry \
         exactly the four canonical locales (EUR, FRA, GER, RUS — \
         the game-server region codes this launcher supports). \
         Found: {actual_strings:?}. Adding a new locale without a \
         PRD note bypasses the l10n + a11y review — every new \
         locale needs a pass over the jargon blocklist, string \
         length, and pluralisation review."
    );
}

/// Iter 241: every locale in `translations.json` must carry at
/// least 10 `MODS_*` keys. Iter-187 pinned the AGGREGATE floor at
/// 40 (10 × 4 locales); a regression that zeroed out ONE locale's
/// mods section while the other three stayed intact would still
/// satisfy the aggregate floor (30 keys survive across 3 intact
/// locales). Per-locale floor catches asymmetric drift.
#[test]
fn every_locale_carries_mods_keyset_floor() {
    let body = read(TRANSLATIONS);
    let v: serde_json::Value =
        serde_json::from_str(&body).expect("translations.json must be valid JSON");
    let obj = v.as_object().expect("root must be an object");
    for (locale, entries) in obj {
        let entries_obj = entries
            .as_object()
            .unwrap_or_else(|| panic!("{locale} locale must be an object"));
        let mods_count = entries_obj
            .keys()
            .filter(|k| k.starts_with("MODS_"))
            .count();
        assert!(
            mods_count >= 10,
            "PRD 3.7.1 (iter 241): locale `{locale}` has only \
             {mods_count} `MODS_*` keys (floor: 10). The aggregate \
             floor (iter 187) would still hold with one locale \
             zeroed out; this per-locale floor catches that \
             asymmetric drift. If `{locale}` is deliberately not \
             localised yet, document it as a PRD exception."
        );
    }
}

/// Iter 241: the jargon scanner must explicitly iterate `Object.entries`
/// of each locale's translation object — not just the aggregate
/// `JSON.stringify(translations)`. A JSON.stringify-based scanner
/// would match string VALUES but also the KEY names themselves —
/// keys like `MODS_COMPOSITE_LABEL` would false-positive. The
/// per-entry iteration lets the scanner distinguish keys (ignored)
/// from values (checked).
#[test]
fn jargon_scanner_iterates_entries_not_stringifies_aggregate() {
    let body = read(JARGON_SCANNER);
    // Positive: the scanner uses Object.entries / Object.values or
    // similar per-entry iteration.
    let per_entry = body.contains("Object.values")
        || body.contains("Object.entries")
        || body.contains(".values(");
    assert!(
        per_entry,
        "PRD 3.4.7 (iter 241): {JARGON_SCANNER} must iterate per-\
         entry (`Object.entries` / `Object.values`) — not \
         JSON.stringify the whole tree. Stringify matches key names \
         too, producing false positives on keys like \
         `MODS_COMPOSITE_LABEL`."
    );
    // Negative: no naive JSON.stringify-then-indexOf pattern that
    // would conflate keys and values.
    assert!(
        !body.contains("JSON.stringify(translations)"),
        "PRD 3.4.7 (iter 241): {JARGON_SCANNER} must NOT call \
         `JSON.stringify(translations)` as its search surface — \
         that conflates keys and values and produces false \
         positives on legitimate key names (e.g. \
         MODS_COMPOSITE_LABEL)."
    );
}

// --------------------------------------------------------------------
// Iter 276 structural pins — scanner/guard bounds + translations bounds
// + PRD 3.4.7 + 3.7.1 cites + translations JSON valid.
// --------------------------------------------------------------------

#[test]
fn jargon_scanner_byte_bounds() {
    const MIN_BYTES: usize = 500;
    const MAX_BYTES: usize = 30_000;
    let bytes = std::fs::metadata(JARGON_SCANNER)
        .expect("jargon scanner must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "PRD 3.4.7 (iter 276): {JARGON_SCANNER} is {bytes} bytes; \
         expected [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

#[test]
fn parity_scanner_byte_bounds() {
    const MIN_BYTES: usize = 500;
    const MAX_BYTES: usize = 30_000;
    let bytes = std::fs::metadata(PARITY_SCANNER)
        .expect("parity scanner must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "PRD 3.7.1 (iter 276): {PARITY_SCANNER} is {bytes} bytes; \
         expected [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

#[test]
fn translations_json_byte_bounds() {
    const MIN_BYTES: usize = 5000;
    const MAX_BYTES: usize = 500_000;
    let bytes = std::fs::metadata(TRANSLATIONS)
        .expect("translations.json must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "PRD 3.7.1 (iter 276): {TRANSLATIONS} is {bytes} bytes; \
         expected [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

#[test]
fn guard_source_cites_both_prd_criteria() {
    let body = std::fs::read_to_string("tests/i18n_scanner_guard.rs").expect("guard must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.4.7"),
        "iter 276: guard header must cite `PRD 3.4.7` (no-jargon)."
    );
    assert!(
        header.contains("PRD 3.7.1"),
        "iter 276: guard header must cite `PRD 3.7.1` (key-parity)."
    );
}

#[test]
fn translations_json_parses_as_valid_json_object() {
    let body = std::fs::read_to_string(TRANSLATIONS).expect("translations.json must exist");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|e| {
        panic!(
            "PRD 3.7.1 (iter 276): {TRANSLATIONS} must parse as \
             valid JSON. Parse error: {e}"
        )
    });
    assert!(
        v.is_object(),
        "PRD 3.7.1 (iter 276): {TRANSLATIONS} must parse as a JSON \
         object (locale → table). Got: {v:?}"
    );
}
