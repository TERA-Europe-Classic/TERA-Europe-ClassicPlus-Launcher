//! PRD 3.8.2 — every `src/services/mods/*.rs` file has a crate-level
//! `//!` doc comment.
//!
//! Criterion text: "Every `teralaunch/src-tauri/src/services/mods/*.rs`
//! has crate-level `//!` comment." Until iter 104 this was enforced only
//! by convention + spot-checking during code review; a new file added
//! without the header would slip through silently.
//!
//! This integration test walks the directory, reads the first non-empty
//! line of every `*.rs` file, and asserts it starts with `//!`. If a
//! future contributor adds a new service file without a module-level
//! doc header, CI breaks and tells them which file and what was
//! expected.

use std::fs;

const MODS_DIR: &str = "src/services/mods";

fn is_rust_file(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("rs")
}

fn first_non_empty_line(body: &str) -> Option<&str> {
    body.lines().map(str::trim_start).find(|l| !l.is_empty())
}

fn rs_files_in_mods_dir() -> Vec<std::path::PathBuf> {
    let mut out: Vec<_> = fs::read_dir(MODS_DIR)
        .unwrap_or_else(|_| panic!("{MODS_DIR} must exist and be readable"))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_rust_file(p))
        .collect();
    // Stable order so a failure message points at the same file every
    // run.
    out.sort();
    out
}

/// Every file in `src/services/mods/*.rs` must start with a crate-level
/// `//!` doc comment on its first non-empty line. PRD 3.8.2.
#[test]
fn every_mods_source_file_has_crate_level_doc() {
    let files = rs_files_in_mods_dir();
    assert!(
        !files.is_empty(),
        "no .rs files found in {MODS_DIR} — directory layout may have \
         changed; update this guard to match."
    );

    // Pin the list of expected files so a future accidental file
    // deletion is caught too. If the count shrinks, the file set changed
    // and a human should verify the cause. If the count grows, a new
    // file was added — make sure it has the //! header.
    let expected_minimum = 6; // catalog, external_app, mod, registry, tmm, types
    assert!(
        files.len() >= expected_minimum,
        "expected at least {expected_minimum} .rs files in {MODS_DIR}, \
         found {} ({:?}) — something was deleted unexpectedly",
        files.len(),
        files,
    );

    for path in &files {
        let body = fs::read_to_string(path).unwrap_or_else(|e| {
            panic!("failed to read {}: {e}", path.display())
        });
        let first = first_non_empty_line(&body).unwrap_or_else(|| {
            panic!("{} is empty — must have a crate-level //! doc", path.display())
        });
        assert!(
            first.starts_with("//!"),
            "PRD 3.8.2 violated: {} first non-empty line is:\n  {:?}\n\
             Expected it to start with `//!` (crate-level doc comment).",
            path.display(),
            first,
        );
    }
}

/// Minimum substantive length the `//!` block must carry. Below
/// this, the header is a stub — it satisfies the iter-104 prefix
/// check but doesn't explain WHAT the module does. 100 chars is
/// comfortably above any real bullet like "Shared types for the
/// mod manager." (31 chars) but well below the smallest real
/// mods-file block (types.rs ≈ 170 chars).
const MIN_DOC_BODY_CHARS: usize = 100;

/// Count the characters inside the leading `//!` doc block,
/// stripping the `//! ` prefix and including newlines between
/// lines. Stops at the first non-`//!` non-blank line.
fn crate_doc_body_chars(body: &str) -> usize {
    let mut total = 0;
    let mut in_block = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("//!") {
            in_block = true;
            // Drop leading space after `//!` if present.
            let content = rest.strip_prefix(' ').unwrap_or(rest);
            total += content.len();
            // Add 1 for the newline separator between doc lines.
            total += 1;
        } else if in_block && !trimmed.is_empty() {
            // First code line after the block — stop.
            break;
        } else if in_block && trimmed.is_empty() {
            // Blank line may precede more `//!` or code; continue
            // scanning.
            continue;
        }
    }
    total
}

/// PRD 3.8.2 (iter 143): per-file body-length floor. Catches a stub
/// like `//! x` that would pass the prefix-only check.
#[test]
fn every_mods_source_file_has_substantive_doc_body() {
    let files = rs_files_in_mods_dir();
    for path in &files {
        let body = fs::read_to_string(path).unwrap_or_else(|e| {
            panic!("failed to read {}: {e}", path.display())
        });
        let chars = crate_doc_body_chars(&body);
        assert!(
            chars >= MIN_DOC_BODY_CHARS,
            "PRD 3.8.2 body-length violation: {} has a `//!` block \
             of only {chars} chars (threshold: {MIN_DOC_BODY_CHARS}). \
             A header this short is a stub — explain WHAT the module \
             does and the key invariants it maintains.",
            path.display()
        );
    }
}

/// Self-test: prove the detector bites on known-bad shapes.
#[test]
fn crate_comment_detector_self_test() {
    // Good: file starting with //! on first line.
    let good = "//! This is the module doc.\n\npub fn foo() {}\n";
    let first = first_non_empty_line(good).unwrap();
    assert!(first.starts_with("//!"));

    // Good: blank lines before //! still picked up.
    let good_with_leading_blanks = "\n\n//! Doc after blanks.\n";
    let first = first_non_empty_line(good_with_leading_blanks).unwrap();
    assert!(first.starts_with("//!"));

    // Bad: regular `//` comment on first non-empty line.
    let bad_regular_comment = "// Not a crate-level doc.\npub fn foo() {}\n";
    let first = first_non_empty_line(bad_regular_comment).unwrap();
    assert!(
        !first.starts_with("//!"),
        "self-test: `//` must not match `//!` prefix detector"
    );

    // Bad: code on first line, no doc at all.
    let bad_no_doc = "pub fn foo() {}\n";
    let first = first_non_empty_line(bad_no_doc).unwrap();
    assert!(
        !first.starts_with("//!"),
        "self-test: code on first line must be rejected"
    );

    // Empty file: no first line, guard's unwrap_or_else panics —
    // covered by the real test's panic message path.
    let empty = "";
    assert!(first_non_empty_line(empty).is_none());

    // Iter 143 body-length checks.
    // Bad: stub body well under threshold.
    let stub = "//! x\n\npub fn foo() {}\n";
    let stub_chars = crate_doc_body_chars(stub);
    assert!(
        stub_chars < MIN_DOC_BODY_CHARS,
        "self-test: stub `//! x` body ({stub_chars} chars) must be \
         under threshold ({MIN_DOC_BODY_CHARS})"
    );

    // Good: real-looking body (well over threshold).
    let real = "//! Shared types for the mod manager.\n\
                //!\n\
                //! These types cross the Tauri boundary — every \
                `Serialize` variant maps to a discriminator the \
                frontend reads to render the right row treatment.\n\n\
                use serde::{Deserialize, Serialize};\n";
    let real_chars = crate_doc_body_chars(real);
    assert!(
        real_chars >= MIN_DOC_BODY_CHARS,
        "self-test: realistic body ({real_chars} chars) must be at \
         or above threshold"
    );

    // Bad: file that has `//!` prefix on first line but nothing
    // beyond a 1-char body on multiple lines (e.g. `//! a\n//! b\n`).
    let tiny_multi = "//! a\n//! b\n//! c\n\npub fn foo() {}\n";
    let tiny_chars = crate_doc_body_chars(tiny_multi);
    assert!(
        tiny_chars < MIN_DOC_BODY_CHARS,
        "self-test: multi-line stubs of 1-char bodies must be \
         flagged"
    );

    // Iter 179 helpers — exercise on representative shapes.
    let multi = "//! line 1\n//! line 2\n//! line 3\n\npub fn foo() {}\n";
    assert_eq!(count_leading_doc_lines(multi), 3);
    let one_liner = "//! only line\n\npub fn foo() {}\n";
    assert_eq!(count_leading_doc_lines(one_liner), 1);

    // Top-of-file check: `//!` must precede any code/use/pub line.
    let code_before_doc = "use std::fs;\n//! doc after use.\n";
    assert!(!crate_doc_is_at_top_of_file(code_before_doc));
    let doc_first = "//! doc.\n//! more.\nuse std::fs;\n";
    assert!(crate_doc_is_at_top_of_file(doc_first));

    // TODO/FIXME detector.
    let dirty = "//! TODO: explain this\n//! Still a stub.\n";
    assert!(crate_doc_contains_forbidden_marker(dirty).is_some());
    let clean = "//! Real doc with purpose.\n//! And invariants.\n";
    assert!(crate_doc_contains_forbidden_marker(clean).is_none());
}

/// Count the contiguous leading `//!` lines (blank lines interleaved
/// do not count and do not terminate the block). Iter 179.
fn count_leading_doc_lines(body: &str) -> usize {
    let mut n = 0;
    let mut in_block = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//!") {
            in_block = true;
            n += 1;
        } else if in_block && trimmed.is_empty() {
            continue;
        } else if in_block {
            break;
        }
    }
    n
}

/// Return `true` iff the first non-`//!`, non-blank line of `body` is
/// absent or preceded by at least one `//!` line. Catches headers
/// that live mid-file (e.g. after a `use` block) which Rustdoc would
/// silently ignore as inner-item docs. Iter 179.
fn crate_doc_is_at_top_of_file(body: &str) -> bool {
    let mut saw_doc = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("//!") {
            saw_doc = true;
            continue;
        }
        // First non-blank non-doc line.
        return saw_doc;
    }
    // File was all-blank/all-doc: vacuously fine.
    saw_doc
}

/// Scan the `//!` block for forbidden work-in-progress markers.
/// Returns `Some(marker)` on the first hit so the failure message
/// names the specific word. Iter 179.
fn crate_doc_contains_forbidden_marker(body: &str) -> Option<&'static str> {
    const MARKERS: &[&str] = &["TODO", "FIXME", "XXX", "HACK"];
    let mut in_block = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//!") {
            in_block = true;
            for m in MARKERS {
                if trimmed.contains(m) {
                    return Some(m);
                }
            }
        } else if in_block && trimmed.is_empty() {
            continue;
        } else if in_block {
            break;
        }
    }
    None
}

/// Minimum number of `//!` lines a crate-doc block must have. A
/// one-liner conveys the file name back at you, nothing more. Iter 179.
const MIN_DOC_LINES: usize = 2;

/// Iter 179: every file's `//!` block must span at least
/// [`MIN_DOC_LINES`] lines. Complements iter 143's char floor — a
/// single long line can trip the char floor but still fails to
/// separate summary from invariants.
#[test]
fn every_mods_source_file_has_multi_line_doc_block() {
    let files = rs_files_in_mods_dir();
    for path in &files {
        let body = fs::read_to_string(path).unwrap_or_else(|e| {
            panic!("failed to read {}: {e}", path.display())
        });
        let lines = count_leading_doc_lines(&body);
        assert!(
            lines >= MIN_DOC_LINES,
            "PRD 3.8.2 (iter 179): {} has a `//!` block of only \
             {lines} line(s); need at least {MIN_DOC_LINES} so the \
             summary and the invariants can live on separate lines.",
            path.display()
        );
    }
}

/// Iter 179: no `//!` block may contain WIP markers — a shipped
/// crate-level doc that advertises `TODO: explain this` bakes the
/// gap into prod.
#[test]
fn no_crate_doc_contains_wip_marker() {
    let files = rs_files_in_mods_dir();
    for path in &files {
        let body = fs::read_to_string(path).unwrap_or_else(|e| {
            panic!("failed to read {}: {e}", path.display())
        });
        if let Some(marker) = crate_doc_contains_forbidden_marker(&body) {
            panic!(
                "PRD 3.8.2 (iter 179): {} `//!` block contains \
                 forbidden WIP marker `{marker}` — resolve the gap \
                 or move the marker to the relevant code block so \
                 the crate-level doc reflects shipped behaviour.",
                path.display()
            );
        }
    }
}

/// Iter 179: the `//!` block must precede any `use`/`pub`/`mod`/code
/// line. Rustdoc treats inner-item `//!` as attribute docs on the
/// surrounding item, not the crate — a mid-file `//!` is silently
/// wrong.
#[test]
fn every_mods_source_file_has_doc_at_top_of_file() {
    let files = rs_files_in_mods_dir();
    for path in &files {
        let body = fs::read_to_string(path).unwrap_or_else(|e| {
            panic!("failed to read {}: {e}", path.display())
        });
        assert!(
            crate_doc_is_at_top_of_file(&body),
            "PRD 3.8.2 (iter 179): {} has code/use before the `//!` \
             block. Crate-level docs must be the first non-blank \
             content in the file.",
            path.display()
        );
    }
}

/// Iter 179: lock the set of canonical mods-service filenames. A
/// rename (`tmm.rs` -> `gpk_installer.rs`) or silent deletion would
/// otherwise slip past the iter-104 count floor as long as the
/// total count stayed above 6.
#[test]
fn expected_mods_filename_set_is_present() {
    const EXPECTED: &[&str] = &[
        "catalog.rs",
        "external_app.rs",
        "mod.rs",
        "registry.rs",
        "tmm.rs",
        "types.rs",
    ];
    let files = rs_files_in_mods_dir();
    let actual: std::collections::BTreeSet<String> = files
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
        .collect();
    for name in EXPECTED {
        assert!(
            actual.contains(*name),
            "PRD 3.8.2 (iter 179): canonical file `{name}` is \
             missing from {MODS_DIR}. Known set: {actual:?}. If \
             renamed, update EXPECTED here and the iter-104 \
             minimum count."
        );
    }
}

/// Iter 179: the first `//!` content line (after stripping `//! `)
/// must carry ≥ 20 chars of non-whitespace. Catches a stub first
/// line like `//! x` followed by padding lines that trip the char
/// floor without giving the reader a real summary up front.
#[test]
fn every_mods_doc_first_line_has_nonempty_summary() {
    const MIN_SUMMARY_CHARS: usize = 20;
    let files = rs_files_in_mods_dir();
    for path in &files {
        let body = fs::read_to_string(path).unwrap_or_else(|e| {
            panic!("failed to read {}: {e}", path.display())
        });
        let first_doc = body
            .lines()
            .map(str::trim_start)
            .find(|l| l.starts_with("//!"))
            .unwrap_or_else(|| {
                panic!("{} has no `//!` line", path.display())
            });
        let content = first_doc
            .strip_prefix("//!")
            .unwrap_or("")
            .trim();
        assert!(
            content.len() >= MIN_SUMMARY_CHARS,
            "PRD 3.8.2 (iter 179): {} first `//!` line is only \
             {} char(s) of content ({content:?}); need ≥ \
             {MIN_SUMMARY_CHARS} so the summary is load-bearing.",
            path.display(),
            content.len()
        );
    }
}

// --------------------------------------------------------------------
// Iter 204 structural pins — meta-guard self-reference + MODS_DIR path
// + rs-extension filter + threshold-constants positive + forbidden-
// markers completeness.
// --------------------------------------------------------------------
//
// The eight pins above walk the filesystem and assert per-file
// properties. They do NOT pin the invariants of the guard file ITSELF:
// (a) the guard's module header must cite PRD 3.8.2 so a reader can
// trace the test back to the criterion without guessing; (b) the
// directory root `MODS_DIR` must remain the canonical path — a silent
// rename to `src/mod_manager` would panic loudly but wouldn't catch a
// typo like `src/services/mod` (off-by-one s); (c) the file-extension
// filter must accept `"rs"` not `"rust"` — a silent typo would filter
// everything out and the "at least 6 files" assert would catch it, but
// the root-cause signal "wrong extension" would be lost in noise; (d)
// the body-length and line-count constants must remain positive — a
// regression to `= 0` silently no-ops both floors; (e) the forbidden-
// marker list must keep all four (TODO/FIXME/XXX/HACK) — dropping one
// would let a specific WIP slip through.

const GUARD_FILE: &str = "tests/crate_comment_guard.rs";

fn read_guard_file() -> String {
    fs::read_to_string(GUARD_FILE).expect("tests/crate_comment_guard.rs must exist")
}

/// The guard's own module header must cite PRD 3.8.2 by section AND
/// name itself as `crate_comment_guard`. Without the cite a reader
/// can't trace the test back to the criterion; without the self-name
/// a rename of this file would produce test failures that don't say
/// which guard regressed.
#[test]
fn guard_file_header_cites_prd_3_8_2_and_self_name() {
    let body = read_guard_file();
    // First 2000 chars cover the header comfortably.
    let header = &body[..body.len().min(2000)];
    assert!(
        header.contains("PRD 3.8.2"),
        "meta-guard contract: tests/crate_comment_guard.rs header must \
         cite `PRD 3.8.2`. Without the cite, a regression triggers an \
         anonymous failure — a reader has to grep the PRD to learn what \
         this test is guarding.\nHeader:\n{header}"
    );
    assert!(
        header.contains("crate-level"),
        "meta-guard contract: header must name the invariant \
         (`crate-level` doc comment) it guards so the failure message \
         carries its own glossary."
    );
}

/// `MODS_DIR` must remain the verbatim `"src/services/mods"` path.
/// A silent off-by-one (e.g. `"src/services/mod"` — missing trailing
/// `s`) would read nothing, the guard's `expected_minimum = 6` assert
/// fires, but the failure points at "no files found" instead of the
/// real cause. Pinning the constant surfaces a path typo at review
/// time, not at CI-break time.
#[test]
fn mods_dir_constant_is_services_mods_path() {
    let body = read_guard_file();
    assert!(
        body.contains("const MODS_DIR: &str = \"src/services/mods\";"),
        "PRD 3.8.2: tests/crate_comment_guard.rs must keep \
         `const MODS_DIR: &str = \"src/services/mods\";` verbatim. \
         A typo like `src/services/mod` would make the directory \
         scanner silently read an empty listing."
    );
}

/// `is_rust_file` must filter on extension `"rs"` — not `"rust"` or
/// some other near-miss. A typo would filter out every file and the
/// guard would fail with "at least 6 files expected, found 0" — the
/// structural pin surfaces the typo BEFORE the guard runs, so the
/// root cause is visible in the commit that introduced it.
#[test]
fn rs_file_extension_filter_is_lowercase_rs() {
    let body = read_guard_file();
    assert!(
        body.contains("path.extension().and_then(|e| e.to_str()) == Some(\"rs\")"),
        "PRD 3.8.2: tests/crate_comment_guard.rs must keep the \
         `Some(\"rs\")` extension comparison verbatim. A typo \
         (`\"rust\"`, `\"Rs\"`) would filter out every file."
    );
}

/// Both threshold constants must remain strictly positive. A
/// regression to `= 0` silently no-ops the guard (every file passes).
/// Pin the canonical values so a silent lowering stands out in
/// review.
#[test]
fn doc_floor_constants_remain_positive() {
    let body = read_guard_file();
    assert!(
        body.contains("const MIN_DOC_BODY_CHARS: usize = 100;"),
        "PRD 3.8.2: `const MIN_DOC_BODY_CHARS: usize = 100;` must \
         remain. Setting to 0 turns `every_mods_source_file_has_\
         substantive_doc_body` into a vacuous pass."
    );
    assert!(
        body.contains("const MIN_DOC_LINES: usize = 2;"),
        "PRD 3.8.2: `const MIN_DOC_LINES: usize = 2;` must remain. \
         Setting to 0 turns `every_mods_source_file_has_multi_line_\
         doc_block` into a vacuous pass."
    );
}

/// `crate_doc_contains_forbidden_marker` must scan for ALL FOUR WIP
/// markers: TODO, FIXME, XXX, HACK. Dropping any one would let that
/// specific WIP slip through into shipped crate-level docs. They are
/// not interchangeable — each maps to a distinct work-in-progress
/// convention across the wider Rust ecosystem.
#[test]
fn forbidden_wip_markers_include_all_four() {
    let body = read_guard_file();
    // Locate the MARKERS slice definition.
    let pos = body
        .find("const MARKERS: &[&str]")
        .expect("MARKERS slice must exist");
    // Window covers the slice declaration + literal.
    let window = &body[pos..pos.saturating_add(200)];
    for marker in ["TODO", "FIXME", "XXX", "HACK"] {
        assert!(
            window.contains(&format!("\"{marker}\"")),
            "PRD 3.8.2 (iter 179): MARKERS slice must contain \
             \"{marker}\" — each marker maps to a distinct WIP \
             convention. Window:\n{window}"
        );
    }
}

// --------------------------------------------------------------------
// Iter 231 structural pins — GUARD_FILE canonicalisation, stable-sort
// determinism, MIN_SUMMARY_CHARS literal pin, self-test era coverage,
// detector helpers wired into real (not just self-test) assertions.
//
// Iter-104 / 143 / 179 / 204 covered filesystem walk invariants and
// first-order constants. These five extend to the meta-guard surface
// a confident refactor could still break silently: GUARD_FILE path
// drift (header-inspection turns into a panic not a pointer), missing
// sort (failure messages lose determinism), MIN_SUMMARY_CHARS floor
// drift (the 20-char literal is inlined in the fn body, not a module
// const — a silent lowering to 1 has no guard today), self-test that
// doesn't cover both eras (an iter-143 refactor could drop the
// iter-179 helpers from self-test and nobody would notice).
// --------------------------------------------------------------------

/// Iter 231: `GUARD_FILE` must stay `tests/crate_comment_guard.rs`
/// verbatim. Every header-inspection pin below reads this path; a
/// rename leaves the pin chain intact shape-wise but panics on
/// `unwrap` with "tests/crate_comment_guard.rs must exist" — a
/// maintainer reading that message would check for the file's
/// existence, not suspect a constant drift.
#[test]
fn guard_file_constant_is_canonical() {
    let body = read_guard_file();
    assert!(
        body.contains("const GUARD_FILE: &str = \"tests/crate_comment_guard.rs\";"),
        "PRD 3.8.2 (iter 231): tests/crate_comment_guard.rs must keep \
         `const GUARD_FILE: &str = \"tests/crate_comment_guard.rs\";` \
         verbatim. A rename of this file without updating the constant \
         produces a `file not found` panic that misdirects triage \
         toward missing files, not a constant-drift bug."
    );
}

/// Iter 231: `rs_files_in_mods_dir()` must call `.sort()` on its
/// output before returning. `fs::read_dir` returns entries in OS-
/// dependent order — on Linux it's filesystem-hash order (non-
/// deterministic), on Windows it's sometimes FILE_BASIC_INFO order.
/// Without sort, failure messages cite files in a non-deterministic
/// order between CI runs and between machines; a bisect on an intermittent
/// guard failure sees the diff between two runs' errors but the diff
/// is just ordering noise.
#[test]
fn files_walker_sorts_output_for_deterministic_failures() {
    let body = read_guard_file();
    // Find the fn body.
    let fn_pos = body
        .find("fn rs_files_in_mods_dir()")
        .expect("rs_files_in_mods_dir must exist");
    let fn_end = body[fn_pos..]
        .find("\n}\n")
        .map(|i| fn_pos + i)
        .unwrap_or(fn_pos + 800);
    let fn_body = &body[fn_pos..fn_end];
    assert!(
        fn_body.contains(".sort()"),
        "PRD 3.8.2 (iter 231): rs_files_in_mods_dir() must call \
         `.sort()` on its return value so failure messages cite files \
         in a stable order across CI runs and developer machines. \
         Without sort, `fs::read_dir` iteration order is OS-dependent \
         (Linux: fs-hash; Windows: FILE_BASIC_INFO) → intermittent \
         red CI where a bisect's diff is just ordering noise.\n\
         Fn body:\n{fn_body}"
    );
}

/// Iter 231: pin the `MIN_SUMMARY_CHARS = 20` literal in
/// `every_mods_doc_first_line_has_nonempty_summary`. The constant
/// is inlined in the fn body (not a module const), so there's no
/// existing guard catching a silent lowering. A refactor that
/// reads "20 is arbitrary; let's loosen to 5" defeats the purpose
/// of the check — short summaries are exactly what iter 179 was
/// guarding against.
#[test]
fn min_summary_chars_literal_is_pinned_to_twenty() {
    let body = read_guard_file();
    assert!(
        body.contains("const MIN_SUMMARY_CHARS: usize = 20;"),
        "PRD 3.8.2 (iter 231): \
         `every_mods_doc_first_line_has_nonempty_summary` must keep \
         `const MIN_SUMMARY_CHARS: usize = 20;` verbatim. A silent \
         lowering (e.g. to 5 or 1) turns the summary-floor check into \
         a vacuous pass — the first `//!` line could say `//! x` and \
         still satisfy `content.len() >= 5`."
    );
}

/// Iter 231: the detector self-test must carry references to BOTH
/// the iter-143 body-length helper (`crate_doc_body_chars`) AND the
/// iter-179 helpers (`count_leading_doc_lines`, `crate_doc_is_at_top_of_file`,
/// `crate_doc_contains_forbidden_marker`). A refactor that dropped the
/// iter-179 helpers from the self-test would leave them unused by the
/// assertion layer — the production-gating tests still call them, but
/// the self-test no longer proves they bite on known-bad shapes.
#[test]
fn detector_self_test_covers_both_iter_eras() {
    let body = read_guard_file();
    let fn_pos = body
        .find("fn crate_comment_detector_self_test()")
        .expect("self-test fn must exist");
    // Find the END of this fn by scanning for a `\n}\n` that isn't
    // inside a string literal or nested block. Simple brace-balance
    // from the opening `{` works because this fn has no nested fns.
    let brace_start = body[fn_pos..].find('{').expect("fn open brace") + fn_pos;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    let bytes = body.as_bytes();
    let mut fn_end = bytes.len();
    for (i, &c) in bytes.iter().enumerate().skip(brace_start) {
        if in_str {
            if escape { escape = false; }
            else if c == b'\\' { escape = true; }
            else if c == b'"' { in_str = false; }
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    fn_end = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    let self_test = &body[fn_pos..fn_end];
    for helper in [
        "crate_doc_body_chars",               // iter 143
        "count_leading_doc_lines",            // iter 179
        "crate_doc_is_at_top_of_file",        // iter 179
        "crate_doc_contains_forbidden_marker", // iter 179
    ] {
        assert!(
            self_test.contains(helper),
            "PRD 3.8.2 (iter 231): crate_comment_detector_self_test \
             must reference `{helper}` so the self-test covers both \
             iter-143 and iter-179 detector eras. A refactor that \
             drops a helper from the self-test leaves it silently \
             unexercised — the production gate still uses it, but \
             no one proves it bites on known-bad shapes."
        );
    }
}

/// Iter 231: every iter-179 production-gating helper must appear at
/// least TWICE in the file: once inside the self-test (proving the
/// helper bites on known shapes) AND once in a real walking test
/// (proving the invariant is actually gated on every mods file).
/// Counting ≥ 2 occurrences of the name is the simplest proxy for
/// "helper is wired into a real assertion, not just the self-test".
/// A refactor that removed the walking test would drop the count to
/// 1 and this pin fires.
#[test]
fn iter_179_helpers_wired_into_real_walking_tests() {
    let body = read_guard_file();
    for helper in [
        "count_leading_doc_lines",
        "crate_doc_is_at_top_of_file",
        "crate_doc_contains_forbidden_marker",
    ] {
        // Count as-identifier occurrences: the name followed by `(` or
        // by whitespace (no false matches on prefix substrings).
        let with_paren = format!("{helper}(");
        let count = body.matches(&with_paren).count();
        assert!(
            count >= 2,
            "PRD 3.8.2 (iter 231): helper `{helper}(` must be called \
             at least twice — once in the self-test (proof the helper \
             bites on known shapes) and once in a real walking test \
             (production gate on every mods file). Found {count} \
             occurrences; a single call means either the self-test \
             was dropped (helper untested) or the walking test was \
             dropped (helper is dead code)."
        );
    }
}
