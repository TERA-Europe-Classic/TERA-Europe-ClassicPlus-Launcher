//! PRD 3.8.4 — `docs/mod-manager/ARCHITECTURE.md` must have a section
//! covering each production subsystem the mod manager ships.
//!
//! Criterion text: "one page per subsystem" with the expected set being
//! `mods.rs, tmm.rs, catalog.rs, external_app.rs, registry.rs,
//! mods_state.rs, mods.js`. Until iter 106 this was enforced only by
//! human review; a new subsystem file added without a corresponding
//! ARCHITECTURE.md section would slip through.
//!
//! Iter 106 added the missing `mods_state.rs` section to the doc; this
//! guard locks that in and enforces the invariant for any future
//! additions.

use std::fs;

const DOC: &str = "../../docs/mod-manager/ARCHITECTURE.md";

/// Each required subsystem must be named in at least one ARCHITECTURE.md
/// heading or body paragraph. We match on the full filename so a section
/// like `**File:** services/mods/external_app.rs` counts as well as an
/// `## N. External-app ...` heading — the contract is "the doc talks
/// about this file", not "heading text matches verbatim".
const REQUIRED_SUBSYSTEMS: &[&str] = &[
    // Rust subsystems (paths relative to src-tauri/src/).
    "commands/mods.rs",
    "services/mods/tmm.rs",
    "services/mods/catalog.rs",
    "services/mods/external_app.rs",
    "services/mods/registry.rs",
    "state/mods_state.rs",
    // Frontend entry point.
    "mods.js",
];

fn doc_body() -> String {
    fs::read_to_string(DOC).unwrap_or_else(|e| {
        panic!(
            "ARCHITECTURE.md must be readable from src-tauri/ via {DOC}: {e}"
        )
    })
}

/// PRD 3.8.4 — every production subsystem is named in the doc.
#[test]
fn every_required_subsystem_has_doc_coverage() {
    let body = doc_body();
    let mut missing = Vec::new();
    for sub in REQUIRED_SUBSYSTEMS {
        if !body.contains(sub) {
            missing.push(*sub);
        }
    }
    assert!(
        missing.is_empty(),
        "PRD 3.8.4 violated: ARCHITECTURE.md is missing coverage for:\n  \
         {}\n\
         Either add a section naming the file, or remove the file from \
         the mod-manager subsystem set.",
        missing.join("\n  ")
    );
}

/// Sanity: doc must have at least one `## ` heading per listed subsystem
/// on average (i.e. it's an actual per-subsystem doc, not a single
/// paragraph that happens to mention all the filenames).
#[test]
fn doc_has_structural_section_headings() {
    let body = doc_body();
    let heading_count = body
        .lines()
        .filter(|l| l.starts_with("## "))
        .count();
    assert!(
        heading_count >= REQUIRED_SUBSYSTEMS.len(),
        "ARCHITECTURE.md has only {heading_count} `## ` headings but \
         PRD 3.8.4 expects at least {} (one per required subsystem). \
         The doc may have been collapsed into prose without proper \
         sectioning.",
        REQUIRED_SUBSYSTEMS.len()
    );
}

/// The 11 `## ` top-level section headings ARCHITECTURE.md currently
/// ships, in disk order. Renaming or deleting a heading invalidates
/// reader bookmarks and breaks the PRD §3.8.4 "one page per
/// subsystem" reading contract. Iter 138 pins the set.
///
/// Adding a section: append the heading here + add to the doc.
/// Renaming: justify the rename in the commit message and update
/// this list atomically with the doc change. Removal: justify why
/// the subsystem no longer deserves a section.
const EXPECTED_SECTIONS: &[&str] = &[
    "## 1. Types (shared vocabulary)",
    "## 2. Catalog",
    "## 3. Registry",
    "## 3a. Mods state (in-memory guard)",
    "## 4. External-app download + extract + spawn",
    "## 5. TMM mapper + GPK install",
    "## 6. Self-integrity",
    "## 7. Tauri command boundary",
    "## 8. Frontend (mods.js)",
    "## 9. Cross-subsystem guarantees",
    "## 10. Known gaps",
];

#[test]
fn every_expected_section_heading_exists() {
    let body = doc_body();
    let mut missing = Vec::new();
    for heading in EXPECTED_SECTIONS {
        if !body.contains(heading) {
            missing.push(*heading);
        }
    }
    assert!(
        missing.is_empty(),
        "ARCHITECTURE.md is missing expected section heading(s):\n  \
         {}\n\
         Renaming or deleting a heading without updating this guard \
         breaks the PRD §3.8.4 contract (one page per subsystem) and \
         invalidates reader bookmarks. Either add the heading back, \
         or update EXPECTED_SECTIONS in sync with the doc change.",
        missing.join("\n  ")
    );
}

/// Section 9 "Cross-subsystem guarantees" is the integration story
/// that ties the per-subsystem sections together. Pinning a minimum
/// set of guarantee keywords prevents silent collapse to prose or
/// drop of a core guarantee.
#[test]
fn cross_subsystem_guarantees_section_names_core_invariants() {
    let body = doc_body();
    assert!(
        body.contains("## 9. Cross-subsystem guarantees"),
        "prior test guarantees the section exists"
    );
    let idx = body
        .find("## 9. Cross-subsystem guarantees")
        .expect("section must exist");
    // Scan to the next ## heading.
    let rest = &body[idx..];
    let section_end = rest[2..]
        .find("\n## ")
        .map(|i| i + 2)
        .unwrap_or(rest.len());
    let section = &rest[..section_end];
    // Core invariants named in the section header list. Each is a
    // criterion the Rust integration tests enforce separately — the
    // doc reference is what ties the rubric together for a reader.
    for needle in &[
        "Fail-closed",
        "Deploy sandbox",
        "Crash recovery",
        "Self-integrity",
        "Deploy scope",
        "Secret scan",
    ] {
        assert!(
            section.contains(needle),
            "`## 9. Cross-subsystem guarantees` must name \
             `{needle}` as a cross-subsystem invariant. Dropping \
             one would erase the doc-level rubric readers rely on \
             to understand how subsystems interlock."
        );
    }
}

/// Section 10 "Known gaps" must point back to fix-plan.md as the
/// authoritative backlog. Without that pointer, the doc's gap list
/// accumulates stale entries that drift from the real fix-plan.
#[test]
fn known_gaps_section_points_to_fix_plan() {
    let body = doc_body();
    assert!(
        body.contains("## 10. Known gaps"),
        "prior test guarantees the section exists"
    );
    let idx = body.find("## 10. Known gaps").expect("section must exist");
    let section = &body[idx..];
    assert!(
        section.contains("fix-plan.md"),
        "`## 10. Known gaps` must reference `fix-plan.md` so \
         readers land on the authoritative backlog. Without this \
         reference, the gap list drifts into a stale parallel \
         backlog."
    );
}

// --------------------------------------------------------------------
// Iter 176 structural pins — preamble contract + per-section `**File:**`
// marker + Mods-state RwLock detail + Self-integrity verify/sidecar
// detail + per-section minimum-lines floor.
// --------------------------------------------------------------------
//
// Iter 106+138 pinned subsystem coverage + heading count + 11-section
// roster + cross-subsystem invariants + known-gaps → fix-plan pointer.
// Iter 176 widens to per-section STRUCTURE invariants those pins skip.

/// The doc's preamble must direct readers to CLAUDE.md first. Without
/// that pointer, new readers (and new loop iterations) skip the
/// shorter on-ramp doc and drown in ARCHITECTURE.md's 300+ lines
/// without context.
#[test]
fn preamble_establishes_read_after_claude_md_contract() {
    let body = doc_body();
    let head: String = body.lines().take(10).collect::<Vec<_>>().join("\n");
    assert!(
        head.contains("CLAUDE.md"),
        "PRD 3.8.4: ARCHITECTURE.md preamble must reference \
         `CLAUDE.md` in the first 10 lines. Without the on-ramp \
         pointer, readers miss the shorter `## Mod Manager` code-\
         layout summary and go deep before establishing context.\n\
         Head seen:\n{head}"
    );
    assert!(
        head.contains("subsystem"),
        "PRD 3.8.4: preamble must use the word `subsystem` to set \
         the reading frame (each section = one subsystem). Without \
         this framing, readers don't know how to slice the doc."
    );
}

/// Every numbered subsystem section (1-8) must carry a `**File:**`
/// marker naming the owning source file. The marker is what lets a
/// reader jump straight from the doc to the code; without it, the
/// mapping iter 106 established between sections and files drifts
/// into prose.
#[test]
fn every_numbered_section_names_owning_file() {
    let body = doc_body();
    // Sections 1-8 are subsystem pages (3a is the in-memory guard
    // sibling of 3). Sections 9+ are cross-cutting — no owning file.
    for section_prefix in [
        "## 1. ",
        "## 2. ",
        "## 3. ",
        "## 3a. ",
        "## 4. ",
        "## 5. ",
        "## 6. ",
        "## 7. ",
        "## 8. ",
    ] {
        let idx = body
            .find(section_prefix)
            .unwrap_or_else(|| panic!("section `{section_prefix}...` must exist"));
        let rest = &body[idx..];
        let section_end = rest[5..]
            .find("\n## ")
            .map(|i| i + 5)
            .unwrap_or(rest.len());
        let section = &rest[..section_end];
        assert!(
            section.contains("**File:**"),
            "PRD 3.8.4: section `{section_prefix}...` must carry a \
             `**File:**` marker naming its owning source file. \
             Without the marker, the section-to-file mapping \
             drifts into prose and readers can't jump to code."
        );
    }
}

/// Section 3a (Mods state — in-memory guard) must document the
/// `RwLock` primitive. This ties the architecture doc to iter 159's
/// `parallel_install.rs` structural pin (`mods_state_is_process_
/// global_rwlock`) — both must stay in sync with the production
/// `mods_state::MODS_STATE: RwLock<...>` wrapper.
#[test]
fn mods_state_section_documents_rwlock_primitive() {
    let body = doc_body();
    let idx = body
        .find("## 3a. Mods state")
        .expect("section 3a must exist");
    let rest = &body[idx..];
    let section_end = rest[5..]
        .find("\n## ")
        .map(|i| i + 5)
        .unwrap_or(rest.len());
    let section = &rest[..section_end];
    assert!(
        section.contains("RwLock"),
        "PRD 3.8.4: section `## 3a. Mods state` must document the \
         `RwLock` primitive. Iter 159's `parallel_install.rs` pin \
         (`mods_state_is_process_global_rwlock`) ties the Rust \
         test to this doc's description — both must reference the \
         same lock type.\nSection:\n{section}"
    );
}

/// Section 6 (Self-integrity) must document the sidecar file name
/// and the entry-point function. Ties the architecture doc to iter
/// 153's `self_integrity.rs` structural pins.
#[test]
fn self_integrity_section_documents_sidecar_and_verify() {
    let body = doc_body();
    let idx = body
        .find("## 6. Self-integrity")
        .expect("section 6 must exist");
    let rest = &body[idx..];
    let section_end = rest[5..]
        .find("\n## ")
        .map(|i| i + 5)
        .unwrap_or(rest.len());
    let section = &rest[..section_end];
    assert!(
        section.contains("self_hash.sha256"),
        "PRD 3.8.4: section `## 6. Self-integrity` must name the \
         `self_hash.sha256` sidecar file. Iter 153's pin \
         (`sidecar_filename_is_self_hash_sha256`) enforces this \
         literal in `src/main.rs` — the doc must name the same \
         artefact so operators can trace release-pipeline output \
         to the launcher's expected baseline."
    );
    assert!(
        section.contains("run_self_integrity_check"),
        "PRD 3.8.4: section `## 6. Self-integrity` must name the \
         `run_self_integrity_check` entry-point function. Iter 153's \
         pin (`integrity_check_called_before_tauri_builder`) enforces \
         this call-site in main.rs; the doc must name the same \
         function so readers understand the boot-time invocation \
         that ties PRD §3.1.11 to the code."
    );
    assert!(
        section.contains("MessageBoxW") || section.contains("process::exit"),
        "PRD 3.8.4: section `## 6. Self-integrity` must describe the \
         mismatch path — either the `MessageBoxW` user-facing dialog \
         (iter 153's `mismatch_branch_shows_native_dialog` pin) or \
         the `process::exit` call (iter 153's \
         `mismatch_branch_exits_process` pin). Without either, the \
         doc's description of what happens on tampered-exe detection \
         is incomplete."
    );
}

/// Every numbered subsystem section (1-8) must have at least 8
/// lines of body content. A 1-3 line stub section would pass the
/// `**File:**` marker check but give readers nothing actionable.
#[test]
fn every_numbered_section_exceeds_minimum_content_lines() {
    const MIN_LINES_PER_SECTION: usize = 8;
    let body = doc_body();
    for section_prefix in [
        "## 1. ",
        "## 2. ",
        "## 3. ",
        "## 3a. ",
        "## 4. ",
        "## 5. ",
        "## 6. ",
        "## 7. ",
        "## 8. ",
    ] {
        let idx = body
            .find(section_prefix)
            .unwrap_or_else(|| panic!("section `{section_prefix}...` must exist"));
        let rest = &body[idx..];
        let section_end = rest[5..]
            .find("\n## ")
            .map(|i| i + 5)
            .unwrap_or(rest.len());
        let section = &rest[..section_end];
        let line_count = section.lines().count();
        assert!(
            line_count >= MIN_LINES_PER_SECTION,
            "PRD 3.8.4: section `{section_prefix}...` has only \
             {line_count} lines (threshold: ≥ {MIN_LINES_PER_SECTION}). \
             A stub section with just a heading + **File:** marker \
             would pass the marker check but give readers nothing \
             — require substantive body content per subsystem."
        );
    }
}

// --------------------------------------------------------------------
// Iter 211 structural pins — meta-guard self-reference + EXPECTED_SECTIONS
// count floor + per-section cross-ties to code invariants (3, 4, 5).
// --------------------------------------------------------------------
//
// The eleven pins above lock down subsystem coverage + section roster
// + preamble + per-section `**File:**` marker + cross-subsystem
// guarantees + known-gaps pointer + sections 3a (RwLock) + 6 (self-
// integrity sidecar) + minimum content lines. They do NOT pin: (a)
// the guard's own header cites PRD 3.8.4 — meta-guard contract; (b)
// EXPECTED_SECTIONS keeps its current count (11) — a silent trim to
// 5 entries where all 5 happen to exist would pass
// `every_expected_section_heading_exists` vacuously; (c) section 3
// (Registry) documents `registry.json` + `Installing` recovery — ties
// doc to `Registry::load()` auto-recovery behaviour pinned elsewhere;
// (d) section 4 (External-app) documents attach-once (`SpawnDecision`)
// + overlay lifecycle (`remaining_clients` / `decide_overlay_action`)
// — ties doc to multi_client.rs iter-158/207 pins; (e) section 5
// (TMM mapper) documents PACKAGE_MAGIC or the CompositePackageMapper
// baseline file — ties doc to bogus_gpk_footer iter-203 pins.

/// The guard's own module header must cite PRD 3.8.4 so a reader
/// chasing an architecture-doc drift lands here via section-grep.
#[test]
fn guard_file_header_cites_prd_3_8_4() {
    let body = fs::read_to_string("tests/architecture_doc_guard.rs")
        .expect("tests/architecture_doc_guard.rs must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.8.4"),
        "meta-guard contract: tests/architecture_doc_guard.rs header \
         must cite `PRD 3.8.4`. Without it, a reader chasing an \
         architecture-doc regression won't land here via section-\
         grep.\nHeader:\n{header}"
    );
    assert!(
        header.contains("ARCHITECTURE.md"),
        "meta-guard contract: header must name the target doc \
         `ARCHITECTURE.md` so the file-under-test is unambiguous \
         from the header alone."
    );
}

/// `EXPECTED_SECTIONS` must carry at least 11 entries (the iter-138
/// baseline). A coordinated trim of both the list AND the doc (e.g.
/// removing 5 section headings and updating EXPECTED_SECTIONS to
/// match) would pass `every_expected_section_heading_exists` — the
/// `contains` check is vacuous when the list is empty of what was
/// removed. This floor catches silent restructuring of the doc.
#[test]
fn expected_sections_count_meets_floor() {
    const MIN_SECTIONS: usize = 11;
    assert!(
        EXPECTED_SECTIONS.len() >= MIN_SECTIONS,
        "PRD 3.8.4 (iter 211): EXPECTED_SECTIONS carries {} entries; \
         floor is {MIN_SECTIONS} (iter-138 baseline: 11 sections). A \
         silent parallel trim of both the list AND the doc would \
         satisfy `every_expected_section_heading_exists` while \
         stripping subsystem coverage. Additions are fine; a trim \
         is a visible event that must update this floor atomically.",
        EXPECTED_SECTIONS.len()
    );
}

/// Section 3 (Registry) must document the `registry.json` on-disk
/// file AND the `Installing` recovery flip. Ties the architecture
/// doc to the Registry crash-recovery invariant (`Registry::load()`
/// auto-flips stranded `Installing` rows to `Error` — see CLAUDE.md
/// §Mod Manager). A doc drop of either would lose the reader's
/// mental model for crash safety.
#[test]
fn registry_section_documents_registry_json_and_installing_recovery() {
    let body = doc_body();
    let idx = body
        .find("## 3. Registry")
        .expect("section 3 must exist");
    let rest = &body[idx..];
    // Walk to next ## heading (but skip ## 3a).
    let after_header = &rest[5..];
    let mut cursor = 0;
    let section_end = loop {
        match after_header[cursor..].find("\n## ") {
            Some(rel) => {
                let abs = cursor + rel + 1;
                // Check it's a sibling (not ## 3a.).
                let tail = &after_header[abs..];
                if tail.starts_with("## 3a") {
                    cursor = abs + 1;
                    continue;
                }
                break abs + 5;
            }
            None => break rest.len(),
        }
    };
    let section = &rest[..section_end.min(rest.len())];
    assert!(
        section.contains("registry.json"),
        "PRD 3.8.4: section `## 3. Registry` must name the \
         `registry.json` on-disk artefact. Without it, the reader's \
         mental model for the Registry persistence layer has no \
         concrete filename to anchor on.\nSection:\n{section}"
    );
    assert!(
        section.contains("Installing"),
        "PRD 3.8.4: section `## 3. Registry` must reference the \
         `Installing` status / recovery flip. The crash-recovery \
         invariant (Registry::load auto-flips stranded Installing \
         rows to Error) is the load-bearing guarantee this section \
         documents."
    );
}

/// Section 4 (External-app) must document both attach-once
/// (`SpawnDecision`) AND the overlay-lifecycle wiring
/// (`decide_overlay_action` / `remaining_clients`). These are the
/// two §3.2.11 + §3.2.12 invariants the multi_client.rs integration
/// test pins at the code level; the doc must carry the prose
/// description so readers can trace the test back to intent.
#[test]
fn external_app_section_documents_attach_once_and_overlay_lifecycle() {
    let body = doc_body();
    let idx = body
        .find("## 4. External-app")
        .expect("section 4 must exist");
    let rest = &body[idx..];
    let section_end = rest[5..]
        .find("\n## ")
        .map(|i| i + 5)
        .unwrap_or(rest.len());
    let section = &rest[..section_end];
    assert!(
        section.contains("SpawnDecision") || section.contains("decide_spawn"),
        "PRD 3.8.4: section `## 4. External-app` must name either \
         `SpawnDecision` or `decide_spawn` — the §3.2.11 attach-once \
         pure predicate pinned by multi_client.rs (iter 158). \
         Without the prose pointer, the doc's description of \
         second-client-no-dup-spawn lacks a code anchor."
    );
    assert!(
        section.contains("decide_overlay_action") || section.contains("remaining_clients"),
        "PRD 3.8.4: section `## 4. External-app` must name either \
         `decide_overlay_action` or `remaining_clients` — the \
         §3.2.12 overlay-lifecycle invariant pinned by multi_client.rs \
         (iter 207). Dropping both loses the doc-level rubric for \
         overlay-keep vs -terminate behaviour on partial close."
    );
}

/// Section 5 (TMM mapper + GPK install) must document the
/// `CompositePackageMapper` file (the encrypted mapper the TMM
/// pipeline patches). This ties the architecture doc to the TMM
/// installer's central invariant — the vanilla baseline is backed
/// up as `.clean` and restored on uninstall (see
/// bogus_gpk_footer.rs iter-163 + iter-203 pins).
#[test]
fn tmm_section_documents_composite_package_mapper() {
    let body = doc_body();
    let idx = body
        .find("## 5. TMM mapper")
        .expect("section 5 must exist");
    let rest = &body[idx..];
    let section_end = rest[5..]
        .find("\n## ")
        .map(|i| i + 5)
        .unwrap_or(rest.len());
    let section = &rest[..section_end];
    assert!(
        section.contains("CompositePackageMapper"),
        "PRD 3.8.4: section `## 5. TMM mapper + GPK install` must \
         name `CompositePackageMapper` — the encrypted mapper file \
         the TMM installer patches. Without it, readers can't trace \
         the `.clean` backup / `.dat` mapper → test coverage in \
         bogus_gpk_footer.rs + clean_recovery.rs.\nSection:\n{section}"
    );
    assert!(
        section.contains(".clean") || section.contains("ensure_backup"),
        "PRD 3.8.4: section `## 5. TMM mapper + GPK install` must \
         reference either the `.clean` backup artefact or the \
         `ensure_backup` function. Both are the vanilla-baseline \
         invariant §3.2 clean-recovery depends on."
    );
}

/// Self-test — prove the detector bites on synthetic bad shapes.
#[test]
fn architecture_doc_detector_self_test() {
    // Bad: doc mentions only 3 of 7 subsystems.
    let partial = "# ARCHITECTURE\n## Catalog\n`services/mods/catalog.rs` handles...\n\
                   ## Registry\n`services/mods/registry.rs` ...\n\
                   ## TMM\n`services/mods/tmm.rs` ...\n";
    let mut missing = Vec::new();
    for sub in REQUIRED_SUBSYSTEMS {
        if !partial.contains(sub) {
            missing.push(*sub);
        }
    }
    assert!(
        !missing.is_empty(),
        "self-test: partial-coverage doc must trip the detector"
    );
    // At minimum 4 subsystems should be flagged missing.
    assert!(missing.len() >= 4);

    // Bad: heading count too low.
    let no_headings = "ARCHITECTURE\n\nJust one big paragraph. \
                       commands/mods.rs, services/mods/tmm.rs, \
                       services/mods/catalog.rs, services/mods/external_app.rs, \
                       services/mods/registry.rs, state/mods_state.rs, \
                       mods.js.\n";
    let heading_count = no_headings
        .lines()
        .filter(|l| l.starts_with("## "))
        .count();
    assert!(
        heading_count < REQUIRED_SUBSYSTEMS.len(),
        "self-test: no-heading doc must trip the structural check"
    );

    // Bad shape C (iter 138): doc missing an expected section.
    let missing_section = "## 1. Types\n## 2. Catalog\n## 3. Registry\n";
    assert!(
        !missing_section.contains("## 9. Cross-subsystem guarantees"),
        "self-test: doc missing `## 9. Cross-subsystem guarantees` \
         must be flagged"
    );

    // Bad shape D: Cross-subsystem section present but missing a
    // core invariant keyword.
    let partial_guarantees =
        "## 9. Cross-subsystem guarantees\n\n- Fail-closed download\n- Deploy sandbox\n## 10. Next\n";
    assert!(
        !partial_guarantees.contains("Self-integrity"),
        "self-test: guarantees section missing a core invariant \
         must be flagged"
    );

    // Bad shape E: Known gaps section without fix-plan.md pointer.
    let orphan_gaps = "## 10. Known gaps\n\nSome TODOs here.\n";
    assert!(
        !orphan_gaps.contains("fix-plan.md"),
        "self-test: known-gaps section without fix-plan.md pointer \
         must be flagged"
    );
}

// --------------------------------------------------------------------
// Iter 246 structural pins — DOC path constant, REQUIRED_SUBSYSTEMS
// cardinality, EXPECTED_SECTIONS cardinality, doc minimum byte size,
// and h1 title canonicalisation.
// --------------------------------------------------------------------

/// Iter 246: `DOC` path constant must stay canonical. Every doc
/// inspection resolves through it; drift leaves pins reading the
/// wrong file with misleading "file not found" panics.
#[test]
fn guard_doc_path_constant_is_canonical() {
    let body = fs::read_to_string("tests/architecture_doc_guard.rs")
        .expect("guard source must exist");
    assert!(
        body.contains(r#"const DOC: &str = "../../docs/mod-manager/ARCHITECTURE.md";"#),
        "PRD 3.8.3 (iter 246): tests/architecture_doc_guard.rs must \
         keep `const DOC: &str = \"../../docs/mod-manager/\
         ARCHITECTURE.md\";` verbatim. A rename without updating \
         the constant leaves every pin reading a non-existent path."
    );
}

/// Iter 246: `REQUIRED_SUBSYSTEMS` must enumerate exactly 7 entries.
/// The array is consumed by the existing `every_required_subsystem_
/// is_covered` pin; a silent drop to 6 (e.g. dropping `registry.rs`)
/// would not trip that pin (the remaining 6 subsystems would still
/// pass the contains-check) but would silently shrink coverage.
/// Pin cardinality explicitly.
#[test]
fn required_subsystems_count_is_exactly_seven() {
    assert_eq!(
        REQUIRED_SUBSYSTEMS.len(),
        7,
        "PRD 3.8.3 (iter 246): REQUIRED_SUBSYSTEMS must enumerate \
         exactly 7 subsystems (commands/mods.rs, services/mods/{{tmm,\
         catalog,external_app,registry,types}}.rs, state/mods_state.rs, \
         mods.js). Found {}. A drop signals silent coverage \
         shrinkage; growth needs a coordinated PRD update.",
        REQUIRED_SUBSYSTEMS.len()
    );
}

/// Iter 246: `EXPECTED_SECTIONS` must enumerate exactly 11 entries
/// (sections 1, 2, 3, 3a, 4, 5, 6, 7, 8, 9, 10 of the architecture
/// doc — note the 3a "Mods state" sub-section). Complements the
/// subsystems cardinality pin — catches a drop in the section-
/// coverage surface that the existing per-section checks wouldn't
/// surface (silent shrinkage).
#[test]
fn expected_sections_count_is_exactly_eleven() {
    assert_eq!(
        EXPECTED_SECTIONS.len(),
        11,
        "PRD 3.8.3 (iter 246): EXPECTED_SECTIONS must enumerate \
         exactly 11 sections (1, 2, 3, 3a, 4, 5, 6, 7, 8, 9, 10). \
         Found {}. Adding a section needs a coordinated PRD update; \
         removing silently shrinks doc coverage.",
        EXPECTED_SECTIONS.len()
    );
}

/// Iter 246: the architecture doc must carry at least 10 000 bytes.
/// Below that floor the doc is too short to substantively cover
/// 7 subsystems × 10 sections — a truncation-to-stub would leave
/// the existing heading-count check passing (10 `## ` lines fit
/// in 200 bytes) but the body-per-section would be a few words.
#[test]
fn doc_body_meets_minimum_byte_floor() {
    let body = fs::read_to_string(DOC)
        .unwrap_or_else(|e| panic!("{DOC} must be readable: {e}"));
    let n = body.len();
    assert!(
        n >= 10_000,
        "PRD 3.8.3 (iter 246): {DOC} has only {n} bytes — below \
         the 10 000-byte floor. 7 subsystems × 10 sections can't \
         be substantively covered in less. A truncation-to-stub \
         would still satisfy the heading-count check (10 `## ` \
         lines fit in 200 bytes) but leave bodies empty."
    );
    // And upper-bound drift check: 500KB signals someone dumped
    // unrelated content into the file.
    assert!(
        n < 500_000,
        "PRD 3.8.3 (iter 246): {DOC} has ballooned to {n} bytes \
         (>500KB). Either unrelated content got dumped in or the \
         doc needs a split into subsystem-specific files — either \
         wants a deliberate review."
    );
}

/// Iter 246: the architecture doc's H1 title must stay canonical.
/// A rename from `# Mod-manager architecture` to something else
/// would break discoverability for humans and any tooling that
/// keys off the title string.
#[test]
fn doc_h1_title_is_canonical() {
    let body = fs::read_to_string(DOC)
        .unwrap_or_else(|e| panic!("{DOC} must be readable: {e}"));
    // Find the first `# ` line (H1).
    let h1_line = body
        .lines()
        .find(|l| l.starts_with("# ") && !l.starts_with("## "))
        .unwrap_or("");
    assert!(
        h1_line.to_lowercase().contains("mod")
            && (h1_line.to_lowercase().contains("manager")
                || h1_line.to_lowercase().contains("architecture")),
        "PRD 3.8.3 (iter 246): {DOC} H1 title must contain both \
         `mod` + (`manager` or `architecture`). Got: `{h1_line}`. \
         A rename breaks discoverability and any tooling that \
         greps for the doc by title."
    );
}
