//! PRD 3.8.1 — `CLAUDE.md` has a `## Mod Manager` section covering
//! feature state + build + deploy.
//!
//! Criterion threshold: section exists with >= 30 lines of content.
//! Until iter 107 this was enforced only by periodic human review.
//! CLAUDE.md is the on-ramp doc every new session reads, so drift here
//! is expensive — a stale or deleted Mod Manager section means the
//! next loop iter starts without key context.
//!
//! This guard asserts the section exists, counts its body lines, and
//! requires a minimum of 30. It also pins presence of the subsections
//! PRD §3.8.1 names explicitly (feature state, build, deploy).

use std::fs;

const CLAUDE_MD: &str = "../../CLAUDE.md";
const SECTION_HEADING: &str = "## Mod Manager";
const MIN_SECTION_LINES: usize = 30;

fn claude_body() -> String {
    fs::read_to_string(CLAUDE_MD).unwrap_or_else(|e| {
        panic!("CLAUDE.md must be readable from src-tauri/ via {CLAUDE_MD}: {e}")
    })
}

/// Extract the lines of the `## Mod Manager` section, from its heading
/// through the line before the next `## ` top-level heading (or EOF).
fn mod_manager_section(body: &str) -> Option<Vec<&str>> {
    let mut in_section = false;
    let mut out: Vec<&str> = Vec::new();
    for line in body.lines() {
        if in_section {
            if line.starts_with("## ") && !line.starts_with(SECTION_HEADING) {
                // Next top-level section started — stop.
                break;
            }
            out.push(line);
        } else if line.starts_with(SECTION_HEADING) {
            in_section = true;
            out.push(line);
        }
    }
    if in_section {
        Some(out)
    } else {
        None
    }
}

/// PRD 3.8.1 — section exists + line count >= threshold.
#[test]
fn mod_manager_section_exists_and_meets_size_threshold() {
    let body = claude_body();
    let section = mod_manager_section(&body).unwrap_or_else(|| {
        panic!(
            "PRD 3.8.1 violated: no `{SECTION_HEADING}` heading found in \
             CLAUDE.md. The on-ramp section every new loop iter reads \
             is missing."
        )
    });
    let n = section.len();
    assert!(
        n >= MIN_SECTION_LINES,
        "PRD 3.8.1 violated: `{SECTION_HEADING}` section has only {n} \
         lines (threshold: >= {MIN_SECTION_LINES}). The section exists \
         but is too sparse to carry the feature-state + build + deploy \
         context it's meant to provide."
    );
}

/// PRD 3.8.1 explicitly names three areas the section should cover:
/// feature state, build, deploy. We check for keywords that would be
/// present in any reasonable description of each.
#[test]
fn mod_manager_section_covers_state_build_and_deploy() {
    let body = claude_body();
    let section = mod_manager_section(&body)
        .expect("prior test would have failed")
        .join("\n")
        .to_lowercase();

    for (label, needles) in &[
        (
            "feature state",
            &["feature state", "shipped", "state"] as &[&str],
        ),
        ("build", &["build", "cargo", "tauri dev"] as &[&str]),
        ("deploy", &["deploy", "nsis", "release"] as &[&str]),
    ] {
        let hit = needles.iter().any(|n| section.contains(n));
        assert!(
            hit,
            "PRD 3.8.1 body-coverage violated: `{SECTION_HEADING}` \
             section does not appear to cover `{label}` (searched for \
             any of: {:?}).",
            needles,
        );
    }
}

/// CLAUDE.md is loaded into every Claude Code session's context, so
/// silent removal of a top-level section costs every future loop
/// iter — the agent has to re-derive that context from source.
/// Iter 137 pins the seven sections currently shipped.
///
/// Adding a section: append its heading to `EXPECTED_SECTIONS` and
/// add a matching line to CLAUDE.md. Removing a section requires a
/// deliberate guard change — the author must justify the removal by
/// deleting the entry here with a commit-message explanation.
const EXPECTED_SECTIONS: &[&str] = &[
    "## Build & Development Commands",
    "## v100 API (Classic+ Server)",
    "## Architecture",
    "## Known Gaps",
    "## Cargo Feature Flags",
    "## Testing",
    "## Mod Manager",
];

#[test]
fn every_expected_section_heading_exists_in_claude_md() {
    let body = claude_body();
    for heading in EXPECTED_SECTIONS {
        assert!(
            body.contains(heading),
            "CLAUDE.md is missing the expected top-level section \
             `{heading}`. Either the heading was renamed (update this \
             guard to match — and confirm nothing else referenced \
             the old name) or the section was deleted (which drops \
             on-ramp context for every future loop iter — justify \
             in the commit message if intentional)."
        );
    }
}

/// The v100 API section specifically pins the endpoints the launcher
/// talks to. Silent removal of any of the 4 endpoint subsections
/// breaks the "new contributor reads CLAUDE.md and ships a change"
/// invariant for anything touching the portal API.
#[test]
fn v100_api_section_documents_four_endpoints() {
    let body = claude_body();
    for subsection in &[
        "### Authentication",
        "### Registration",
        "### Account Info",
        "### Other Endpoints",
    ] {
        assert!(
            body.contains(subsection),
            "CLAUDE.md `## v100 API` must keep the `{subsection}` \
             subsection. Classic+ Portal API documentation is the only \
             in-repo source; dropping a subsection means a contributor \
             touching that endpoint has to reverse-engineer its shape \
             from launcher source."
        );
    }
    // Portal base URL pin — if someone swaps LAN endpoint for an
    // incorrect value, the misdocumentation would mislead every
    // future contributor.
    assert!(
        body.contains("157.90.107.2:8090"),
        "CLAUDE.md `## v100 API` must cite the LAN dev base URL \
         `http://157.90.107.2:8090`. The 3.1.13.portal-https gate \
         tracks the production FQDN migration; until then, this URL \
         is the documented source-of-truth for local testing."
    );
}

/// The Cargo Feature Flags section names the `skip-updates` flag —
/// dropping this doc erases the only in-repo explanation of why the
/// flag exists and when to use it.
#[test]
fn cargo_feature_flags_section_documents_skip_updates() {
    let body = claude_body();
    assert!(
        body.contains("## Cargo Feature Flags"),
        "prior test guarantees the section exists"
    );
    assert!(
        body.contains("skip-updates"),
        "CLAUDE.md `## Cargo Feature Flags` section must document \
         the `skip-updates` feature flag — it's the only in-repo \
         explanation of why the flag exists and when a dev should \
         use it."
    );
    assert!(
        body.contains("custom-protocol"),
        "CLAUDE.md must also document the `custom-protocol` feature \
         flag (required for production Tauri builds)."
    );
}

/// The Testing section must name the test counts + paths so a new
/// contributor knows WHERE tests live and roughly HOW MANY there
/// are. The counts drift with every iteration — we check only that
/// SOMETHING resembling a count is cited, not the exact number.
#[test]
fn testing_section_cites_test_paths() {
    let body = claude_body();
    assert!(
        body.contains("## Testing"),
        "prior test guarantees the section exists"
    );
    // Section must mention frontend + backend test roots.
    let idx = body.find("## Testing").expect("section must exist");
    let rest = &body[idx..];
    // Scan up to the next top-level heading.
    let section_end = rest[2..].find("\n## ").map(|i| i + 2).unwrap_or(rest.len());
    let section = &rest[..section_end];
    assert!(
        section.contains("teralaunch/tests"),
        "CLAUDE.md `## Testing` must cite `teralaunch/tests` so a \
         contributor knows where the frontend tests live."
    );
    assert!(
        section.contains("src-tauri"),
        "CLAUDE.md `## Testing` must cite `src-tauri` tests path."
    );
}

// --------------------------------------------------------------------
// Iter 175 structural pins — Build-commands checklist + API key-diff
// subsection + Mod-Manager subsection set + Disabled-Features + Known-
// Gaps specifics.
// --------------------------------------------------------------------
//
// Iter 107+137 pinned section presence + size threshold + body
// coverage + 7-section roster + v100 API 4-subsection set + Cargo
// feature flags + Testing paths. Iter 175 widens to surfaces those
// pins skip: the actual build commands in the Build section, the
// 5th v100 API subsection (Key Differences — critical for anyone
// touching auth), the Mod Manager's own subsection structure, the
// Architecture section's Disabled Features subsection, and the
// Known Gaps section's specific gap references.

/// The Build & Development Commands section must document the core
/// command surface a new contributor runs during onboarding. Each
/// missing command means a session starts without a working build
/// recipe. Iter 175 pins the five commands the section currently
/// carries so a refactor that truncates the block fails CI.
#[test]
fn build_section_documents_core_commands() {
    let body = claude_body();
    let idx = body
        .find("## Build & Development Commands")
        .expect("build section must exist — iter 137 pin");
    let rest = &body[idx..];
    let section_end = rest[3..].find("\n## ").map(|i| i + 3).unwrap_or(rest.len());
    let section = &rest[..section_end];
    for cmd in [
        "npm install",
        "npm run tauri dev",
        "npm run tauri build",
        "cargo build --features skip-updates",
        "./builder.ps1",
    ] {
        assert!(
            section.contains(cmd),
            "PRD 3.8.1: CLAUDE.md `## Build & Development Commands` \
             must document `{cmd}`. Dropping a core command means a \
             new-session contributor starts without a working recipe \
             for that build path."
        );
    }
}

/// The v100 API section must carry a `### Key Differences from
/// Classic API` subsection. This is the in-repo rosetta stone for
/// anyone who previously worked on the Classic launcher — without
/// it, every auth/session-cookie refactor ends up re-deriving the
/// comparison table from scratch.
#[test]
fn v100_api_section_has_key_differences_subsection() {
    let body = claude_body();
    assert!(
        body.contains("### Key Differences from Classic API"),
        "PRD 3.8.1: CLAUDE.md `## v100 API` must carry \
         `### Key Differences from Classic API` subsection. Without \
         this rosetta stone, every auth/session refactor re-derives \
         the Classic-vs-Classic+ comparison from scratch — a waste \
         of session context per loop iter."
    );
    // The subsection carries a comparison table (Classic vs v100);
    // pin the column headers so a truncation to a bullet list would
    // be caught.
    assert!(
        body.contains("| Classic | Classic+ (v100) |"),
        "PRD 3.8.1: v100 API `### Key Differences` must keep its \
         `| Classic | Classic+ (v100) |` comparison table columns. \
         A truncation to a bullet list loses the side-by-side \
         contract that makes the subsection useful."
    );
}

/// The Mod Manager section must carry its expected subsections:
/// Feature state, Code layout, Build, Deploy, Running the
/// perfection loop. Each missing subsection drops a specific
/// context block that iter 107's body-coverage keyword check
/// doesn't catch (the keywords could appear in any section body;
/// the subsection structure is what organizes them for readers).
#[test]
fn mod_manager_section_has_expected_subsections() {
    let body = claude_body();
    for subsection in [
        "### Feature state",
        "### Code layout",
        "### Build",
        "### Deploy",
        "### Running the perfection loop",
    ] {
        assert!(
            body.contains(subsection),
            "PRD 3.8.1: CLAUDE.md `## Mod Manager` must carry \
             `{subsection}` subsection. Iter 107's body-coverage \
             keyword check doesn't distinguish between `build` \
             appearing in a subsection heading vs a random \
             sentence — dropping the heading weakens readers' \
             ability to find the relevant block."
        );
    }
}

/// The Architecture section must carry `### Disabled Features` — the
/// in-repo list of Classic features stubbed in Classic+ (OAuth,
/// leaderboard consent, profile token exchange, news feed). A new
/// contributor without this list would waste time trying to wire up
/// features that are intentionally off.
#[test]
fn architecture_section_documents_disabled_features() {
    let body = claude_body();
    assert!(
        body.contains("### Disabled Features"),
        "PRD 3.8.1: CLAUDE.md `## Architecture` must carry \
         `### Disabled Features` subsection. Without the list, a \
         new contributor can waste a session trying to wire up \
         Classic-only features (OAuth, leaderboard consent, etc.) \
         that are intentionally stubbed in Classic+."
    );
    // The subsection must name at least one of the stubbed feature
    // classes — catches a heading-without-body truncation.
    let idx = body
        .find("### Disabled Features")
        .expect("subsection present from check above");
    let rest = &body[idx..];
    let window = &rest[..rest.len().min(1500)];
    assert!(
        window.contains("OAuth")
            || window.contains("Leaderboard")
            || window.contains("leaderboard"),
        "PRD 3.8.1: `### Disabled Features` subsection body must \
         name at least one stubbed feature class (OAuth, \
         Leaderboard, etc.). Empty subsection heading would pass \
         the presence check but give readers nothing.\n\
         Window:\n{window}"
    );
}

/// The Known Gaps section must cite specific active gaps by name:
/// XML parsing of the server list, the updater's missing hash-file
/// baseline, and the removed-command JS errors. Without named gaps,
/// the section becomes a placeholder that contributors skim past.
#[test]
fn known_gaps_section_names_specific_gaps() {
    let body = claude_body();
    let idx = body
        .find("## Known Gaps")
        .expect("section must exist — iter 137 pin");
    let rest = &body[idx..];
    let section_end = rest[3..].find("\n## ").map(|i| i + 3).unwrap_or(rest.len());
    let section = &rest[..section_end];
    for gap in [
        "XML",       // Server list XML parsing
        "hash file", // Updater hash file absence
        "removed",   // Removed-command JS errors
    ] {
        assert!(
            section.contains(gap),
            "PRD 3.8.1: CLAUDE.md `## Known Gaps` must mention \
             `{gap}` as one of the specific active gaps. Without \
             named gaps, the section becomes a placeholder — \
             contributors skim past instead of reading.\n\
             Section:\n{section}"
        );
    }
}

// --------------------------------------------------------------------
// Iter 214 structural pins — meta-guard header + CLAUDE_MD path constant
// + MIN_SECTION_LINES value + EXPECTED_SECTIONS count floor + Running-
// the-perfection-loop fix-plan cross-ref.
// --------------------------------------------------------------------
//
// The twelve pins above cover section presence + size + body coverage
// + 7-section roster + v100 API 4+1 subsection set + feature flags +
// testing paths + build commands + Mod Manager subsections + Disabled
// Features + Known Gaps. They do NOT pin: (a) the guard's own header
// cites PRD 3.8.1 — meta-guard contract; (b) the `CLAUDE_MD` constant
// equals its canonical relative path — rename drift hides as opaque
// "file not readable" panic; (c) the `MIN_SECTION_LINES` constant
// retains its canonical value of 30 — a silent lowering to 0 vacuates
// `mod_manager_section_exists_and_meets_size_threshold`; (d)
// `EXPECTED_SECTIONS` carries at least 7 entries (the iter-137
// baseline) — a coordinated trim of list + disk passes the per-entry
// pin but strips the roster; (e) the Mod Manager's
// `### Running the perfection loop` subsection cites `fix-plan.md` —
// without the pointer, the on-ramp doc stops telling contributors
// where the loop state actually lives.

/// The guard's own module header must cite PRD 3.8.1 so a reader
/// chasing a CLAUDE.md drift lands here via section-grep.
#[test]
fn guard_file_header_cites_prd_3_8_1() {
    let body = fs::read_to_string("tests/claude_md_guard.rs")
        .expect("tests/claude_md_guard.rs must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.8.1"),
        "meta-guard contract: tests/claude_md_guard.rs header must \
         cite `PRD 3.8.1`. Without it, a reader chasing a CLAUDE.md \
         drift won't land here via section-grep.\nHeader:\n{header}"
    );
    assert!(
        header.contains("CLAUDE.md"),
        "meta-guard contract: header must name the target doc \
         `CLAUDE.md` so the file-under-test is unambiguous from \
         the header alone."
    );
}

/// The `CLAUDE_MD` path constant must equal `../../CLAUDE.md`
/// verbatim. A rename (e.g. to `../../CLAUDE.md.bak` or moving
/// CLAUDE.md into a subdirectory) would silently cause every
/// `fs::read_to_string(CLAUDE_MD)` call to panic with an opaque
/// "file not readable" message that doesn't name the constant as
/// the root cause.
#[test]
fn claude_md_path_constant_is_canonical() {
    let guard_body =
        fs::read_to_string("tests/claude_md_guard.rs").expect("guard source must be readable");
    assert!(
        guard_body.contains("const CLAUDE_MD: &str = \"../../CLAUDE.md\";"),
        "PRD 3.8.1 (iter 214): tests/claude_md_guard.rs must retain \
         `const CLAUDE_MD: &str = \"../../CLAUDE.md\";` verbatim. A \
         rename of either the constant or the doc must be atomic — \
         otherwise every pin fails with opaque `file not readable` \
         errors."
    );
}

/// `MIN_SECTION_LINES` must retain its canonical value of 30. A
/// silent lowering to 0 would turn
/// `mod_manager_section_exists_and_meets_size_threshold` into a
/// vacuous pass — every empty section would satisfy `len >= 0`.
#[test]
fn min_section_lines_constant_is_thirty() {
    let guard_body =
        fs::read_to_string("tests/claude_md_guard.rs").expect("guard source must be readable");
    assert!(
        guard_body.contains("const MIN_SECTION_LINES: usize = 30;"),
        "PRD 3.8.1 (iter 214): tests/claude_md_guard.rs must retain \
         `const MIN_SECTION_LINES: usize = 30;` verbatim. A silent \
         lowering to 0 would make the size-threshold pin vacuous — \
         every empty section would pass."
    );
}

/// `EXPECTED_SECTIONS` must carry at least 7 entries (the iter-137
/// baseline: Build & Development Commands, v100 API, Architecture,
/// Known Gaps, Cargo Feature Flags, Testing, Mod Manager). A
/// coordinated trim of both the list AND the doc would satisfy
/// `every_expected_section_heading_exists_in_claude_md` while
/// stripping the on-ramp roster. This floor catches silent
/// restructuring of CLAUDE.md.
#[test]
fn expected_sections_count_meets_floor() {
    const MIN_SECTIONS: usize = 7;
    assert!(
        EXPECTED_SECTIONS.len() >= MIN_SECTIONS,
        "PRD 3.8.1 (iter 214): EXPECTED_SECTIONS carries {} entries; \
         floor is {MIN_SECTIONS} (iter-137 baseline). A silent \
         parallel trim of both the list AND CLAUDE.md would satisfy \
         `every_expected_section_heading_exists_in_claude_md` while \
         stripping on-ramp context. Additions are fine; a trim must \
         update this floor atomically.",
        EXPECTED_SECTIONS.len()
    );
}

/// The Mod Manager `### Running the perfection loop` subsection must
/// cite `fix-plan.md`. This is the pointer that tells a fresh loop
/// iter WHERE the iteration counter + queue live — without it,
/// contributors (or future Claude sessions) land in CLAUDE.md with
/// no reference to the authoritative state file.
#[test]
fn running_perfection_loop_subsection_cites_fix_plan_md() {
    let body = claude_body();
    // Find the Mod Manager section + walk to `### Running the
    // perfection loop`.
    let section = mod_manager_section(&body)
        .expect("## Mod Manager section must exist (iter 107 pin)")
        .join("\n");
    let subsection_idx = section
        .find("### Running the perfection loop")
        .expect("## Mod Manager must carry `### Running the perfection loop` (iter 175 pin)");
    // Window to the next `### ` or EOF.
    let rest = &section[subsection_idx..];
    let end = rest[3..]
        .find("\n### ")
        .map(|i| i + 3)
        .unwrap_or(rest.len());
    let subsection = &rest[..end];
    assert!(
        subsection.contains("fix-plan.md"),
        "PRD 3.8.1 (iter 214): `### Running the perfection loop` \
         subsection must cite `fix-plan.md`. Without the pointer, \
         the on-ramp doc stops telling contributors where the \
         iteration counter + queue actually live — every fresh \
         session has to re-derive this from `docs/PRD/` glob.\n\
         Subsection:\n{subsection}"
    );
}

// --------------------------------------------------------------------
// Iter 252 structural pins — SECTION_HEADING constant + EXPECTED_SECTIONS
// ceiling + doc size floor + v100 API comparison table rows + Mod Manager
// feature-state table shape.
// --------------------------------------------------------------------
//
// The seventeen pins above cover section presence, size, body coverage,
// subsection roster, and constant sanity. They do NOT pin:
// (a) the `SECTION_HEADING` constant equals `"## Mod Manager"` verbatim
//     — a drift to `"## Mod manager"` (lowercase m) would silently cause
//     every `mod_manager_section(body)` call to return None;
// (b) the `EXPECTED_SECTIONS` list has a sane ceiling — addition of
//     unrelated sections would pass the ≥ 7 floor while bloating the
//     on-ramp surface;
// (c) CLAUDE.md meets a byte floor — truncation to one section per line
//     passes presence checks while stripping body content;
// (d) the v100 API `### Key Differences from Classic API` comparison
//     table carries the eight rows the section documents (Login, Auth,
//     Account info, Server list, Registration, Leaderboard consent,
//     OAuth, Hash file / Updates);
// (e) the Mod Manager `### Feature state` table carries both `Shipped`
//     and `Blocked` states (required for reviewers to distinguish
//     complete from in-progress work).

/// `SECTION_HEADING` must equal `"## Mod Manager"` verbatim. A drift
/// to `"## Mod manager"` (lowercase m), `"##Mod Manager"` (missing
/// space), or `"### Mod Manager"` (sub-section depth) would silently
/// cause every `mod_manager_section(body)` call to return None, and
/// the prior pins would panic with the opaque "section is missing"
/// message without identifying the constant as root cause.
#[test]
fn section_heading_constant_is_canonical() {
    let guard_body =
        fs::read_to_string("tests/claude_md_guard.rs").expect("guard source must be readable");
    assert!(
        guard_body.contains("const SECTION_HEADING: &str = \"## Mod Manager\";"),
        "PRD 3.8.1 (iter 252): tests/claude_md_guard.rs must retain \
         `const SECTION_HEADING: &str = \"## Mod Manager\";` verbatim. \
         A drift to a different casing, depth, or spacing would \
         silently cause every `mod_manager_section(body)` call to \
         return None — the prior pins would panic with opaque \
         'section is missing' messages."
    );
}

/// `EXPECTED_SECTIONS` must carry a ceiling of ≤ 20 entries. The iter-
/// 214 floor (≥ 7) catches a trim; a ceiling catches runaway growth.
/// Current state: 7 sections. A drift to 20+ top-level sections in
/// CLAUDE.md is a structural problem worth flagging — the file is
/// meant to be on-ramp context, not exhaustive documentation.
#[test]
fn expected_sections_count_has_sane_ceiling() {
    const MAX_SECTIONS: usize = 20;
    assert!(
        EXPECTED_SECTIONS.len() <= MAX_SECTIONS,
        "PRD 3.8.1 (iter 252): EXPECTED_SECTIONS carries {} entries; \
         ceiling is {MAX_SECTIONS}. CLAUDE.md is on-ramp context for \
         new sessions — a 20+ top-level section count signals the \
         doc has drifted into exhaustive reference documentation, \
         which belongs under docs/ instead.",
        EXPECTED_SECTIONS.len()
    );
}

/// CLAUDE.md must meet a minimum byte-size floor. A truncation to
/// a skeleton (7 section headings with no body) would satisfy
/// `every_expected_section_heading_exists_in_claude_md` while
/// stripping the actual content. Current state: ~9.5 KB, 185 lines.
/// A floor of 3000 bytes (~30%) gives generous margin while catching
/// accidental mass-truncation.
#[test]
fn claude_md_file_size_meets_byte_floor() {
    const MIN_BYTES: usize = 3000;
    let bytes = fs::metadata(CLAUDE_MD)
        .unwrap_or_else(|e| panic!("{CLAUDE_MD}: {e}"))
        .len() as usize;
    assert!(
        bytes >= MIN_BYTES,
        "PRD 3.8.1 (iter 252): {CLAUDE_MD} is only {bytes} bytes; \
         floor is {MIN_BYTES}. Truncation past the floor suggests \
         a skeleton rewrite (section headings but no body) — on-ramp \
         context that every future session needs is gone."
    );
}

/// The v100 API `### Key Differences from Classic API` comparison
/// table must carry at least six rows. Iter 175 pinned the table's
/// existence; this pin enforces substance. The section currently
/// documents 8 comparison rows (Login, Auth, Account info, Server
/// list, Registration, Leaderboard consent, OAuth, Hash file /
/// Updates). A floor of 6 gives margin while catching a table
/// collapse to one or two rows.
#[test]
fn v100_api_key_differences_table_meets_row_floor() {
    const MIN_ROWS: usize = 6;
    let body = claude_body();
    let idx = body
        .find("### Key Differences from Classic API")
        .expect("subsection must exist (iter 175 pin)");
    let rest = &body[idx..];
    let end = rest[3..].find("\n## ").map(|i| i + 3).unwrap_or(rest.len());
    let subsection = &rest[..end];
    // Count table rows: lines starting with `| ` that are NOT the
    // header `| Classic | Classic+ (v100) |` or the separator line.
    let row_count = subsection
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("| ")
                && !t.starts_with("| Classic |")
                && !t.starts_with("| Aspect |")
                && !t.starts_with("|---")
                && !t.starts_with("| ---")
        })
        .count();
    assert!(
        row_count >= MIN_ROWS,
        "PRD 3.8.1 (iter 252): v100 API `### Key Differences` \
         comparison table has {row_count} content row(s); floor is \
         {MIN_ROWS}. A table collapse to 1-2 rows would pass the \
         table-exists pin (iter 175) while stripping the side-by-side \
         contract that makes the subsection useful.\nSubsection:\n{subsection}"
    );
}

/// The Mod Manager `### Feature state` table must cite both
/// `Shipped` and `Blocked` state terms. Readers use the table to
/// distinguish complete from in-progress work at a glance; if
/// either state term is missing, the table either over-promises
/// (no Blocked means all work looks shipped) or under-reports
/// (no Shipped means nothing appears complete).
#[test]
fn mod_manager_feature_state_table_cites_shipped_and_blocked() {
    let body = claude_body();
    let section = mod_manager_section(&body)
        .expect("## Mod Manager section must exist (iter 107 pin)")
        .join("\n");
    let subsection_idx = section
        .find("### Feature state")
        .expect("### Feature state subsection must exist (iter 175 pin)");
    let rest = &section[subsection_idx..];
    let end = rest[3..]
        .find("\n### ")
        .map(|i| i + 3)
        .unwrap_or(rest.len());
    let subsection = &rest[..end];
    assert!(
        subsection.contains("Shipped"),
        "PRD 3.8.1 (iter 252): Mod Manager `### Feature state` table \
         must cite `Shipped` state. Without it, all work appears \
         in-progress or blocked — readers can't distinguish complete \
         work from pending.\nSubsection:\n{subsection}"
    );
    assert!(
        subsection.contains("Blocked") || subsection.contains("blocked"),
        "PRD 3.8.1 (iter 252): Mod Manager `### Feature state` table \
         must cite a `Blocked` state. Without it, all work looks \
         shipped — readers can't see which items are gated on \
         upstream decisions (e.g. sec.tauri-v1-eol-plan sign-off).\n\
         Subsection:\n{subsection}"
    );
}

/// Self-test — prove the detector bites on synthetic bad shapes.
#[test]
fn claude_md_detector_self_test() {
    // Bad: file with no Mod Manager heading.
    let no_section = "# Title\n## Build\nsome text\n## Workflow\nmore text\n";
    assert!(
        mod_manager_section(no_section).is_none(),
        "self-test: detector must report None when section is missing"
    );

    // Bad: section exists but too short.
    let short = "# Title\n\n## Mod Manager\n\nJust three lines.\n\n## Next\n";
    let section = mod_manager_section(short).expect("section present");
    assert!(
        section.len() < MIN_SECTION_LINES,
        "self-test: short section must be flagged under threshold"
    );

    // Good: section with plenty of lines and body-coverage keywords.
    let mut good = String::from("# Title\n\n## Mod Manager\n\n");
    for i in 0..40 {
        good.push_str(&format!("line {i}: feature state, build, deploy — cargo test, npm run tauri build, nsis installer, shipped\n"));
    }
    good.push_str("\n## Next\n");
    let section = mod_manager_section(&good).expect("section present");
    assert!(section.len() >= MIN_SECTION_LINES);
    let joined = section.join("\n").to_lowercase();
    assert!(joined.contains("feature state"));
    assert!(joined.contains("build"));
    assert!(joined.contains("deploy"));

    // Bad shape E (iter 137): CLAUDE.md missing an expected section.
    let missing_section = "# Title\n\n## Build & Development Commands\n\nstuff\n";
    assert!(
        !missing_section.contains("## Testing"),
        "self-test: body missing expected section must be flagged"
    );

    // Bad shape F (iter 137): v100 API section without one of the 4
    // endpoint subsections.
    let partial_api =
        "## v100 API (Classic+ Server)\n\n### Authentication\n### Registration\n### Account Info\n";
    assert!(
        !partial_api.contains("### Other Endpoints"),
        "self-test: v100 API section missing a subsection must be \
         flagged"
    );

    // Bad shape G (iter 137): Cargo Feature Flags section missing
    // the skip-updates flag doc.
    let no_skip = "## Cargo Feature Flags\n\n- `custom-protocol` required\n";
    assert!(
        !no_skip.contains("skip-updates"),
        "self-test: feature-flags section missing skip-updates must \
         be flagged"
    );
}
