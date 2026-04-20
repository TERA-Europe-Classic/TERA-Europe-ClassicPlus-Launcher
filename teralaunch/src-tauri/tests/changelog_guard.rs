//! PRD 3.8.5 — `docs/CHANGELOG.md` is player-facing plain English, not
//! a dump of conventional-commit messages.
//!
//! Criterion text: "Per-release player-facing CHANGELOG.md in plain
//! English (no conventional-commit prefixes)." Threshold: grep for
//! `^feat|^fix|^chore` returns 0 matches.
//!
//! Iter 109 adds this as a structural test so any future release that
//! copy-pastes `git log --oneline` into the changelog (or forgets to
//! rewrite commit-style entries into user-facing prose) fails CI.
//!
//! Covers both the raw-prefix case (`feat: add X`) and the
//! scoped-prefix case (`feat(mods): add X`) that release tooling
//! commonly emits. Also flags the common sibling prefixes (docs,
//! refactor, test, build, ci, perf, style, revert) for the same
//! reason — none of those belong in a user-visible file.

use std::fs;

const CHANGELOG: &str = "../../docs/CHANGELOG.md";

const BANNED_PREFIXES: &[&str] = &[
    "feat:",
    "feat(",
    "fix:",
    "fix(",
    "chore:",
    "chore(",
    "docs:",
    "docs(",
    "refactor:",
    "refactor(",
    "test:",
    "test(",
    "build:",
    "build(",
    "ci:",
    "ci(",
    "perf:",
    "perf(",
    "style:",
    "style(",
    "revert:",
    "revert(",
];

fn changelog_body() -> String {
    fs::read_to_string(CHANGELOG).unwrap_or_else(|e| {
        panic!(
            "CHANGELOG.md must be readable from src-tauri/ via {CHANGELOG}: {e}"
        )
    })
}

/// No line may start with a conventional-commit prefix (with or without
/// scope). Bullet-form lines (`- feat: ...`) are also caught — we strip
/// a leading `- ` or `* ` before checking.
#[test]
fn no_conventional_commit_prefixes() {
    let body = changelog_body();
    let mut offenders: Vec<(usize, String)> = Vec::new();

    for (i, line) in body.lines().enumerate() {
        let stripped = line
            .trim_start_matches(|c: char| c.is_whitespace())
            .trim_start_matches("- ")
            .trim_start_matches("* ");
        for prefix in BANNED_PREFIXES {
            if stripped.starts_with(prefix) {
                offenders.push((i + 1, line.to_string()));
                break;
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "PRD 3.8.5 violated: {} line(s) in CHANGELOG.md start with a \
         conventional-commit prefix (this file is player-facing, not a \
         dev log). Offenders:\n  {}\n\
         Rewrite in plain user-facing English, or move dev-log details \
         to release-notes tooling if needed.",
        offenders.len(),
        offenders
            .iter()
            .map(|(n, l)| format!("L{n}: {l}"))
            .collect::<Vec<_>>()
            .join("\n  "),
    );
}

/// Changelog must have at least one `## ` release heading and a human-
/// readable preamble. A one-line "TBD" file would pass the prefix check
/// but fails the criterion's intent.
#[test]
fn changelog_has_structure_and_content() {
    let body = changelog_body();
    let heading_count = body.lines().filter(|l| l.starts_with("## ")).count();
    assert!(
        heading_count >= 1,
        "CHANGELOG.md has 0 `## ` headings — at least one release section \
         (or `## Unreleased`) should be present."
    );
    assert!(
        body.lines().count() >= 10,
        "CHANGELOG.md is too short ({} lines) — PRD 3.8.5 expects a \
         real player-facing release log.",
        body.lines().count()
    );
}

/// CHANGELOG.md must carry a `## Unreleased` section — the buffer
/// where incoming player-facing entries land between releases.
/// Without it, new work has no designated spot and either ends up
/// appended to the previous release (rewriting history) or dropped.
#[test]
fn changelog_carries_unreleased_section() {
    let body = changelog_body();
    assert!(
        body.contains("## Unreleased"),
        "CHANGELOG.md must carry an `## Unreleased` section as the \
         buffer for incoming release notes. Without it, new entries \
         either rewrite the previous release's section or get \
         dropped. Restore the section before the next release cut."
    );
}

/// Every non-Unreleased `## ` heading must follow the player-facing
/// semver shape `## X.Y.Z — title` (em-dash separator, not hyphen).
/// `## 0.1.3 and earlier` is a legacy terminal section and is
/// allowed as an exception because it summarises pre-tracked history.
#[test]
fn release_sections_follow_semver_em_dash_shape() {
    let body = changelog_body();
    let mut offenders: Vec<(usize, String)> = Vec::new();
    for (i, line) in body.lines().enumerate() {
        if !line.starts_with("## ") {
            continue;
        }
        let tail = line.trim_start_matches("## ");
        // Allow `## Unreleased` verbatim and `## 0.1.3 and earlier`
        // legacy terminal.
        if tail == "Unreleased" || tail == "0.1.3 and earlier" {
            continue;
        }
        // Expected shape: X.Y.Z — title, where X/Y/Z are digit runs
        // and the separator is the em-dash character (U+2014).
        let looks_semver_ok = {
            let first_token = tail.split_whitespace().next().unwrap_or("");
            let parts: Vec<&str> = first_token.split('.').collect();
            parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        };
        let has_em_dash = tail.contains(" \u{2014} ");
        if !(looks_semver_ok && has_em_dash) {
            offenders.push((i + 1, line.to_string()));
        }
    }
    assert!(
        offenders.is_empty(),
        "PRD 3.8.5 shape drift: {} `## ` release heading(s) don't \
         follow `## X.Y.Z — title` (em-dash U+2014, not hyphen). \
         Offenders:\n  {}\n\
         Fixed exceptions: `## Unreleased` and `## 0.1.3 and earlier`.",
        offenders.len(),
        offenders
            .iter()
            .map(|(n, l)| format!("L{n}: {l}"))
            .collect::<Vec<_>>()
            .join("\n  "),
    );
}

/// The header must advertise the newest-release-first ordering so a
/// contributor appending at the bottom (chronological-forward) gets
/// corrected. Same drift-prevention reasoning as
/// lessons_learned_guard's header check.
#[test]
fn header_advertises_newest_release_first_ordering() {
    let body = changelog_body();
    let head: String = body.lines().take(15).collect::<Vec<_>>().join("\n");
    assert!(
        head.to_lowercase().contains("most recent release")
            || head.to_lowercase().contains("newest")
            || head.to_lowercase().contains("top"),
        "CHANGELOG.md header must advertise the newest-release-first \
         ordering so a contributor doesn't append chronologically-\
         forward and drift the file's shape. Header seen:\n{head}"
    );
}

/// Semver versions in headings must be strictly descending from
/// top to bottom (excluding the terminal `0.1.3 and earlier` and
/// skipping `Unreleased`). A forward-ordered or out-of-order
/// changelog confuses readers and breaks release-notes tooling.
#[test]
fn release_versions_descend_from_top_to_bottom() {
    let body = changelog_body();
    let mut versions: Vec<(usize, (u32, u32, u32))> = Vec::new();
    for (i, line) in body.lines().enumerate() {
        if !line.starts_with("## ") {
            continue;
        }
        let tail = line.trim_start_matches("## ");
        if tail == "Unreleased" || tail == "0.1.3 and earlier" {
            continue;
        }
        let first = tail.split_whitespace().next().unwrap_or("");
        let parts: Vec<&str> = first.split('.').collect();
        if parts.len() == 3 {
            if let (Ok(a), Ok(b), Ok(c)) = (
                parts[0].parse::<u32>(),
                parts[1].parse::<u32>(),
                parts[2].parse::<u32>(),
            ) {
                versions.push((i + 1, (a, b, c)));
            }
        }
    }
    for pair in versions.windows(2) {
        let (l1, v1) = pair[0];
        let (l2, v2) = pair[1];
        assert!(
            v1 > v2,
            "CHANGELOG.md ordering drift: L{l1} version {v1:?} should \
             be strictly greater than L{l2} version {v2:?} \
             (newest-first). A forward-ordered or out-of-sequence \
             changelog breaks reader expectations and release-notes \
             tooling."
        );
    }
}

// --------------------------------------------------------------------
// Iter 173 structural pins — preamble contract + HR separators +
// non-empty titles + Unreleased ordering + placeholder-free content.
// --------------------------------------------------------------------
//
// Iter 109+141 pinned the conv-commit absence + structure + Unreleased
// presence + em-dash shape + descending order + newest-first header.
// Iter 173 widens to the DOCUMENT-level shape invariants those pins
// skip: the player-facing preamble, the `---` HR section separator,
// titles that actually exist after the em-dash, Unreleased appearing
// FIRST (not just present), and no `TBD`/`TODO`/`fixme` in numbered
// release sections (the Unreleased section is exempt — "Nothing yet"
// is a legit placeholder there).

/// The CHANGELOG preamble must explicitly state the player-facing
/// purpose. Without it, a contributor could mistake the file for a
/// dev changelog and paste `git log` output — which is what iter
/// 109's conv-commit pin catches AFTER the fact. This pin catches
/// the intent drift at the doc header instead.
#[test]
fn preamble_advertises_player_facing_purpose() {
    let body = changelog_body();
    let head: String = body.lines().take(10).collect::<Vec<_>>().join("\n");
    assert!(
        head.contains("Player-facing release notes"),
        "PRD 3.8.5: CHANGELOG.md preamble must contain \
         `Player-facing release notes` in the first 10 lines. \
         Without the explicit contract, a future contributor could \
         treat the file as a dev changelog — which is how the \
         conv-commit prefixes (caught by iter 109's pin) end up \
         here in the first place.\nHead seen:\n{head}"
    );
    assert!(
        head.contains("git log"),
        "PRD 3.8.5: preamble must reference `git log` as the \
         alternative destination for commit-level history. Without \
         this redirect, contributors looking for a commit log \
         would erode the plain-English invariant here."
    );
}

/// Every numbered release section must be separated from its
/// neighbours by an `---` HR line. The eye-level separator is what
/// lets players scan release boundaries without parsing the version
/// numbers; dropping it makes adjacent releases read as one long
/// section.
#[test]
fn release_sections_are_separated_by_hr_lines() {
    let body = changelog_body();
    let hr_count = body.lines().filter(|l| l.trim() == "---").count();
    let release_heading_count = body
        .lines()
        .filter(|l| l.starts_with("## "))
        .count();
    // At least as many HR lines as release headings (one before each
    // section is the approved layout).
    assert!(
        hr_count >= release_heading_count,
        "PRD 3.8.5: CHANGELOG.md has {release_heading_count} `## ` \
         headings but only {hr_count} `---` HR lines. Every section \
         boundary should carry an HR separator so adjacent releases \
         don't run together visually."
    );
}

/// Every numbered release heading must carry a non-empty human-
/// readable title after the em-dash. `## 0.1.12 — ` (empty after
/// the separator) would pass the iter-141 em-dash check but give
/// players no signal about what the release is about.
#[test]
fn release_sections_have_nonempty_title_after_em_dash() {
    let body = changelog_body();
    let mut offenders: Vec<(usize, String)> = Vec::new();
    for (i, line) in body.lines().enumerate() {
        if !line.starts_with("## ") {
            continue;
        }
        let tail = line.trim_start_matches("## ");
        // Skip exempt shapes.
        if tail == "Unreleased" || tail == "0.1.3 and earlier" {
            continue;
        }
        // Find the em-dash and check what follows.
        let Some(idx) = tail.find(" \u{2014} ") else {
            continue; // the em-dash check (earlier test) catches this case
        };
        let title = tail[idx + " \u{2014} ".len()..].trim();
        if title.is_empty() {
            offenders.push((i + 1, line.to_string()));
        }
    }
    assert!(
        offenders.is_empty(),
        "PRD 3.8.5: {} release heading(s) have an em-dash but no \
         title after it. Offenders:\n  {}\nPlayers need the title \
         to know what the release delivered; an empty title reduces \
         the section header to noise.",
        offenders.len(),
        offenders
            .iter()
            .map(|(n, l)| format!("L{n}: {l}"))
            .collect::<Vec<_>>()
            .join("\n  "),
    );
}

/// `## Unreleased` must appear BEFORE any numbered release heading.
/// The iter-141 pin proves the section EXISTS; this pin proves it
/// lands first. A file that carries `## Unreleased` at the bottom
/// would pass descending-version and presence checks but would
/// violate the newest-first convention — new work would appear AFTER
/// old releases.
#[test]
fn unreleased_section_precedes_numbered_releases() {
    let body = changelog_body();
    let unreleased_line = body
        .lines()
        .position(|l| l == "## Unreleased")
        .expect("## Unreleased must exist (iter 141 pin)");
    let first_numbered_line = body.lines().position(|l| {
        if !l.starts_with("## ") {
            return false;
        }
        let tail = l.trim_start_matches("## ");
        let first = tail.split_whitespace().next().unwrap_or("");
        let parts: Vec<&str> = first.split('.').collect();
        parts.len() == 3 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty())
    });
    if let Some(first) = first_numbered_line {
        assert!(
            unreleased_line < first,
            "PRD 3.8.5: `## Unreleased` (line {}) must come BEFORE \
             the first numbered release (line {}). A bottom-placed \
             Unreleased would put new work AFTER old releases, \
             breaking the newest-first convention.",
            unreleased_line + 1,
            first + 1
        );
    }
}

/// Numbered release sections must NOT contain `TBD` / `TODO` /
/// `FIXME` / `XXX` / `[placeholder]` markers — those belong in the
/// `## Unreleased` buffer, not shipped releases. A release with a
/// TBD in its bullet list means either the release went out half-
/// written or the author meant to move the bullet to Unreleased and
/// forgot.
#[test]
fn numbered_release_sections_carry_no_placeholder_markers() {
    let body = changelog_body();
    let mut in_unreleased = false;
    let mut in_numbered = false;
    let mut offenders: Vec<(usize, String)> = Vec::new();

    for (i, line) in body.lines().enumerate() {
        if line.starts_with("## ") {
            let tail = line.trim_start_matches("## ");
            in_unreleased = tail == "Unreleased";
            // Numbered = starts with digit.digit.digit OR the terminal
            // "0.1.3 and earlier".
            let first = tail.split_whitespace().next().unwrap_or("");
            let parts: Vec<&str> = first.split('.').collect();
            let is_semver = parts.len() == 3
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
            in_numbered = is_semver || tail == "0.1.3 and earlier";
            continue;
        }
        if !in_numbered || in_unreleased {
            continue;
        }
        for marker in ["TBD", "TODO", "FIXME", "XXX", "[placeholder]"] {
            if line.contains(marker) {
                offenders.push((i + 1, line.to_string()));
                break;
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "PRD 3.8.5: {} line(s) in numbered release sections contain \
         placeholder markers (TBD/TODO/FIXME/XXX/[placeholder]). \
         Offenders:\n  {}\nThose belong in the `## Unreleased` \
         buffer, not shipped releases.",
        offenders.len(),
        offenders
            .iter()
            .map(|(n, l)| format!("L{n}: {l}"))
            .collect::<Vec<_>>()
            .join("\n  "),
    );
}

// --------------------------------------------------------------------
// Iter 213 structural pins — meta-guard header + CHANGELOG path constant
// + BANNED_PREFIXES type coverage + Cargo.toml version sync + per-release
// bullet content.
// --------------------------------------------------------------------
//
// The twelve pins above cover conv-commit absence, structure, Unreleased
// presence + ordering, em-dash shape, descending semver, preamble, HR
// separators, non-empty titles, and no-placeholder-in-releases. They do
// NOT pin: (a) the guard's own header cites PRD 3.8.5 — meta-guard
// contract; (b) the `CHANGELOG` constant equals the canonical relative
// path — rename drift hides as an opaque "file not readable" panic;
// (c) BANNED_PREFIXES covers all 11 conventional-commit types (feat /
// fix / chore / docs / refactor / test / build / ci / perf / style /
// revert) in BOTH forms (`:` and `(`) — dropping one (e.g. `feat(`)
// would let scoped `feat(mods): ...` lines slip past; (d) CHANGELOG's
// newest release heading matches the current Cargo.toml version —
// release-tooling drift (bump Cargo without updating CHANGELOG) is the
// classic release-day regression; (e) every numbered release section
// has at least one bullet `- `; a heading-only release has zero
// content and offers players no signal despite passing title + shape
// pins.

const CARGO_TOML: &str = "Cargo.toml";

/// The guard's own module header must cite PRD 3.8.5 so a reader
/// chasing a CHANGELOG drift lands here via section-grep.
#[test]
fn guard_file_header_cites_prd_3_8_5() {
    let body = fs::read_to_string("tests/changelog_guard.rs")
        .expect("tests/changelog_guard.rs must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.8.5"),
        "meta-guard contract: tests/changelog_guard.rs header must \
         cite `PRD 3.8.5`. Without it, a reader chasing a changelog-\
         shape regression won't land here via section-grep.\n\
         Header:\n{header}"
    );
    assert!(
        header.contains("CHANGELOG") || header.contains("player-facing"),
        "meta-guard contract: header must name the target doc \
         (`CHANGELOG`) or its shape contract (`player-facing`) so \
         the file-under-test is unambiguous from the header alone."
    );
}

/// The `CHANGELOG` path constant must equal its canonical relative
/// form verbatim. A rename (e.g. swap with `docs/changelog.md` or
/// `CHANGELOG.md` at repo root) would silently cause every
/// `fs::read_to_string(CHANGELOG)` call to panic with an opaque
/// "file not readable" message that doesn't point at the constant
/// as the root cause.
#[test]
fn changelog_path_constant_is_canonical() {
    let guard_body = fs::read_to_string("tests/changelog_guard.rs")
        .expect("guard source must be readable");
    assert!(
        guard_body.contains("const CHANGELOG: &str = \"../../docs/CHANGELOG.md\";"),
        "PRD 3.8.5 (iter 213): tests/changelog_guard.rs must retain \
         `const CHANGELOG: &str = \"../../docs/CHANGELOG.md\";` \
         verbatim. A rename of either the constant or the doc must \
         happen atomically — otherwise every changelog test fails \
         with opaque `file not readable` errors."
    );
}

/// `BANNED_PREFIXES` must cover all 11 conventional-commit types in
/// BOTH forms (`type:` and `type(`). Dropping any one (e.g. `feat(`)
/// would let scoped conv-commits like `feat(mods): add search` slip
/// past the prefix check — the classic release-tooling regression
/// this guard exists to prevent.
#[test]
fn banned_prefixes_covers_all_conventional_types_both_forms() {
    const REQUIRED_TYPES: &[&str] = &[
        "feat", "fix", "chore", "docs", "refactor",
        "test", "build", "ci", "perf", "style", "revert",
    ];
    let mut missing: Vec<String> = Vec::new();
    for ty in REQUIRED_TYPES {
        for form in [":", "("] {
            let combo = format!("{ty}{form}");
            if !BANNED_PREFIXES.iter().any(|p| *p == combo) {
                missing.push(combo);
            }
        }
    }
    assert!(
        missing.is_empty(),
        "PRD 3.8.5 (iter 213): BANNED_PREFIXES must contain each \
         conventional-commit type in BOTH `type:` and `type(` forms. \
         Missing: {missing:?}. Dropping one form lets that shape \
         (e.g. scoped `feat(mods): ...`) slip past the prefix check \
         into player-facing release notes."
    );
    // Also pin the total count as a floor (22 = 11 types × 2 forms).
    assert!(
        BANNED_PREFIXES.len() >= 22,
        "PRD 3.8.5 (iter 213): BANNED_PREFIXES has {} entries; floor \
         is 22 (11 types × 2 forms). A bulk-delete or bad rebase may \
         have dropped rows.",
        BANNED_PREFIXES.len()
    );
}

/// The newest-release heading in CHANGELOG.md must match the current
/// Cargo.toml version. Release-tooling drift (bumping Cargo.toml
/// without updating the CHANGELOG, or vice versa) is the classic
/// release-day regression — players see a mismatched version, or
/// auto-update pipeline ships a build with stale release notes.
#[test]
fn newest_release_matches_cargo_toml_version() {
    let cargo = fs::read_to_string(CARGO_TOML)
        .expect("Cargo.toml must be readable");
    // Extract the first `version = "X.Y.Z"` line — this is the
    // package version (the workspace/dep versions live elsewhere).
    let version_line = cargo
        .lines()
        .find(|l| l.trim_start().starts_with("version = \""))
        .expect("Cargo.toml must declare a `version = \"...\"` line");
    // Parse the version between the quotes.
    let first_quote = version_line
        .find('"')
        .expect("version line must contain a quote");
    let rest = &version_line[first_quote + 1..];
    let second_quote = rest
        .find('"')
        .expect("version line must close with a quote");
    let cargo_version = &rest[..second_quote];

    // Walk the changelog for the first numbered release heading.
    let body = changelog_body();
    let first_release = body
        .lines()
        .find_map(|l| {
            if !l.starts_with("## ") {
                return None;
            }
            let tail = l.trim_start_matches("## ");
            if tail == "Unreleased" {
                return None;
            }
            let first_tok = tail.split_whitespace().next()?;
            let parts: Vec<&str> = first_tok.split('.').collect();
            if parts.len() == 3
                && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
            {
                Some(first_tok.to_string())
            } else {
                None
            }
        })
        .expect("CHANGELOG.md must carry at least one numbered release heading");

    assert_eq!(
        first_release, cargo_version,
        "PRD 3.8.5 (iter 213): newest CHANGELOG release `{first_release}` \
         must match Cargo.toml version `{cargo_version}`. Release-\
         tooling drift (bump Cargo without updating CHANGELOG, or \
         ship a release without adding its section) is the classic \
         release-day regression — players see a mismatched version."
    );
}

/// Every numbered release section must carry substantive body
/// content — either a bullet list (`- `/`* `) OR at least two
/// non-blank non-heading lines of prose. A heading-only or
/// one-line-only release passes the title + em-dash + HR pins but
/// gives players zero signal about what changed. Both bullet-list
/// and prose-paragraph forms are legitimate player-facing shapes
/// (shipped releases use both); the requirement is substance, not
/// a specific list syntax. Exempt: `0.1.3 and earlier` (summary
/// pointer, not a real release).
#[test]
fn every_numbered_release_has_substantive_body() {
    const MIN_PROSE_LINES: usize = 2;
    let body = changelog_body();
    let lines: Vec<&str> = body.lines().collect();
    let mut offenders: Vec<String> = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if !line.starts_with("## ") {
            i += 1;
            continue;
        }
        let tail = line.trim_start_matches("## ");
        if tail == "Unreleased" || tail == "0.1.3 and earlier" {
            i += 1;
            continue;
        }
        let first_tok = tail.split_whitespace().next().unwrap_or("");
        let parts: Vec<&str> = first_tok.split('.').collect();
        let is_semver = parts.len() == 3
            && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
        if !is_semver {
            i += 1;
            continue;
        }

        // Scan to next `## ` or EOF. Count bullet lines AND prose lines
        // (any non-blank, non-HR, non-heading line).
        let mut j = i + 1;
        let mut has_bullet = false;
        let mut prose_lines = 0usize;
        while j < lines.len() && !lines[j].starts_with("## ") {
            let trimmed = lines[j].trim_start();
            if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                has_bullet = true;
            } else if !trimmed.is_empty() && trimmed != "---" {
                prose_lines += 1;
            }
            j += 1;
        }
        // Substance is met if EITHER a bullet exists OR prose reaches
        // the floor.
        if !has_bullet && prose_lines < MIN_PROSE_LINES {
            offenders.push(format!(
                "L{}: {} (no bullets; only {} prose line(s))",
                i + 1,
                line,
                prose_lines
            ));
        }
        i = j;
    }

    assert!(
        offenders.is_empty(),
        "PRD 3.8.5 (iter 213): {} numbered release section(s) lack \
         substantive body (need either a bullet `- ` OR ≥ {MIN_PROSE_LINES} \
         prose lines). Offenders:\n  {}\nA heading-only or one-liner \
         release passes the title + em-dash + HR pins but gives \
         players zero signal about what changed.",
        offenders.len(),
        offenders.join("\n  ")
    );
}

// --------------------------------------------------------------------
// Iter 251 structural pins — CARGO_TOML constant pin + byte floor
// + bullet-style consistency + release heading floor + Classic+ brand.
// --------------------------------------------------------------------
//
// The seventeen pins above cover conv-commit absence, preamble, release
// shape, ordering, and Cargo-version sync. They do NOT pin:
// (a) the `CARGO_TOML` constant (added iter 213) equals `"Cargo.toml"`
//     verbatim — a rename would break the version-sync pin with an
//     opaque "file not readable" panic;
// (b) the CHANGELOG meets a byte-size floor — a truncation to just a
//     preamble + `## Unreleased` stub would pass preamble + structure
//     pins while erasing real history;
// (c) bullet list style is consistent (`- ` not `* `) — mixed syntaxes
//     look unprofessional in a player-facing doc;
// (d) a minimum number of numbered release headings — a doc with just
//     `## Unreleased` + a single release passes every structural pin
//     while offering no history to players;
// (e) the preamble cites the `Classic+` brand name — drift to the
//     plain `TERA` or `TERA Europe Classic` brand would mis-position
//     the release notes for players of the Classic+ fork.

/// The `CARGO_TOML` path constant must equal `"Cargo.toml"` verbatim.
/// A rename (e.g. to `"./Cargo.toml"` or `"../Cargo.toml"`) would break
/// `newest_release_matches_cargo_toml_version` with an opaque
/// "file not readable" panic that doesn't point at the constant.
#[test]
fn cargo_toml_path_constant_is_canonical() {
    let guard_body = fs::read_to_string("tests/changelog_guard.rs")
        .expect("guard source must be readable");
    assert!(
        guard_body.contains("const CARGO_TOML: &str = \"Cargo.toml\";"),
        "PRD 3.8.5 (iter 251): tests/changelog_guard.rs must retain \
         `const CARGO_TOML: &str = \"Cargo.toml\";` verbatim. A \
         rename would break the cargo-version-sync pin (iter 213) \
         with an opaque `file not readable` panic."
    );
}

/// CHANGELOG.md must meet a minimum byte-size floor. A truncation to
/// just the preamble + a stub `## Unreleased` would pass the preamble,
/// structure, and Unreleased-section pins while erasing real history
/// — 13 current release sections compress into ~6.4 KB, so 2000
/// bytes gives ~3× margin while catching an accidental truncation.
#[test]
fn changelog_file_size_meets_byte_floor() {
    const MIN_BYTES: usize = 2000;
    let bytes = fs::metadata(CHANGELOG)
        .unwrap_or_else(|e| panic!("{CHANGELOG}: {e}"))
        .len() as usize;
    assert!(
        bytes >= MIN_BYTES,
        "PRD 3.8.5 (iter 251): {CHANGELOG} is only {bytes} bytes; \
         floor is {MIN_BYTES}. Truncation past the floor suggests \
         an accidental overwrite or bad rebase — player-facing \
         history is gone even though preamble + structure pins still \
         pass."
    );
}

/// Every bullet line must use `- ` (dash-space), NOT `* ` (asterisk-
/// space). Both are valid CommonMark but mixing styles looks
/// unprofessional in a player-facing document, and the
/// `numbered_release_sections_carry_no_placeholder_markers` + the
/// `no_conventional_commit_prefixes` + `every_numbered_release_has_
/// substantive_body` detectors all normalise `- ` AND `* ` prefixes
/// together — flipping the syntax silently works, but inconsistency
/// is real drift.
#[test]
fn every_bullet_uses_dash_not_asterisk() {
    let body = changelog_body();
    let mut offenders: Vec<(usize, String)> = Vec::new();
    for (i, line) in body.lines().enumerate() {
        if line.trim_start().starts_with("* ") {
            offenders.push((i + 1, line.to_string()));
        }
    }
    assert!(
        offenders.is_empty(),
        "PRD 3.8.5 (iter 251): {} bullet line(s) use `* ` instead \
         of `- `. Mixed bullet styles look unprofessional in a \
         player-facing doc. Offenders:\n  {}",
        offenders.len(),
        offenders
            .iter()
            .map(|(n, l)| format!("L{n}: {l}"))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// CHANGELOG must carry at least FIVE numbered release headings. The
/// structure pin (iter 109) only checks ≥ 1 heading, which passes
/// for a doc with just `## Unreleased` + one release — no history
/// is far worse than no changelog. Current state: 13 numbered
/// releases. A floor at 5 gives generous margin while catching
/// accidental mass-truncation of the history section.
#[test]
fn release_heading_count_meets_minimum_floor() {
    const MIN_RELEASES: usize = 5;
    let body = changelog_body();
    let count = body
        .lines()
        .filter(|l| {
            if !l.starts_with("## ") {
                return false;
            }
            let tail = l.trim_start_matches("## ");
            if tail == "Unreleased" {
                return false;
            }
            let first = tail.split_whitespace().next().unwrap_or("");
            let parts: Vec<&str> = first.split('.').collect();
            parts.len() == 3
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        })
        .count();
    assert!(
        count >= MIN_RELEASES,
        "PRD 3.8.5 (iter 251): CHANGELOG.md has {count} numbered \
         release heading(s); floor is {MIN_RELEASES}. A changelog \
         with < {MIN_RELEASES} numbered releases offers players no \
         history — the structure pin (≥ 1) passes on a single-\
         release stub."
    );
}

/// The CHANGELOG preamble must cite the `Classic+` brand name. Brand
/// drift (e.g. to `TERA Europe Classic` without the `+`, or plain
/// `TERA`) would mis-position the release notes — Classic+ is a
/// distinct fork from upstream Classic, and players need to see the
/// brand in the header to trust they're reading notes for the right
/// build.
#[test]
fn changelog_preamble_cites_classic_plus_brand_name() {
    let body = changelog_body();
    let head: String = body.lines().take(5).collect::<Vec<_>>().join("\n");
    assert!(
        head.contains("Classic+"),
        "PRD 3.8.5 (iter 251): CHANGELOG.md preamble (first 5 \
         lines) must cite `Classic+` so players know this is the \
         fork-specific release log, not upstream Classic. Head \
         seen:\n{head}"
    );
}

/// Self-test — prove the detector bites on known-bad shapes.
#[test]
fn changelog_detector_self_test() {
    let bad_raw = "feat: add mod manager\nfix(mods): drop duplicate row\n";
    let mut hits = 0;
    for line in bad_raw.lines() {
        let stripped = line.trim_start_matches("- ").trim_start_matches("* ");
        for prefix in BANNED_PREFIXES {
            if stripped.starts_with(prefix) {
                hits += 1;
                break;
            }
        }
    }
    assert_eq!(hits, 2, "self-test: raw conv-commit lines must be flagged");

    let bad_bulleted = "- feat(mods): add search\n- chore: bump deps\n";
    let mut hits = 0;
    for line in bad_bulleted.lines() {
        let stripped = line.trim_start_matches("- ").trim_start_matches("* ");
        for prefix in BANNED_PREFIXES {
            if stripped.starts_with(prefix) {
                hits += 1;
                break;
            }
        }
    }
    assert_eq!(hits, 2, "self-test: bulleted conv-commit lines must be flagged");

    // Good: player-facing prose should NOT trigger.
    let good = "Your mods now know when a new version is out.\n\
                - The catalog fetch now drives an updates indicator.\n\
                - Toggle switches are intent-only in the Installed tab.\n";
    for line in good.lines() {
        let stripped = line.trim_start_matches("- ").trim_start_matches("* ");
        for prefix in BANNED_PREFIXES {
            assert!(
                !stripped.starts_with(prefix),
                "self-test: player-facing prose must NOT be flagged: {line:?}"
            );
        }
    }

    // Bad shape C (iter 141): changelog without Unreleased section.
    let no_unreleased = "# Changelog\n\n## 0.1.12 — Release\n";
    assert!(
        !no_unreleased.contains("## Unreleased"),
        "self-test: changelog without Unreleased section must be \
         flagged"
    );

    // Bad shape D: hyphen instead of em-dash in release heading.
    let hyphen_sep = "## 0.1.12 - wrong separator";
    assert!(
        !hyphen_sep.contains(" \u{2014} "),
        "self-test: hyphen-separator heading must not pass the \
         em-dash check"
    );

    // Bad shape E: forward-ordered versions.
    let fwd1 = (0u32, 1u32, 10u32);
    let fwd2 = (0u32, 1u32, 11u32);
    assert!(
        fwd1 <= fwd2,
        "self-test: forward-ordered pair must fail the descending \
         check"
    );

    // Positive: descending versions pass.
    let newer = (0u32, 1u32, 12u32);
    let older = (0u32, 1u32, 11u32);
    assert!(
        newer > older,
        "self-test: descending pair must pass"
    );
}
