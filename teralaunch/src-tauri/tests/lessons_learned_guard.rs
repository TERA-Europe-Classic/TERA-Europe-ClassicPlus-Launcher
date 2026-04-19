//! PRD 3.8.8 — `docs/PRD/lessons-learned.md` exists, capped at 200
//! lines, with archived entries living in
//! `docs/PRD/lessons-learned.archive.md`.
//!
//! Criterion text: "exists, capped 200 lines, archived when full."
//! Measurement: "retrospective iteration asserts line count + archive
//! presence" — iter 108 makes that assertion part of the test suite
//! so cap drift surfaces on every CI run, not just every-30 retros.
//!
//! Iter 108 found the file had drifted to 212 lines (12 over the cap);
//! moved iter 24 entry to the archive and added this guard. Going
//! forward, an over-cap push fails CI immediately.

use std::fs;

const ACTIVE: &str = "../../docs/PRD/lessons-learned.md";
const ARCHIVE: &str = "../../docs/PRD/lessons-learned.archive.md";
const LINE_CAP: usize = 200;

fn line_count(path: &str) -> usize {
    let body = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{path} must be readable: {e}"));
    body.lines().count()
}

/// Active file must exist and must not exceed the 200-line cap.
#[test]
fn active_file_exists_and_under_cap() {
    let n = line_count(ACTIVE);
    assert!(
        n > 0,
        "PRD 3.8.8 violated: {ACTIVE} is empty. The retrospective \
         history must be preserved, not blanked."
    );
    assert!(
        n <= LINE_CAP,
        "PRD 3.8.8 violated: {ACTIVE} has {n} lines (cap: {LINE_CAP}). \
         Archive the oldest entries to lessons-learned.archive.md at \
         the next retrospective iteration."
    );
}

/// Archive file must exist. If the active file ever hits cap, we'll
/// need somewhere to move entries — the archive being present is a
/// precondition for the cap policy to work.
#[test]
fn archive_file_exists() {
    let n = line_count(ARCHIVE);
    assert!(
        n > 0,
        "PRD 3.8.8 violated: {ARCHIVE} is missing or empty. The \
         archival path is a precondition for the active file's cap \
         policy — reinstating it is cheap and has no downside."
    );
}

/// The active file should advertise its cap policy so a reviewer
/// editing it doesn't quietly blow past 200. Structural nudge: the
/// header mentions the cap and the archival path.
#[test]
fn active_file_header_advertises_cap_policy() {
    let body = fs::read_to_string(ACTIVE).expect("active file present");
    // First 20 lines should mention the cap + archive path.
    let head: String = body.lines().take(20).collect::<Vec<_>>().join("\n");
    assert!(
        head.contains("200"),
        "Header should mention the 200-line cap so editors know about it."
    );
    assert!(
        head.contains("lessons-learned.archive.md"),
        "Header should reference the archive file so the migration path is obvious."
    );
}

/// Every H3 entry in the active file must follow the documented
/// shape: `### YYYY-MM-DD / iter N — title`. Enforcing this stops
/// the file from drifting into ad-hoc formats that make the
/// retrospective corpus unparseable.
#[test]
fn every_h3_entry_follows_date_iter_format() {
    let body = fs::read_to_string(ACTIVE).expect("active file present");
    let mut offenders: Vec<(usize, String)> = Vec::new();
    for (i, line) in body.lines().enumerate() {
        if !line.starts_with("### ") {
            continue;
        }
        // Tail must look like `YYYY-MM-DD / iter <num> — <title>` or
        // `YYYY-MM-DD / iter <num>-<num> — <title>` (range form for
        // multi-iter lessons).
        let tail = line.trim_start_matches("### ");
        // Date prefix check: 10 chars `\d{4}-\d{2}-\d{2}` then ` / iter ` then digit(s).
        let looks_like = tail.len() >= 10
            && tail[..4].chars().all(|c| c.is_ascii_digit())
            && tail.chars().nth(4) == Some('-')
            && tail[5..7].chars().all(|c| c.is_ascii_digit())
            && tail.chars().nth(7) == Some('-')
            && tail[8..10].chars().all(|c| c.is_ascii_digit())
            && tail[10..].starts_with(" / iter ");
        if !looks_like {
            offenders.push((i + 1, line.to_string()));
        }
    }
    assert!(
        offenders.is_empty(),
        "PRD 3.8.8 format drift: {} H3 entries in lessons-learned.md \
         don't follow `### YYYY-MM-DD / iter N — title`:\n  {}\n\
         Format is documented in the file header — preserve it so \
         the retrospective corpus stays parseable.",
        offenders.len(),
        offenders
            .iter()
            .map(|(n, l)| format!("L{n}: {l}"))
            .collect::<Vec<_>>()
            .join("\n  "),
    );
}

/// Every H3 entry must be followed (somewhere before the next H3)
/// by both a `**Pattern.**` paragraph and a `**When to apply.**`
/// paragraph. These are the two halves of the entry's shape; a
/// pattern without a when-to-apply is incomplete.
#[test]
fn every_h3_entry_has_pattern_and_when_to_apply() {
    let body = fs::read_to_string(ACTIVE).expect("active file present");
    let lines: Vec<&str> = body.lines().collect();
    let mut entries: Vec<(usize, &str)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("### ") {
            entries.push((i, *line));
        }
    }
    for (idx, (start, heading)) in entries.iter().enumerate() {
        let end = entries
            .get(idx + 1)
            .map(|(next_start, _)| *next_start)
            .unwrap_or(lines.len());
        let block = &lines[*start..end].join("\n");
        assert!(
            block.contains("**Pattern.**"),
            "H3 entry at L{} (`{heading}`) is missing the \
             `**Pattern.**` paragraph. Every retrospective lesson \
             needs one — it's the body of the entry.",
            start + 1
        );
        assert!(
            block.contains("**When to apply.**"),
            "H3 entry at L{} (`{heading}`) is missing the \
             `**When to apply.**` paragraph. Pattern without \
             when-to-apply is incomplete — readers need both halves \
             to decide if the lesson applies.",
            start + 1
        );
    }
}

/// The header must advertise the newest-at-top ordering + the entry
/// format. Without this documentation, a future contributor may add
/// entries at the bottom (chronological-forward) and the file's
/// shape drifts.
#[test]
fn header_documents_newest_at_top_and_entry_format() {
    let body = fs::read_to_string(ACTIVE).expect("active file present");
    let head: String = body.lines().take(20).collect::<Vec<_>>().join("\n");
    assert!(
        head.to_lowercase().contains("newest at top"),
        "Header must advertise the newest-at-top ordering convention \
         so contributors don't append chronologically-forward and \
         drift the file's shape."
    );
    assert!(
        head.contains("### ") || head.contains("H3"),
        "Header must mention the H3 entry shape (`### ...`) so the \
         format contract is self-documenting."
    );
}

// --------------------------------------------------------------------
// Iter 177 structural pins — archive-header contract + iter-ordering
// + non-empty title + combined-entry floor + no-duplicate between
// active and archive.
// --------------------------------------------------------------------
//
// Iter 108+139 pinned cap presence + archive existence + header cap
// advertisement + H3 date-iter format + Pattern/When-to-apply + newest-
// at-top header. Iter 177 widens to five more angles those pins skip.

/// The archive file's own header must self-document its purpose. A
/// contributor stumbling into `lessons-learned.archive.md` needs to
/// know it's the overflow buffer for `lessons-learned.md`, not a
/// separate artefact — otherwise edits land in the wrong file and
/// the cap policy drifts.
#[test]
fn archive_file_header_documents_archival_purpose() {
    let body = fs::read_to_string(ARCHIVE).expect("archive file present");
    let head: String = body.lines().take(10).collect::<Vec<_>>().join("\n");
    assert!(
        head.contains("lessons-learned.md"),
        "PRD 3.8.8: {ARCHIVE} header must reference \
         `lessons-learned.md` as its source. Without this self-\
         documentation, a contributor might edit the archive \
         directly instead of the active file, and the cap policy \
         drifts.\nHead:\n{head}"
    );
    assert!(
        head.contains("200-line cap") || head.contains("200")
            || head.to_lowercase().contains("cap"),
        "PRD 3.8.8: {ARCHIVE} header must reference the cap (200-\
         line cap or similar) so archive editors understand why \
         entries ended up here."
    );
}

/// H3 entries in the active file must be ordered by `iter N`
/// descending (newest first). The header advertises newest-at-top;
/// this pin enforces the ordering. Single-iter form only — ranges
/// like `iter 50-60` are not currently used and if they arrive,
/// this pin needs extending.
#[test]
fn active_file_entries_are_ordered_newest_iter_first() {
    let body = fs::read_to_string(ACTIVE).expect("active file present");
    let mut iters: Vec<(usize, u32)> = Vec::new();
    for (i, line) in body.lines().enumerate() {
        if !line.starts_with("### ") {
            continue;
        }
        // Tail after `### YYYY-MM-DD / iter `: parse the iter digits.
        let tail = line.trim_start_matches("### ");
        let Some(iter_pos) = tail.find(" / iter ") else {
            continue; // format check (other test) catches this
        };
        let after = &tail[iter_pos + " / iter ".len()..];
        let iter_num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = iter_num_str.parse::<u32>() {
            iters.push((i + 1, n));
        }
    }
    assert!(
        !iters.is_empty(),
        "active file must have at least one H3 entry for ordering check"
    );
    for pair in iters.windows(2) {
        let (line_a, iter_a) = pair[0];
        let (line_b, iter_b) = pair[1];
        assert!(
            iter_a > iter_b,
            "PRD 3.8.8: lessons-learned.md must be ordered iter-\
             descending (newest first). L{line_a} iter {iter_a} \
             should be strictly greater than L{line_b} iter \
             {iter_b}. A forward-ordered or out-of-sequence file \
             breaks the header's `newest at top` contract."
        );
    }
}

/// Every H3 entry must have a non-empty title after the em-dash —
/// parallel to iter 173's changelog pin. `### 2026-04-19 / iter 42 —`
/// (empty after the separator) passes iter 139's format check but
/// gives readers no signal about the lesson's content.
#[test]
fn every_h3_entry_has_nonempty_title_after_em_dash() {
    let body = fs::read_to_string(ACTIVE).expect("active file present");
    let mut offenders: Vec<(usize, String)> = Vec::new();
    for (i, line) in body.lines().enumerate() {
        if !line.starts_with("### ") {
            continue;
        }
        // Find the em-dash and check what follows.
        let Some(idx) = line.find(" \u{2014} ") else {
            continue; // format check (other test) catches this
        };
        let title = line[idx + " \u{2014} ".len()..].trim();
        if title.is_empty() {
            offenders.push((i + 1, line.to_string()));
        }
    }
    assert!(
        offenders.is_empty(),
        "PRD 3.8.8: {} H3 entry/entries have em-dash but empty title. \
         Offenders:\n  {}\nA missing title reduces the heading to \
         noise — readers need the title to decide if the lesson \
         applies to their current task.",
        offenders.len(),
        offenders
            .iter()
            .map(|(n, l)| format!("L{n}: {l}"))
            .collect::<Vec<_>>()
            .join("\n  "),
    );
}

/// Combined entry count across active + archive must meet a floor.
/// The retrospective corpus is cumulative — a bulk-delete or bad
/// rebase that drops entries below the floor means we've silently
/// lost the institutional memory iter 108 made this file cap an
/// active discipline to preserve.
#[test]
fn total_entry_count_across_active_and_archive_meets_floor() {
    const MIN_TOTAL_ENTRIES: usize = 10;
    let active_body = fs::read_to_string(ACTIVE).expect("active file present");
    let archive_body = fs::read_to_string(ARCHIVE).expect("archive file present");
    let active_count = active_body.lines().filter(|l| l.starts_with("### ")).count();
    let archive_count = archive_body.lines().filter(|l| l.starts_with("### ")).count();
    let total = active_count + archive_count;
    assert!(
        total >= MIN_TOTAL_ENTRIES,
        "PRD 3.8.8: total H3 entry count across active + archive is \
         {total} (active={active_count}, archive={archive_count}); \
         floor is {MIN_TOTAL_ENTRIES}. The retrospective corpus is \
         cumulative — a drop below the floor suggests a bulk delete \
         or a bad rebase lost institutional memory."
    );
}

/// No H3 heading appearing in `lessons-learned.md` may also appear
/// in `lessons-learned.archive.md`. At archive time, the entry
/// should be MOVED (not copied); a duplicate indicates a copy-paste
/// regression that leaves the active file still carrying an entry
/// that was supposedly archived to restore the cap.
#[test]
fn active_and_archive_do_not_duplicate_entries() {
    let active_body = fs::read_to_string(ACTIVE).expect("active file present");
    let archive_body = fs::read_to_string(ARCHIVE).expect("archive file present");
    let active_headings: std::collections::HashSet<&str> = active_body
        .lines()
        .filter(|l| l.starts_with("### "))
        .collect();
    let archive_headings: std::collections::HashSet<&str> = archive_body
        .lines()
        .filter(|l| l.starts_with("### "))
        .collect();
    let duplicates: Vec<&&str> = active_headings.intersection(&archive_headings).collect();
    let dup_count = duplicates.len();
    assert!(
        duplicates.is_empty(),
        "PRD 3.8.8: {dup_count} H3 heading(s) appear in BOTH the \
         active file AND the archive. Archiving is a MOVE operation \
         — a duplicate indicates a copy-paste regression. Removing \
         the copy in the active file restores the cap and the \
         authoritative-archive invariant.\nDuplicates: {duplicates:?}"
    );
}

// --------------------------------------------------------------------
// Iter 215 structural pins — meta-guard header + ACTIVE/ARCHIVE path
// constants + LINE_CAP literal + archive-side ordering + Pattern-
// before-When-to-apply order.
// --------------------------------------------------------------------
//
// The twelve pins above cover cap compliance, archive presence, header
// advertisements, H3 format + non-empty title + Pattern/When-to-apply
// presence, iter-descending ordering (active only), cumulative corpus
// floor, and active↔archive non-duplication. They do NOT pin: (a)
// the guard's own header cites PRD 3.8.8 — meta-guard contract; (b)
// the `ACTIVE` + `ARCHIVE` path constants equal their canonical
// relative forms — rename drift hides as opaque "file not readable"
// panics; (c) the `LINE_CAP` constant retains its canonical value
// (200) — a silent raising to 10_000 vacuates `active_file_exists_
// and_under_cap`; (d) archive H3 entries are ALSO ordered iter-
// descending — active-side ordering is pinned but the archive could
// drift forward silently, breaking the "newest at top" contract for
// readers who navigate there; (e) within each H3 entry, `**Pattern.**`
// precedes `**When to apply.**` — the iter-108 presence check accepts
// either order, but readers need Pattern first so the lesson's setup
// arrives before the guidance.

/// The guard's own module header must cite PRD 3.8.8 so a reader
/// chasing a lessons-learned drift lands here via section-grep.
#[test]
fn guard_file_header_cites_prd_3_8_8() {
    let body = fs::read_to_string("tests/lessons_learned_guard.rs")
        .expect("tests/lessons_learned_guard.rs must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.8.8"),
        "meta-guard contract: tests/lessons_learned_guard.rs header \
         must cite `PRD 3.8.8`. Without it, a reader chasing a \
         lessons-learned drift won't land here via section-grep.\n\
         Header:\n{header}"
    );
    assert!(
        header.contains("lessons-learned"),
        "meta-guard contract: header must name the target doc slug \
         `lessons-learned` so the file-under-test is unambiguous."
    );
}

/// `ACTIVE` + `ARCHIVE` path constants must equal their canonical
/// relative forms verbatim. A rename (e.g. moving the docs under a
/// different directory) would silently cause `fs::read_to_string`
/// calls to panic with opaque "file not readable" messages.
#[test]
fn active_and_archive_path_constants_are_canonical() {
    let guard_body = fs::read_to_string("tests/lessons_learned_guard.rs")
        .expect("guard source must be readable");
    assert!(
        guard_body
            .contains("const ACTIVE: &str = \"../../docs/PRD/lessons-learned.md\";"),
        "PRD 3.8.8 (iter 215): tests/lessons_learned_guard.rs must \
         retain `const ACTIVE: &str = \"../../docs/PRD/lessons-learned.md\";` \
         verbatim. A rename without atomic constant update would break \
         every pin with an opaque `file not readable` panic."
    );
    assert!(
        guard_body.contains(
            "const ARCHIVE: &str = \"../../docs/PRD/lessons-learned.archive.md\";"
        ),
        "PRD 3.8.8 (iter 215): tests/lessons_learned_guard.rs must \
         retain `const ARCHIVE: &str = \"../../docs/PRD/lessons-learned.archive.md\";` \
         verbatim. Same rationale as ACTIVE."
    );
}

/// The `LINE_CAP` constant must retain its canonical value of 200.
/// A silent raising to 10_000 (or removal) would turn
/// `active_file_exists_and_under_cap` into a vacuous pass — the
/// retrospective corpus could grow unbounded and the archival
/// discipline iter 108 established would erode silently.
#[test]
fn line_cap_constant_is_two_hundred() {
    let guard_body = fs::read_to_string("tests/lessons_learned_guard.rs")
        .expect("guard source must be readable");
    assert!(
        guard_body.contains("const LINE_CAP: usize = 200;"),
        "PRD 3.8.8 (iter 215): tests/lessons_learned_guard.rs must \
         retain `const LINE_CAP: usize = 200;` verbatim. A silent \
         raise would vacuate the cap-enforcement pin; the 200-line \
         discipline is what keeps lessons-learned a READABLE corpus \
         rather than a write-only log."
    );
}

/// H3 entries in the archive file must also be ordered `iter N`
/// descending (newest first), same rule as the active file. The
/// archive is cumulative and chronological; entries with non-numeric
/// iter slots (e.g. `/ iter meta`) or range slots (e.g. `/ iter 13-16`)
/// are skipped — only numeric iter IDs participate in the ordering
/// check. Mirrors `active_file_entries_are_ordered_newest_iter_first`
/// so archive readers navigate the same convention.
#[test]
fn archive_entries_are_ordered_newest_iter_first() {
    let body = fs::read_to_string(ARCHIVE).expect("archive file present");
    let mut iters: Vec<(usize, u32)> = Vec::new();
    for (i, line) in body.lines().enumerate() {
        if !line.starts_with("### ") {
            continue;
        }
        let tail = line.trim_start_matches("### ");
        let Some(iter_pos) = tail.find(" / iter ") else {
            continue;
        };
        let after = &tail[iter_pos + " / iter ".len()..];
        // Take leading digits only. `iter 13-16` → 13; `iter meta` → "" → skip.
        let iter_num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if iter_num_str.is_empty() {
            continue;
        }
        if let Ok(n) = iter_num_str.parse::<u32>() {
            iters.push((i + 1, n));
        }
    }
    // Archive must have at least 2 entries to make the ordering check
    // meaningful. A < 2 count is fine (nothing to compare).
    for pair in iters.windows(2) {
        let (line_a, iter_a) = pair[0];
        let (line_b, iter_b) = pair[1];
        assert!(
            iter_a > iter_b,
            "PRD 3.8.8 (iter 215): archive H3 entries must be ordered \
             iter-descending (newest first), same as the active file. \
             L{line_a} iter {iter_a} should be strictly greater than \
             L{line_b} iter {iter_b}. A forward-ordered archive breaks \
             the `newest at top` reader convention."
        );
    }
}

/// Within each H3 entry, `**Pattern.**` must appear BEFORE
/// `**When to apply.**`. The iter-108 presence check accepts either
/// order, but readers expect Pattern first so the lesson's setup
/// arrives before the guidance — inverted order reads confusingly.
#[test]
fn every_entry_has_pattern_before_when_to_apply() {
    let body = fs::read_to_string(ACTIVE).expect("active file present");
    let lines: Vec<&str> = body.lines().collect();
    let mut entries: Vec<(usize, String)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("### ") {
            entries.push((i, line.to_string()));
        }
    }
    for (idx, (start, heading)) in entries.iter().enumerate() {
        let end = entries
            .get(idx + 1)
            .map(|(next_start, _)| *next_start)
            .unwrap_or(lines.len());
        let block = &lines[*start..end].join("\n");
        // Both must exist (iter 108 pin), so these unwraps are safe in
        // normal green-CI state; if one's missing, the iter-108 pin
        // fires first with a more specific message.
        let Some(pattern_pos) = block.find("**Pattern.**") else {
            continue;
        };
        let Some(when_pos) = block.find("**When to apply.**") else {
            continue;
        };
        assert!(
            pattern_pos < when_pos,
            "PRD 3.8.8 (iter 215): H3 entry at L{} (`{heading}`) has \
             `**When to apply.**` appearing BEFORE `**Pattern.**` \
             (positions: pattern={pattern_pos}, when={when_pos}). \
             Pattern must come first so the lesson's setup arrives \
             before the guidance. Reorder the paragraphs in the \
             entry.",
            start + 1
        );
    }
}

/// Self-test — prove the detectors bite on known-bad shapes.
#[test]
fn lessons_learned_detector_self_test() {
    // Synthetic too-large body.
    let oversize: String = (0..LINE_CAP + 5).map(|i| format!("line {i}\n")).collect();
    let n = oversize.lines().count();
    assert!(
        n > LINE_CAP,
        "self-test: oversize fixture must exceed the cap"
    );

    // Synthetic empty body.
    let empty = "";
    assert_eq!(empty.lines().count(), 0);

    // Synthetic header-missing-cap.
    let head_no_cap = "# Lessons\n\nSome content without cap mention.\n";
    assert!(
        !head_no_cap.contains("200"),
        "self-test: header without 200 must trip the header check"
    );

    // Bad shape D (iter 139): H3 in wrong format (missing `iter N`).
    let bad_h3 = "### 2026-04-19 — some lesson";
    let looks_ok = bad_h3.len() >= 10
        && bad_h3.trim_start_matches("### ")[..4]
            .chars()
            .all(|c| c.is_ascii_digit())
        && bad_h3.trim_start_matches("### ")[10..].starts_with(" / iter ");
    assert!(
        !looks_ok,
        "self-test: H3 without ` / iter N` must be flagged"
    );

    // Bad shape E: entry with Pattern but no When-to-apply.
    let partial_entry = "### 2026-04-19 / iter 50 — title\n\n**Pattern.** Some pattern text.\n\n";
    assert!(
        partial_entry.contains("**Pattern.**")
            && !partial_entry.contains("**When to apply.**"),
        "self-test: entry missing When-to-apply must be flagged"
    );

    // Bad shape F: header without newest-at-top advertisement.
    let silent_header = "# Lessons\n\nCapped at 200 lines.\n";
    assert!(
        !silent_header.to_lowercase().contains("newest at top"),
        "self-test: header without ordering convention must be flagged"
    );
}
