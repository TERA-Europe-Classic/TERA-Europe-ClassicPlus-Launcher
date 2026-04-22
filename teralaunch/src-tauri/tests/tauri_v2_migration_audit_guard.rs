//! Tauri v1 → v2 migration audit-trail drift guard.
//!
//! The tauri-v2 migration shipped through 8 milestones (M0-M8) with
//! an audit-doc quartet under `docs/PRD/audits/security/`:
//!
//! - `tauri-v2-migration-baseline.md` — M0 baseline snapshot
//! - `tauri-v2-migration-plan.md` — migration plan
//! - `tauri-v2-migration.md` — umbrella audit
//! - `tauri-v2-migration-validation.md` — M8 validation sweep
//!
//! Together they comprise the audit trail that backs the fix-plan's
//! `tauri_v2_migration_milestone: M8-validated` header field and
//! `ready_for_squash_merge: true` status. Silent deletion of any one
//! would leave reviewers with an incomplete picture during the
//! post-squash audit.
//!
//! This guard pins file presence + a minimal-content sanity check
//! per doc so a file rename or truncation fails fast. Parallel to
//! iter 106 `architecture_doc_guard.rs` and iter 118 `anti_reverse_guard.rs`.

use std::fs;

const AUDIT_DIR: &str = "../../docs/PRD/audits/security";

struct DocFixture {
    filename: &'static str,
    /// Keywords that must appear in the doc body. Used as surface
    /// sanity check to catch truncation-to-stub.
    required_content: &'static [&'static str],
}

const DOCS: &[DocFixture] = &[
    DocFixture {
        filename: "tauri-v2-migration-baseline.md",
        required_content: &["Baseline", "M0"],
    },
    DocFixture {
        filename: "tauri-v2-migration-plan.md",
        required_content: &["Migration Plan"],
    },
    DocFixture {
        filename: "tauri-v2-migration.md",
        required_content: &["Tauri", "Migration"],
    },
    DocFixture {
        filename: "tauri-v2-migration-validation.md",
        required_content: &["Validation", "M8"],
    },
];

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Every expected audit doc must exist and carry the sanity-check
/// keywords that prove it's the right doc at the right stage.
#[test]
fn every_tauri_v2_audit_doc_exists_and_carries_required_content() {
    for doc in DOCS {
        let path = format!("{AUDIT_DIR}/{}", doc.filename);
        let body = read(&path);
        assert!(
            !body.trim().is_empty(),
            "Tauri-v2 migration audit doc {} exists but is empty. The \
             M0-M8 trail must not be truncated to a stub.",
            doc.filename
        );
        for needle in doc.required_content {
            assert!(
                body.contains(needle),
                "Tauri-v2 migration doc {} must contain `{needle}` as a \
                 surface sanity check (ensures file name still maps to \
                 the expected content).",
                doc.filename
            );
        }
    }
}

/// The umbrella doc must reference the PRD criterion it's written
/// against. §3.1.8 / §3.1.9 / §3.1.12 are the security-cluster gates
/// the migration touches; drift-guard on the umbrella doc catches a
/// future rename of those section numbers (would require coordinated
/// doc + PRD updates).
#[test]
fn umbrella_doc_cites_prd_criteria() {
    let body = read(&format!("{AUDIT_DIR}/tauri-v2-migration.md"));
    assert!(
        body.contains("3.1"),
        "Umbrella migration audit must cite PRD §3.1 (umbrella) so \
         future readers can trace back to the criterion."
    );
}

/// The validation doc must explicitly document the M8 state so the
/// fix-plan header field `tauri_v2_migration_milestone: M8-validated`
/// traces back to a specific signed-off audit.
#[test]
fn validation_doc_documents_m8_state() {
    let body = read(&format!("{AUDIT_DIR}/tauri-v2-migration-validation.md"));
    assert!(
        body.contains("M8"),
        "Validation doc must document M8 state (matches the \
         fix-plan header `tauri_v2_migration_milestone: M8-validated`)."
    );
    assert!(
        body.contains("Worktree") || body.contains("worktree"),
        "Validation doc must reference the worktree commit it \
         validated so a reader can git-log against that SHA."
    );
}

/// Per-doc minimum line count. All four docs shipped with 150+
/// lines; truncation past 50% is either a revert, a partial edit
/// mid-commit, or a stub replacement. Any of those wants a loud
/// signal at CI time.
const MIN_LINES_PER_DOC: usize = 100;

#[test]
fn every_audit_doc_meets_minimum_line_count() {
    for doc in DOCS {
        let path = format!("{AUDIT_DIR}/{}", doc.filename);
        let body = read(&path);
        let lines = body.lines().count();
        assert!(
            lines >= MIN_LINES_PER_DOC,
            "Tauri-v2 migration audit doc {} has only {} lines \
             (threshold: {}). The M0-M8 audit trail must carry \
             substantive content per doc — truncation points at a \
             stub replacement, partial edit, or silent revert.",
            doc.filename,
            lines,
            MIN_LINES_PER_DOC
        );
    }
}

/// The plan doc must cite the two key automation artefacts that
/// made the migration tractable at iter 62: `cargo tauri migrate`
/// (the automated command) and `v1Compatible` (the dual-format
/// updater flag). These are the load-bearing facts the plan
/// depends on — dropping either citation breaks the plan's chain
/// of reasoning.
#[test]
fn plan_doc_cites_key_automation_artefacts() {
    let body = read(&format!("{AUDIT_DIR}/tauri-v2-migration-plan.md"));
    assert!(
        body.contains("cargo tauri migrate"),
        "Plan doc must cite `cargo tauri migrate` — the automated \
         migration command is load-bearing on the iter-62 plan (it's \
         why the hand-porting sequence collapsed to tool-assisted)."
    );
    assert!(
        body.contains("v1Compatible"),
        "Plan doc must cite the `v1Compatible` updater flag — it's \
         the single-flag fix that let 0.1.x installs keep auto-\
         updating through the migration window."
    );
}

/// The umbrella doc must cite the three PRD §3.1 items v2 unlocks:
/// 3.1.8 (anti-reverse), 3.1.9 (updater-downgrade), 3.1.12 (CSP).
/// This is the dependency chain that justifies the whole migration;
/// without these citations the umbrella doc loses its motivating
/// rubric.
#[test]
fn umbrella_doc_cites_three_unlocked_prd_items() {
    let body = read(&format!("{AUDIT_DIR}/tauri-v2-migration.md"));
    for item in &["3.1.8", "3.1.9", "3.1.12"] {
        assert!(
            body.contains(item),
            "Umbrella migration doc must cite PRD §{item} — one of \
             the three v2-only features that motivate the migration. \
             Dropping any citation erases the dependency chain that \
             justifies the whole migration."
        );
    }
}

/// The baseline doc must cite a specific `main` commit SHA so the
/// pre-migration state is anchored to a re-checkoutable point.
/// Without the SHA, a future revalidator can't diff the baseline
/// against post-migration without guessing.
#[test]
fn baseline_doc_anchors_to_main_commit_sha() {
    let body = read(&format!("{AUDIT_DIR}/tauri-v2-migration-baseline.md"));
    // Look for a 7+-char hex SHA mentioned alongside `main` or
    // `commit` in the header region.
    let head: String = body.lines().take(15).collect::<Vec<_>>().join("\n");
    assert!(
        head.to_lowercase().contains("commit") || head.contains("@"),
        "Baseline doc header must reference a specific main commit \
         (either `commit <SHA>` or `main @ <SHA>` shape). Without \
         it, a future revalidator can't diff the baseline against \
         post-migration state."
    );
    // Stricter: some 7+ hex digits must appear in the first few lines.
    let has_sha = head.lines().take(10).any(|line| {
        let toks: Vec<&str> = line.split_whitespace().collect();
        toks.iter().any(|t| {
            let cleaned = t.trim_matches(|c: char| !c.is_ascii_hexdigit());
            cleaned.len() >= 7
                && cleaned.chars().all(|c| c.is_ascii_hexdigit())
                && cleaned.chars().any(|c| c.is_ascii_alphabetic())
        })
    });
    assert!(
        has_sha,
        "Baseline doc header must include a specific 7+-char hex SHA \
         (at least one hex digit) — anchors the pre-migration state \
         to a re-checkoutable commit."
    );
}

// --------------------------------------------------------------------
// Iter 172 structural pins — milestone enumeration + decision-rubric
// sections + ready-state semantics + iter-62 baseline + rollback
// pointer.
// --------------------------------------------------------------------
//
// Iter 122+147 pinned file presence + minimum line count + plan
// automation citations + 3-way PRD criterion list + baseline SHA.
// Iter 172 widens to the decision-rubric + readiness-state surface
// those pins skip: a plan doc that drops a milestone heading, a
// validation doc without the ready-state + rollback sections, an
// umbrella doc missing its Risks/Recommendation/Acceptance triple —
// any of these would weaken the squash-merge evidence trail without
// tripping the existing pins.

/// The plan doc must enumerate all ten milestones (M0 through M9) as
/// `### M<N>` headings. A missing milestone breaks the migration
/// narrative — e.g. dropping M6 would erase the anti-reverse gate
/// from the plan's dependency chain, even if the M6 audit doc
/// itself survives.
#[test]
fn plan_doc_enumerates_all_milestones_m0_through_m9() {
    let body = read(&format!("{AUDIT_DIR}/tauri-v2-migration-plan.md"));
    for n in 0..=9 {
        let heading = format!("### M{n}");
        assert!(
            body.contains(&heading),
            "Plan doc must carry the `{heading}` milestone heading \
             (followed by ` — <title>`). The M0-M9 sequence is the \
             migration's dependency chain — dropping any heading \
             erases that milestone from the plan's narrative even \
             if the corresponding audit doc survives."
        );
    }
}

/// The validation doc must carry the `## Ready state` section AND
/// declare the worktree ready for user-gated squash merge. This is
/// the single human-prose claim that backs the fix-plan header's
/// `ready_for_squash_merge: true`. If the doc drops the phrase, the
/// fix-plan's claim becomes unverifiable.
#[test]
fn validation_doc_has_ready_state_section_and_flags_user_gated_squash() {
    let body = read(&format!("{AUDIT_DIR}/tauri-v2-migration-validation.md"));
    assert!(
        body.contains("## Ready state"),
        "Validation doc must carry the `## Ready state` section. \
         Without it, there's no documented hand-off point between \
         `M8-validated` and the user-gated squash merge."
    );
    assert!(
        body.contains("ready for user-gated squash merge"),
        "Validation doc must carry the phrase \
         `ready for user-gated squash merge` — this is the human-\
         prose claim that the fix-plan's machine-parseable \
         `ready_for_squash_merge: true` traces back to."
    );
}

/// The validation doc's test-count diff section must cite `iter 62`
/// as the pre-migration baseline. Without the citation, the
/// "M0 → M8" columns lose their anchor and future revalidators
/// can't reproduce the comparison point.
#[test]
fn validation_doc_cites_iter_62_as_pre_migration_baseline() {
    let body = read(&format!("{AUDIT_DIR}/tauri-v2-migration-validation.md"));
    assert!(
        body.contains("iter 62"),
        "Validation doc must cite `iter 62` as the pre-migration \
         baseline. The test-count diff columns (`M0 (iter 62)` vs \
         `M8 (iter N)`) lose their anchor without the citation — \
         future revalidators can't reproduce the comparison point."
    );
}

/// The umbrella doc must carry its three decision-rubric headings:
/// `## Risks of staying on 1.x`, `## Risks of migrating`, and
/// `## Recommendation`. These are the sections that justify the
/// migration itself — dropping any breaks the evidence trail that
/// backs the M0-M8 work.
#[test]
fn umbrella_doc_carries_decision_rubric_sections() {
    let body = read(&format!("{AUDIT_DIR}/tauri-v2-migration.md"));
    for heading in [
        "## Risks of staying on 1.x",
        "## Risks of migrating",
        "## Recommendation",
        "## Acceptance",
    ] {
        assert!(
            body.contains(heading),
            "Umbrella doc must carry the `{heading}` section. The \
             Risks / Recommendation / Acceptance rubric is what \
             justifies the migration; dropping any section breaks \
             the evidence trail that backs M0-M8."
        );
    }
}

/// The validation doc must carry a `## Rollback pointer` section.
/// The ability to rollback is a load-bearing invariant — a
/// validation doc that declares readiness without documenting how
/// to back out is a one-way door, which is exactly the shape the
/// Rollback-strategy section in the plan doc was written to
/// prevent.
#[test]
fn validation_doc_has_rollback_pointer_section() {
    let body = read(&format!("{AUDIT_DIR}/tauri-v2-migration-validation.md"));
    assert!(
        body.contains("## Rollback pointer"),
        "Validation doc must carry the `## Rollback pointer` \
         section. A validation that declares readiness without \
         documenting the rollback path turns the squash merge into \
         a one-way door — defeating the whole reason the plan doc \
         carries a Rollback-strategy section."
    );
    // The pointer must actually reference the plan doc, otherwise
    // it's a stub heading.
    assert!(
        body.contains("tauri-v2-migration-plan.md") || body.contains("Rollback strategy"),
        "Rollback pointer section must reference the plan doc \
         (either `tauri-v2-migration-plan.md` or `Rollback \
         strategy`). A heading without the cross-ref is a stub."
    );
}

/// Self-test — prove the detectors bite on synthetic bad shapes.
#[test]
fn tauri_v2_audit_guard_detector_self_test() {
    // Bad shape A: doc present but empty.
    let empty = "";
    assert!(
        empty.trim().is_empty(),
        "self-test: empty body must trip the emptiness check"
    );

    // Bad shape B: doc present but with wrong content (e.g. the
    // file was replaced with a different doc).
    let wrong_doc = "# Completely unrelated content\n\nNo migration words here.\n";
    assert!(
        !wrong_doc.contains("Baseline") && !wrong_doc.contains("M8"),
        "self-test: unrelated doc must trip the content check"
    );

    // Bad shape C: validation doc missing M8 mention.
    let no_m8 = "# Validation\n\nSome text about M7.\n";
    assert!(
        !no_m8.contains("M8"),
        "self-test: missing M8 must trip the validation check"
    );

    // Bad shape D (iter 147): doc truncated below minimum.
    let truncated = "# Title\n\nStub.\n";
    assert!(
        truncated.lines().count() < MIN_LINES_PER_DOC,
        "self-test: truncated doc must fall under the threshold"
    );

    // Bad shape E: plan doc without `cargo tauri migrate`.
    let no_tool = "# Plan\n\nDo a manual port.\n";
    assert!(
        !no_tool.contains("cargo tauri migrate"),
        "self-test: plan without automation citation must be \
         flagged"
    );

    // Bad shape F: umbrella doc missing a §3.1 item.
    let partial_umbrella = "# Umbrella\n\nMigrate for 3.1.8 and 3.1.12.\n";
    assert!(
        !partial_umbrella.contains("3.1.9"),
        "self-test: umbrella missing 3.1.9 must be flagged"
    );

    // Bad shape G: baseline header without a SHA.
    let no_sha = "# Baseline\n\nCaptured at some time.\n";
    let head_lines: Vec<&str> = no_sha.lines().take(10).collect();
    let has_sha = head_lines.iter().any(|line| {
        let toks: Vec<&str> = line.split_whitespace().collect();
        toks.iter().any(|t| {
            let cleaned = t.trim_matches(|c: char| !c.is_ascii_hexdigit());
            cleaned.len() >= 7
                && cleaned.chars().all(|c| c.is_ascii_hexdigit())
                && cleaned.chars().any(|c| c.is_ascii_alphabetic())
        })
    });
    assert!(!has_sha, "self-test: baseline without SHA must be flagged");
}

// --------------------------------------------------------------------
// Iter 231 structural pins — guard-const canonicalisation, fixture-
// array cardinality, threshold literal pin, plan-side rollback pairing,
// doc-filename prefix invariant.
//
// Iter-122/147/172 cover file presence + line floor + citation
// requirements + decision rubric + rollback pointer. These five
// extend to the meta-guard / structural-integrity surface a confident
// refactor could silently drift: a renamed AUDIT_DIR (reads no files,
// every test fails with "file not found" pointing at the FS not the
// constant), a silently dropped DocFixture (set-cardinality falls to
// 3 without anyone noticing because the walk still succeeds), a
// lowered MIN_LINES_PER_DOC (turns the stub check vacuous), a plan
// doc without the rollback-strategy section (validation doc's
// rollback-pointer test points at a missing target), a renamed
// filename that breaks the prefix invariant.
// --------------------------------------------------------------------

/// Iter 231: `AUDIT_DIR` must stay `../../docs/PRD/audits/security`
/// verbatim. The relative path is relative to `src-tauri/` at test
/// runtime; drift to `docs/PRD/audits/security` (missing `../../`)
/// would work for tests run from the repo root but fail in CI's
/// working-directory layout. A misroute leaves every file-read with
/// a "file not found" pointing at the FS, not at the constant.
#[test]
fn guard_audit_dir_constant_is_canonical() {
    let guard_body = fs::read_to_string("tests/tauri_v2_migration_audit_guard.rs")
        .expect("guard source must exist");
    assert!(
        guard_body.contains("const AUDIT_DIR: &str = \"../../docs/PRD/audits/security\";"),
        "Iter 231: tests/tauri_v2_migration_audit_guard.rs must keep \
         `const AUDIT_DIR: &str = \"../../docs/PRD/audits/security\";` \
         verbatim. A drift to an absolute path or a different relative \
         route would read no files in CI's working-directory layout, \
         turning every `file not found` into a misrouted investigation."
    );
}

/// Iter 231: the `DOCS` fixture array must enumerate exactly FOUR
/// doc fixtures. Lifting the cardinality requires adding a new
/// milestone audit AND the fifth fixture together; lowering it
/// requires deleting a milestone audit AND a fixture — both
/// coordinated changes. A silent drop to 3 (e.g. removing the
/// baseline fixture while the other three tests keep passing)
/// would shrink the audit trail without anyone noticing.
#[test]
fn docs_array_enumerates_exactly_four_fixtures() {
    assert_eq!(
        DOCS.len(),
        4,
        "Iter 231: DOCS must enumerate exactly 4 fixtures (baseline, \
         plan, umbrella, validation). Current len={}. Lifting the \
         cardinality means adding a fifth milestone audit; lowering \
         means shrinking the M0-M8 trail — either is a coordinated \
         PRD/docs change, not a silent drift.",
        DOCS.len()
    );
    // Also pin that the four expected filenames are present in the
    // array. A swap (e.g. `-baseline` → `-migration-v2`) would pass
    // the cardinality check but change the semantic content.
    let names: Vec<&str> = DOCS.iter().map(|d| d.filename).collect();
    for expected in [
        "tauri-v2-migration-baseline.md",
        "tauri-v2-migration-plan.md",
        "tauri-v2-migration.md",
        "tauri-v2-migration-validation.md",
    ] {
        assert!(
            names.contains(&expected),
            "Iter 231: DOCS array must enumerate `{expected}`. Current \
             filenames: {names:?}. A rename of any of the four shifts \
             the audit trail to a different filename set without a PRD \
             change to match."
        );
    }
}

/// Iter 231: `MIN_LINES_PER_DOC` must stay `100` verbatim. A silent
/// lowering to 10 or 1 makes the "truncation is a stub replacement"
/// signal vacuous. A principled lift to 200 is fine, but drift in
/// either direction without a PRD note is a smell worth surfacing
/// in code review.
#[test]
fn min_lines_per_doc_literal_is_pinned_to_one_hundred() {
    let guard_body = fs::read_to_string("tests/tauri_v2_migration_audit_guard.rs")
        .expect("guard source must exist");
    assert!(
        guard_body.contains("const MIN_LINES_PER_DOC: usize = 100;"),
        "Iter 231: tests/tauri_v2_migration_audit_guard.rs must keep \
         `const MIN_LINES_PER_DOC: usize = 100;` verbatim. A silent \
         lowering (e.g. to 10 or 1) turns \
         `every_audit_doc_meets_minimum_line_count` into a vacuous \
         pass — a stub replacement slips through. A principled lift \
         is fine but should land with a PRD note, not as a drift."
    );
}

/// Iter 231: the plan doc must carry a `## Rollback strategy`
/// section. The validation doc's `## Rollback pointer` test (iter
/// 172) only asserts the pointer exists + references the plan; if
/// the plan doc itself drops the target section, the pointer
/// becomes a dead link. Pair the two: validation points AT, plan
/// documents HOW.
#[test]
fn plan_doc_carries_rollback_strategy_section() {
    let body = read(&format!("{AUDIT_DIR}/tauri-v2-migration-plan.md"));
    assert!(
        body.contains("## Rollback strategy") || body.contains("## Rollback"),
        "Iter 231: plan doc must carry `## Rollback strategy` (or at \
         minimum `## Rollback`). The validation doc's iter-172 pin \
         already asserts a `## Rollback pointer` that references the \
         plan — if the plan itself lacks the section, the pointer \
         points at nothing and the rollback contract is a dead link."
    );
}

/// Iter 231: every DOCS fixture's filename must start with the
/// `tauri-v2-migration` prefix. A rename to `tauri-2-migration-*`
/// (dropping `v`) or `tauri-migration-v2-*` (word swap) would shift
/// the audit-trail's filename convention; CI keeps passing because
/// the fixtures still enumerate the renamed files — but grep-
/// discoverability breaks for any tool keying off the original
/// prefix.
#[test]
fn every_docs_fixture_filename_starts_with_tauri_v2_migration() {
    for doc in DOCS {
        assert!(
            doc.filename.starts_with("tauri-v2-migration"),
            "Iter 231: DocFixture filename `{}` must start with \
             `tauri-v2-migration`. Dropping or reshaping the prefix \
             breaks grep-discoverability for any tooling that keys \
             off the migration's canonical filename scheme.",
            doc.filename
        );
        assert!(
            doc.filename.ends_with(".md"),
            "Iter 231: DocFixture filename `{}` must end with \
             `.md` — a non-markdown extension drift would leave the \
             audit-trail discoverability intact but change the \
             document format without a PRD note.",
            doc.filename
        );
    }
}

// --------------------------------------------------------------------
// Iter 269 structural pins — guard bounds + each audit doc bounds +
// MIN_LINES_PER_DOC constant + sec slot cite.
// --------------------------------------------------------------------

/// Iter 269: guard source byte bounds.
#[test]
fn guard_source_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 5000;
    const MAX_BYTES: usize = 80_000;
    let bytes = std::fs::metadata("tests/tauri_v2_migration_audit_guard.rs")
        .expect("guard must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "tauri-v2 (iter 269): guard is {bytes} bytes; expected \
         [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

/// Iter 269: every audit doc file must meet a byte-size floor. The
/// existing iter-198 pin checks line count; this pin adds a byte
/// floor to catch a doc with many empty lines that passes the line
/// pin vacuously.
#[test]
fn every_audit_doc_meets_byte_floor() {
    const MIN_BYTES: usize = 2000;
    for doc in DOCS {
        let path = format!("{AUDIT_DIR}/{}", doc.filename);
        let bytes = std::fs::metadata(&path)
            .unwrap_or_else(|e| panic!("{path}: {e}"))
            .len() as usize;
        assert!(
            bytes >= MIN_BYTES,
            "tauri-v2 (iter 269): {path} is {bytes} bytes; floor is \
             {MIN_BYTES}. A doc with padded empty lines passes \
             MIN_LINES_PER_DOC but loses real content."
        );
    }
}

/// Iter 269: MIN_LINES_PER_DOC constant must remain at the canonical
/// value (100). A silent lowering to 0 would vacate the iter-198
/// line-floor pin.
#[test]
fn min_lines_per_doc_constant_is_one_hundred() {
    let body = std::fs::read_to_string("tests/tauri_v2_migration_audit_guard.rs")
        .expect("guard must exist");
    assert!(
        body.contains("const MIN_LINES_PER_DOC: usize = 100;"),
        "tauri-v2 (iter 269): guard must retain \
         `const MIN_LINES_PER_DOC: usize = 100;` verbatim. A silent \
         lowering would vacate the per-doc line floor."
    );
}

/// Iter 269: guard header must cite the `sec.tauri-v1-eol-plan` slot
/// (the fix-plan name for this audit) — or the tauri-v2 label.
#[test]
fn guard_source_cites_tauri_v2_or_eol_plan_slot() {
    let body = std::fs::read_to_string("tests/tauri_v2_migration_audit_guard.rs")
        .expect("guard must exist");
    let header = &body[..body.len().min(500)];
    assert!(
        header.contains("tauri-v2") || header.contains("sec.tauri-v1-eol-plan"),
        "tauri-v2 (iter 269): guard header must cite `tauri-v2` or \
         the fix-plan slot `sec.tauri-v1-eol-plan`.\n\
         Header:\n{header}"
    );
}

/// Iter 269: every audit doc filename must start with `tauri-v2-` —
/// the canonical prefix. A file named `v2-migration.md` or
/// `migration.md` would pass individual pins but drift the naming
/// convention.
#[test]
fn every_audit_doc_filename_has_canonical_prefix() {
    for doc in DOCS {
        assert!(
            doc.filename.starts_with("tauri-v2-"),
            "tauri-v2 (iter 269): DocFixture filename `{}` must start \
             with `tauri-v2-` — the canonical prefix that groups the \
             4-doc quartet. Naming drift splits the audit-trail across \
             naming conventions.",
            doc.filename
        );
    }
}
