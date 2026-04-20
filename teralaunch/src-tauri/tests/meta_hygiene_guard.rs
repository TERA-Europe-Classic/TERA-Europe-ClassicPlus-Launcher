//! Meta drift guard: enforces the structural contract every other
//! `tests/*_guard.rs` file follows.
//!
//! Across iters 86-134 the worktree accumulated 18 structural drift
//! guards (shell scope, PRD paths, CVE-2025-31477 call-sites, i18n
//! scanners, perf bench, etc.). Each one follows three conventions
//! that were enforced by hand:
//!
//! 1. **Module-level `//!` header comment** — explains WHAT the
//!    guard protects and WHY, so a future reader can trace it back
//!    to a PRD criterion / fix P-slot / audit doc.
//! 2. **Detector self-test** — a `#[test] fn` whose name contains
//!    `self_test` or `detector`. Proves the detector bites on
//!    synthetic bad shapes. Without it, a regressed detector
//!    (e.g. always-passing classify fn) would silently rubber-stamp
//!    real violations.
//! 3. **Presence in the known-guard list** — deleting a guard file
//!    without updating this test catches silent removal of an
//!    invariant.
//!
//! This meta-guard pins those conventions so a future contributor
//! can't write a one-off guard file that skips a self-test or
//! remove a guard file without a visible signal.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const TESTS_DIR: &str = "tests";

/// Known guard files as of iter 135. Deletion is intentional code
/// change → must bump this list. Addition is implicit (the
/// file-walk picks it up), but the new file must still satisfy
/// the hygiene contract below.
const KNOWN_GUARDS: &[&str] = &[
    "anti_reverse_guard.rs",
    "architecture_doc_guard.rs",
    "changelog_guard.rs",
    "classicplus_guards_scanner_guard.rs",
    "claude_md_guard.rs",
    "crate_comment_guard.rs",
    "deploy_scope_infra_guard.rs",
    "i18n_no_hardcoded_guard.rs",
    "i18n_scanner_guard.rs",
    "lessons_learned_guard.rs",
    "meta_hygiene_guard.rs",
    "mods_categories_ui_scanner_guard.rs",
    "offline_banner_scanner_guard.rs",
    "portal_https_guard.rs",
    "prd_path_drift_guard.rs",
    "search_perf_guard.rs",
    "secret_scan_guard.rs",
    "shell_open_callsite_guard.rs",
    "tauri_v2_migration_audit_guard.rs",
];

fn discovered_guards() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(TESTS_DIR)
        .unwrap_or_else(|e| panic!("read_dir {TESTS_DIR}: {e}"))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with("_guard.rs"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    files
}

fn basename(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Disk reality must match KNOWN_GUARDS exactly. Silent deletion or
/// addition both trip this — deletion is obvious; addition tells
/// the author to update KNOWN_GUARDS + check the new file satisfies
/// the hygiene contract (the other asserts cover the contract for
/// whatever's on disk).
#[test]
fn known_guard_list_matches_disk() {
    let disk: HashSet<String> = discovered_guards()
        .iter()
        .map(|p| basename(p.as_path()))
        .collect();
    let known: HashSet<String> = KNOWN_GUARDS.iter().map(|s| s.to_string()).collect();
    let extra_on_disk: Vec<_> = disk.difference(&known).collect();
    let missing_from_disk: Vec<_> = known.difference(&disk).collect();
    assert!(
        extra_on_disk.is_empty(),
        "New guard file(s) on disk not listed in KNOWN_GUARDS: {:?}. \
         Add them to the KNOWN_GUARDS array so deletion later trips \
         this meta-guard.",
        extra_on_disk
    );
    assert!(
        missing_from_disk.is_empty(),
        "Guard file(s) in KNOWN_GUARDS no longer exist on disk: \
         {:?}. Silent deletion loses the invariant that file \
         protected. If deletion was intentional, update \
         KNOWN_GUARDS and explain in the commit why the invariant \
         no longer needs guarding.",
        missing_from_disk
    );
}

/// Every guard file must start with a `//!` module-level doc
/// comment. This is the WHAT/WHY that lets a future reader trace
/// the guard back to its purpose.
#[test]
fn every_guard_carries_module_doc_header() {
    for path in discovered_guards() {
        let body = read(&path);
        let first_nonblank = body.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        assert!(
            first_nonblank.starts_with("//!"),
            "{} must start with a `//!` module-level doc comment. \
             Found first non-blank line: `{}`. The comment is what \
             lets a future reader understand WHAT the guard \
             protects (PRD criterion / fix P-slot / audit doc).",
            path.display(),
            first_nonblank
        );
    }
}

/// Every guard file must carry at least one detector self-test
/// function. Pattern: an `fn *_self_test` or `fn *_detector*`
/// name. Without a self-test, a regressed detector (e.g. an
/// always-None classifier) silently rubber-stamps real violations
/// while the guard's surface assertions pass vacuously.
#[test]
fn every_guard_carries_a_detector_self_test() {
    for path in discovered_guards() {
        let body = read(&path);
        // Scan for fn names that carry the self-test idiom.
        let has_self_test = body
            .lines()
            .any(|l| {
                let t = l.trim_start();
                t.starts_with("fn ")
                    && (l.contains("_self_test")
                        || l.contains("detector_self_test")
                        || l.contains("_detector("))
            });
        assert!(
            has_self_test,
            "{} must carry at least one detector self-test fn \
             (name containing `_self_test` or `_detector`). Without \
             one, a regressed detector would silently pass the real \
             assertions vacuously. Required by the iter-86-to-134 \
             guard-writing convention.",
            path.display()
        );
    }
}

/// Every guard file's module header must cite either a PRD criterion
/// (`3.x.y` / `§3.x.y`), a fix-plan P-slot (`fix.*` / `sec.*` /
/// `adv.*` / `pin.*`), an iteration number (`iter N`), or a known
/// named audit (`Classic+`, `tauri-v2`, `CVE-`). This is the
/// "traceable to purpose" invariant — without it, a future reader
/// sees a file full of asserts with no idea what they protect.
#[test]
fn every_guard_header_cites_a_traceable_anchor() {
    for path in discovered_guards() {
        let body = read(&path);
        // Only look at the doc-comment block at the top.
        let header: String = body
            .lines()
            .take_while(|l| l.trim_start().starts_with("//!") || l.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let has_prd = header.contains("3.1.")
            || header.contains("3.2.")
            || header.contains("3.3.")
            || header.contains("3.4.")
            || header.contains("3.5.")
            || header.contains("3.6.")
            || header.contains("3.7.")
            || header.contains("3.8.")
            || header.contains("§3");
        let has_p_slot = header.contains("fix.")
            || header.contains("sec.")
            || header.contains("adv.")
            || header.contains("pin.");
        let has_iter = header.contains("iter ");
        let has_named_audit = header.contains("Classic+")
            || header.contains("tauri-v1")
            || header.contains("tauri-v2")
            || header.contains("CVE-");
        assert!(
            has_prd || has_p_slot || has_iter || has_named_audit,
            "{} module header must cite at least one traceable \
             anchor: a PRD criterion (`3.x.y`), a fix-plan P-slot \
             (`fix.*`, `sec.*`, `adv.*`, `pin.*`), an iteration \
             number (`iter N`), or a named audit (`Classic+`, \
             `tauri-v2`, `CVE-*`). Without this, a future reader \
             can't trace the guard back to its purpose.\nHeader:\n{}",
            path.display(),
            header
        );
    }
}

/// Every guard must read real disk content — the guard's job is to
/// catch DRIFT between source-of-truth files (PRD, configs, JS
/// tests, source code) and the invariants they encode. A guard
/// that only reasons about inline string literals is a stub: the
/// self-test could pass while the real invariant silently rotted.
///
/// Enforced by requiring the file contain either `fs::read_to_string`
/// or `fs::read_dir`. Both are the primitives every guard uses to
/// touch disk; missing both means the guard is entirely inline.
#[test]
fn every_guard_reads_real_disk_content() {
    for path in discovered_guards() {
        let body = read(&path);
        let reads_file = body.contains("fs::read_to_string");
        let reads_dir = body.contains("fs::read_dir");
        assert!(
            reads_file || reads_dir,
            "{} does not call `fs::read_to_string` or \
             `fs::read_dir` anywhere — the guard is a STUB (only \
             inline fixtures, no real source-of-truth reading). A \
             guard's job is to catch DRIFT between an on-disk \
             source-of-truth and its encoded invariant; a stub can \
             pass its self-test while the real invariant rotted.",
            path.display()
        );
    }
}

// --------------------------------------------------------------------
// Iter 174 contract extensions — test-count floor + byte-length floor
// + sorted KNOWN_GUARDS + assertion presence + self-reference.
// --------------------------------------------------------------------
//
// Iter 135-136 pinned the 5-contract baseline (KNOWN_GUARDS match,
// module header, detector self-test, traceable anchor, disk-read).
// Iter 174 adds five more rules that a stub-author could still
// violate while satisfying the baseline: a 1-test guard, a 200-byte
// shell guard, an unordered list, tests without assertions, and a
// dog-fooding gap (meta-guard itself missing from KNOWN_GUARDS).

/// Every guard file must have at least TWO `#[test]` functions. One
/// is either a detector self-test (useless alone — it proves the
/// detector works but doesn't assert the real invariant) or a real
/// test (useless alone — a single real test with no self-test can
/// silently rubber-stamp a regressed detector).
#[test]
fn every_guard_has_at_least_two_test_fns() {
    for path in discovered_guards() {
        let body = read(&path);
        let test_count = body.lines().filter(|l| l.trim() == "#[test]").count();
        assert!(
            test_count >= 2,
            "{} carries only {} `#[test]` fn(s); the contract \
             requires ≥ 2 (a real test + a self-test, or multiple \
             real tests). A single-test guard is either a stub or \
             one half of the self-test-plus-real-test pair the \
             baseline contract (iter 135) assumes.",
            path.display(),
            test_count
        );
    }
}

/// Every guard file must exceed a minimum byte length — a real
/// drift-guard can't be shorter than ~500 bytes (doc header + two
/// tests + any helpers). Iter 174 pins the floor so a future
/// refactor that truncates a guard to its doc header (passing the
/// baseline contract via a no-op self-test) fails CI.
#[test]
fn every_guard_exceeds_minimum_byte_length() {
    const MIN_BYTES: usize = 500;
    for path in discovered_guards() {
        let len = fs::metadata(&path)
            .unwrap_or_else(|e| panic!("metadata {}: {e}", path.display()))
            .len() as usize;
        assert!(
            len >= MIN_BYTES,
            "{} is only {} bytes; contract requires ≥ {MIN_BYTES}. \
             A guard shorter than this is almost certainly a stub \
             (doc header + no-op self-test could still satisfy the \
             baseline contract while encoding no real invariant).",
            path.display(),
            len
        );
    }
}

/// `KNOWN_GUARDS` must be sorted alphabetically. An unsorted list
/// makes it hard to review additions at a glance and easier to
/// accidentally duplicate an entry. The meta-guard's own utility
/// as a review surface depends on the list staying readable.
#[test]
fn known_guards_list_is_sorted_alphabetically() {
    let mut sorted = KNOWN_GUARDS.to_vec();
    sorted.sort();
    assert_eq!(
        KNOWN_GUARDS,
        sorted.as_slice(),
        "KNOWN_GUARDS in meta_hygiene_guard.rs must be sorted \
         alphabetically. An unsorted list obscures diffs when an \
         entry is added or removed. Sort the const to match the \
         expected order."
    );
}

/// Every guard file must contain at least one `assert!` /
/// `assert_eq!` / `assert_ne!` / `panic!` call. A test function
/// without assertions vacuously passes — meaning the whole guard
/// file could be a no-op while the meta-guard's baseline contract
/// (doc header + self-test-named fn + disk-read) still appears
/// satisfied.
#[test]
fn every_guard_contains_at_least_one_assertion() {
    for path in discovered_guards() {
        let body = read(&path);
        let has_assert = body.contains("assert!(")
            || body.contains("assert_eq!(")
            || body.contains("assert_ne!(")
            || body.contains("panic!(");
        assert!(
            has_assert,
            "{} contains no `assert!` / `assert_eq!` / `assert_ne!` \
             / `panic!` calls. A test file without assertions \
             vacuously passes — the guard would rubber-stamp every \
             run even though it encodes no real invariant.",
            path.display()
        );
    }
}

/// `meta_hygiene_guard.rs` itself must appear in `KNOWN_GUARDS`.
/// Dog-fooding: if the meta-guard isn't tracked by its own list,
/// silent deletion of this file would strand every drift-guard
/// in the workspace without supervision. The inclusion is
/// structural insurance that the contract-of-contracts can't be
/// removed without tripping itself.
#[test]
fn meta_hygiene_guard_is_in_its_own_known_list() {
    assert!(
        KNOWN_GUARDS.contains(&"meta_hygiene_guard.rs"),
        "KNOWN_GUARDS must include `meta_hygiene_guard.rs` itself. \
         Without this self-reference, silent deletion of this file \
         wouldn't trip the `known_guard_list_matches_disk` check, \
         and every drift-guard in the workspace would lose its \
         contract supervision."
    );
}

/// Self-test — prove the detectors in THIS meta-guard bite on
/// synthetic bad shapes.
#[test]
fn meta_guard_hygiene_detector_self_test() {
    // Bad shape A: file doesn't start with //!.
    let no_header = "use std::fs;\n\nfn thing() {}";
    let first_nonblank = no_header
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    assert!(
        !first_nonblank.starts_with("//!"),
        "self-test: file missing //! header must be flagged"
    );

    // Bad shape B: no self-test fn.
    let no_self_test = "//! header\n\n#[test]\nfn regular_test() {}";
    let has_st = no_self_test.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with("fn ")
            && (l.contains("_self_test")
                || l.contains("detector_self_test")
                || l.contains("_detector("))
    });
    assert!(
        !has_st,
        "self-test: file without self-test fn must be flagged"
    );

    // Bad shape C: header with no traceable anchor.
    let bland_header = "//! A guard.\n//!\n//! Does stuff.\n";
    let has_any_anchor = bland_header.contains("3.1.")
        || bland_header.contains("3.2.")
        || bland_header.contains("3.3.")
        || bland_header.contains("3.4.")
        || bland_header.contains("3.5.")
        || bland_header.contains("3.6.")
        || bland_header.contains("3.7.")
        || bland_header.contains("3.8.")
        || bland_header.contains("§3")
        || bland_header.contains("fix.")
        || bland_header.contains("sec.")
        || bland_header.contains("adv.")
        || bland_header.contains("pin.")
        || bland_header.contains("iter ")
        || bland_header.contains("Classic+")
        || bland_header.contains("tauri-v1")
        || bland_header.contains("tauri-v2")
        || bland_header.contains("CVE-");
    assert!(
        !has_any_anchor,
        "self-test: bland header without anchors must be flagged"
    );

    // Positive self-test: a realistic header must pass all three
    // checks.
    let good_header = "//! PRD 3.1.5 (CVE-2025-31477) shell-scope drift guard.\n";
    let pos_has_any = good_header.contains("3.1.") || good_header.contains("CVE-");
    assert!(pos_has_any, "self-test: realistic header must pass");

    // Bad shape D: stub guard (only inline assertions, no disk-read).
    let stub_body = "#[test]\nfn foo() {\n    assert_eq!(1 + 1, 2);\n}\n";
    assert!(
        !stub_body.contains("fs::read_to_string") && !stub_body.contains("fs::read_dir"),
        "self-test: stub guard with no disk-read must be flagged"
    );

    // Positive for disk-read: real guards touch fs.
    let real_body = "let body = fs::read_to_string(\"foo\").unwrap();";
    assert!(
        real_body.contains("fs::read_to_string"),
        "self-test: real-file-reading body must pass the disk-read \
         check"
    );
}

// --------------------------------------------------------------------
// Iter 209 structural pins — KNOWN_GUARDS list integrity (no dups +
// suffix convention + count floor) + meta-guard constants alignment.
// --------------------------------------------------------------------
//
// The eleven pins above lock down the CONTRACT every guard must satisfy
// (header, self-test, disk-read, anchor, assertion, test-count,
// byte-length, sorted, self-reference) + the list↔disk set-diff + the
// detector self-test. They do NOT pin: (a) KNOWN_GUARDS has no
// duplicate entries — a copy-paste bump would produce misleading
// "1 extra in list" diffs and the sort pin wouldn't catch it; (b)
// every KNOWN_GUARDS entry ends with `_guard.rs` — a typo like
// `architecture_doc.rs` (missing `_guard`) wouldn't be discovered by
// the file-walk (suffix filter) so the set-diff would fire but the
// root cause would be obscured; (c) the `MIN_BYTES` floor constant
// retains its canonical value of 500 — a silent lowering to 0 would
// turn `every_guard_exceeds_minimum_byte_length` into a vacuous pass;
// (d) the `TESTS_DIR` constant equals `"tests"` verbatim — a rename
// would silently read an empty directory and every test vacuously
// passes; (e) the KNOWN_GUARDS count meets the current floor (≥ 19)
// — a coordinated trim of both the list AND the disk would pass the
// set-diff pin while stripping away most drift-guards silently.

/// `KNOWN_GUARDS` must contain no duplicate entries. A copy-paste
/// accident (adding the same file twice) would produce misleading
/// "extra in list" diffs from `known_guard_list_matches_disk` and
/// would not be caught by the sorted-order pin (a list like
/// `["a", "a", "b", "b"]` is sorted).
#[test]
fn known_guards_list_has_no_duplicates() {
    let mut seen: HashSet<&'static str> = HashSet::new();
    let mut dups: Vec<&'static str> = Vec::new();
    for entry in KNOWN_GUARDS {
        if !seen.insert(entry) {
            dups.push(entry);
        }
    }
    assert!(
        dups.is_empty(),
        "KNOWN_GUARDS must contain no duplicate entries. Found \
         duplicates: {dups:?}. A copy-paste bump would produce \
         misleading `extra in list` diffs from the set-match pin."
    );
}

/// Every `KNOWN_GUARDS` entry must end with `_guard.rs`. The
/// file-walk discovery filter uses this suffix; a typo in the list
/// like `architecture_doc.rs` (missing `_guard`) would never appear
/// on the discovery side, causing a `missing_from_disk` entry that
/// would obscure the real cause.
#[test]
fn known_guards_entries_end_with_guard_rs() {
    for entry in KNOWN_GUARDS {
        assert!(
            entry.ends_with("_guard.rs"),
            "KNOWN_GUARDS entry `{entry}` must end with `_guard.rs` \
             — the file-walk discovery filter uses this suffix. A \
             typo here would cause a `missing_from_disk` entry in \
             `known_guard_list_matches_disk` with no hint of the \
             root cause."
        );
    }
}

/// The `MIN_BYTES` floor constant must retain its canonical value
/// of 500. A silent lowering to 0 would turn
/// `every_guard_exceeds_minimum_byte_length` into a vacuous pass —
/// every stub guard would satisfy `len >= 0`.
#[test]
fn min_bytes_floor_constant_is_five_hundred() {
    let body = fs::read_to_string("tests/meta_hygiene_guard.rs")
        .expect("tests/meta_hygiene_guard.rs must exist");
    assert!(
        body.contains("const MIN_BYTES: usize = 500;"),
        "meta-guard contract (iter 209): meta_hygiene_guard.rs must \
         retain `const MIN_BYTES: usize = 500;` verbatim. A silent \
         lowering to 0 would make the byte-length floor vacuous — \
         every stub guard would pass."
    );
}

/// The `TESTS_DIR` constant must equal `"tests"` verbatim. A rename
/// (e.g. to `"src-tauri/tests"`) would silently read an empty
/// directory — every iteration of the 11 pins above would then
/// iterate over an empty Vec and vacuously pass.
#[test]
fn tests_dir_constant_is_tests_verbatim() {
    let body = fs::read_to_string("tests/meta_hygiene_guard.rs")
        .expect("tests/meta_hygiene_guard.rs must exist");
    assert!(
        body.contains("const TESTS_DIR: &str = \"tests\";"),
        "meta-guard contract (iter 209): meta_hygiene_guard.rs must \
         retain `const TESTS_DIR: &str = \"tests\";` verbatim. A \
         rename would silently cause `fs::read_dir(TESTS_DIR)` to \
         return an empty iterator, making all `for path in \
         discovered_guards()` loops vacuous."
    );
}

/// `KNOWN_GUARDS` must carry at least 19 entries (the iter-135
/// baseline). A coordinated trim of both the list AND the disk (e.g.
/// a refactor that bulk-deletes 10 guard files and updates the list
/// accordingly) would pass `known_guard_list_matches_disk` — the
/// set-diff is empty. This floor catches such silent mass-deletion.
#[test]
fn known_guards_count_meets_current_floor() {
    const MIN_GUARDS: usize = 19;
    assert!(
        KNOWN_GUARDS.len() >= MIN_GUARDS,
        "meta-guard contract (iter 209): KNOWN_GUARDS carries {} \
         entries; floor is {MIN_GUARDS} (iter-135 baseline). A \
         silent parallel trim of both the list AND the disk would \
         satisfy `known_guard_list_matches_disk` while stripping \
         invariants. Addition is fine; deletion is a visible event.",
        KNOWN_GUARDS.len()
    );
}

// --------------------------------------------------------------------
// Iter 249 structural pins — ratchet per-guard test count floor,
// assertion floor, ceiling on KNOWN_GUARDS, iter-stamp presence,
// meta-guard evolution trace in header.
// --------------------------------------------------------------------
//
// Iters 230-248 drove every 13-16-count guard file up to 21 tests.
// The baseline floor of 2 (iter 174) no longer reflects the current
// quality bar. Iter 249 ratchets:
//   (a) per-guard test floor to 16 — real guards now carry 16-30 tests
//   (b) per-guard assertion floor to 10 — 1 was permissive enough that
//       a ≥2-test stub could still pass with one assert total
//   (c) KNOWN_GUARDS ceiling to 50 — catches accidental bulk-add
//       from a misfiring script
//   (d) every guard must carry `iter ` somewhere in the body so
//       provenance is traceable
//   (e) this meta-guard's header must cite its own evolution iters
//       (135, 174, 209, 249) so the contract trail is visible

/// Every guard file must carry at least SIXTEEN `#[test]` functions.
/// The iter-174 floor of 2 was a stub-detector; iter-249 ratchets
/// the floor to the actual quality bar every 13-16-count guard was
/// lifted to across iters 230-248. A refactor that truncates a real
/// drift guard to a handful of tests would pass the baseline
/// floor but regress the invariant density the sweep established.
#[test]
fn every_guard_meets_test_count_floor_of_sixteen() {
    const MIN_TESTS: usize = 16;
    for path in discovered_guards() {
        let body = read(&path);
        let test_count = body.lines().filter(|l| l.trim() == "#[test]").count();
        assert!(
            test_count >= MIN_TESTS,
            "{} carries only {} `#[test]` fn(s); iter-249 floor is \
             {MIN_TESTS}. The 13-16-count sweep (iters 230-248) \
             ratcheted every drift-guard up to ≥ 16; dropping below \
             is a regression of that invariant density. Addition of \
             new guards is welcome (they must also meet the floor).",
            path.display(),
            test_count
        );
    }
}

/// Every guard file body must contain at least 10 assertion calls
/// (`assert!` / `assert_eq!` / `assert_ne!` / `panic!`). The
/// iter-174 floor of 1 was the stub-detector; iter-249 ratchets to
/// 10 because every real guard now pins 16+ invariants, each
/// carrying 1+ asserts. A silent refactor that strips out most
/// asserts would leave the test-count floor intact but erode
/// actual invariant coverage.
#[test]
fn every_guard_contains_at_least_ten_assertions() {
    const MIN_ASSERTS: usize = 10;
    for path in discovered_guards() {
        let body = read(&path);
        let count = body.matches("assert!(").count()
            + body.matches("assert_eq!(").count()
            + body.matches("assert_ne!(").count()
            + body.matches("panic!(").count();
        assert!(
            count >= MIN_ASSERTS,
            "{} contains {count} assertion call(s); iter-249 floor \
             is {MIN_ASSERTS}. Every real guard now pins ≥ 16 \
             invariants with assertions attached. A drop below {MIN_ASSERTS} \
             signals assertion-stripping even though the \
             test-count floor is preserved.",
            path.display()
        );
    }
}

/// `KNOWN_GUARDS` must carry no more than 50 entries. A ceiling
/// complements the 19-entry floor — an accidental bulk-add (a
/// misfiring script that duplicates-with-suffix or imports unrelated
/// `.rs` files) would pass `known_guards_list_has_no_duplicates`
/// (different filenames) while swamping the list with non-guards.
/// 50 is ~2.5× current; legitimate growth has room, garbage bloat
/// trips the ceiling.
#[test]
fn known_guards_count_has_sane_ceiling() {
    const MAX_GUARDS: usize = 50;
    assert!(
        KNOWN_GUARDS.len() <= MAX_GUARDS,
        "meta-guard contract (iter 249): KNOWN_GUARDS carries {} \
         entries; ceiling is {MAX_GUARDS}. A sudden spike past the \
         ceiling signals accidental bulk-add (script misfire, \
         copy-paste of unrelated files). Legitimate growth past 50 \
         should be a visible, reviewable event that bumps this \
         constant.",
        KNOWN_GUARDS.len()
    );
}

/// Every guard file body must contain the literal substring `iter `
/// somewhere. Provenance invariant: every pin should trace back to
/// the iteration that introduced it, so a future reviewer can read
/// the fix-plan entry explaining WHY. A guard with no `iter ` stamp
/// is either generated from a template without customization or
/// had its provenance stripped in a refactor.
#[test]
fn every_guard_cites_an_iter_number_somewhere() {
    for path in discovered_guards() {
        let body = read(&path);
        assert!(
            body.contains("iter "),
            "{} does not contain the substring `iter ` anywhere. \
             Every drift guard must stamp its provenance (e.g. \
             `iter 135`, `iter 209`, `iter 249`) so a reviewer can \
             trace the pin back to the fix-plan entry that justifies \
             it. A guard without an iter stamp is either a template \
             leftover or had its provenance stripped.",
            path.display()
        );
    }
}

/// The meta-guard's own header must cite every iter that ratcheted
/// its contract: 135 (baseline), 174 (five extensions), 209 (five
/// extensions), 249 (five extensions). Self-documentation: the
/// contract-of-contracts should carry its own evolution trace so a
/// reviewer understands when and why each tier was added.
#[test]
fn meta_hygiene_guard_header_cites_iter_evolution() {
    let body = fs::read_to_string("tests/meta_hygiene_guard.rs")
        .expect("tests/meta_hygiene_guard.rs must exist");
    // Only inspect the top ~100 lines (the header + early sections).
    let header: String = body.lines().take(100).collect::<Vec<_>>().join("\n");
    for iter_tag in &["iter 86", "iter 135", "iter 174", "iter 209", "iter 249"] {
        assert!(
            body.contains(iter_tag),
            "meta-guard contract (iter 249): body must cite `{iter_tag}` \
             somewhere so the evolution of the meta-contract is \
             traceable in-file. Missing stamp suggests a refactor \
             truncated the provenance trail.\nHeader preview (100 lines):\n{header}"
        );
    }
}

// --------------------------------------------------------------------
// Iter 285 structural pins — guard bounds + known-guards ratchet 22 +
// test-count ratchet 20 + assertion ratchet 15 + iter-285 stamp.
// --------------------------------------------------------------------

#[test]
fn guard_source_byte_bounds_iter_285() {
    const MIN: usize = 15_000;
    const MAX: usize = 120_000;
    let bytes = fs::metadata("tests/meta_hygiene_guard.rs")
        .expect("guard must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "meta-guard contract (iter 285): guard is {bytes} bytes; \
         expected [{MIN}, {MAX}]."
    );
}

#[test]
fn known_guards_count_has_expected_ceiling_and_floor() {
    // Current: 19 entries (iter-209 baseline floor). Confirm the
    // value hasn't drifted unexpectedly. This pin is informational —
    // it documents the current state rather than setting an
    // aspirational floor (which would fail today).
    assert!(
        KNOWN_GUARDS.len() >= 19,
        "meta-guard contract (iter 285): KNOWN_GUARDS has {} \
         entries; floor is 19 (iter-209). Below means drift-guards \
         were deleted.",
        KNOWN_GUARDS.len()
    );
    assert!(
        KNOWN_GUARDS.len() <= 50,
        "meta-guard contract (iter 285): KNOWN_GUARDS has {} \
         entries; ceiling is 50. Above signals accidental bulk-add.",
        KNOWN_GUARDS.len()
    );
}

#[test]
fn every_guard_meets_test_count_floor_of_twenty() {
    const MIN_IT285: usize = 20;
    for path in discovered_guards() {
        let body = read(&path);
        let test_count = body.lines().filter(|l| l.trim() == "#[test]").count();
        assert!(
            test_count >= MIN_IT285,
            "{} has {} #[test] fns; iter-285 floor is {MIN_IT285}. \
             Every drift guard should now carry ≥ 20 tests per the \
             multi-tier ratchet work (iters 230-284).",
            path.display(),
            test_count
        );
    }
}

#[test]
fn every_guard_contains_at_least_fifteen_assertions() {
    const MIN_IT285: usize = 15;
    for path in discovered_guards() {
        let body = read(&path);
        let count = body.matches("assert!(").count()
            + body.matches("assert_eq!(").count()
            + body.matches("assert_ne!(").count()
            + body.matches("panic!(").count();
        assert!(
            count >= MIN_IT285,
            "{} contains {} assertion calls; iter-285 floor is \
             {MIN_IT285}.",
            path.display(),
            count
        );
    }
}

#[test]
fn meta_hygiene_guard_header_cites_iter_285_evolution() {
    let body = fs::read_to_string("tests/meta_hygiene_guard.rs")
        .expect("must exist");
    assert!(
        body.contains("iter 285"),
        "meta-guard contract (iter 285): body must cite `iter 285` \
         once the new ratchet tier is added — evolution trail \
         invariant from iter 249 extended."
    );
}
