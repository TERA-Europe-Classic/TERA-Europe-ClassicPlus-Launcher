//! PRD §3.8.7 — per-unit audit doc coverage gate.
//!
//! The PRD's §3.8.7 success criterion is "per-unit audit doc exists
//! for every entry in the catalog + every External app + every
//! launcher module" with an exit floor of ≥ 110 (99 GPK + 2 external
//! + 7 launcher + 13 TCC class layouts = 121 total target).
//!
//! This guard counts real `.md` files under `docs/PRD/audits/units/`
//! and asserts the floor. Starts relaxed (iter 229: 2 external + 1
//! GPK exemplar + README + TEMPLATE), tightens as categories
//! complete. Bumping the floor requires writing the audit docs
//! first, then raising the constants in this file — a deliberate
//! acknowledgement that the rollout moved, not a silent PRD drift.

use std::fs;
use std::path::{Path, PathBuf};

const AUDITS_UNITS_DIR: &str = "../../docs/PRD/audits/units";

/// Current floor per category. Raise these as rollout progresses.
const FLOOR_EXTERNAL: usize = 2;
const FLOOR_GPK: usize = 1;
const FLOOR_LAUNCHER: usize = 0;
const FLOOR_TCC: usize = 0;

/// Eventual target per PRD §3.8.7 / §5.5. Keep documented here so a
/// reader sees both current + end state.
#[allow(dead_code)]
const TARGET_EXTERNAL: usize = 2;
#[allow(dead_code)]
const TARGET_GPK: usize = 99;
#[allow(dead_code)]
const TARGET_LAUNCHER: usize = 7;
#[allow(dead_code)]
const TARGET_TCC: usize = 13;

fn count_md_in(subdir: &str) -> usize {
    let dir = PathBuf::from(AUDITS_UNITS_DIR).join(subdir);
    if !dir.exists() {
        return 0;
    }
    fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x == "md")
        })
        .count()
}

fn assert_audits_units_dir_exists() {
    let d = Path::new(AUDITS_UNITS_DIR);
    assert!(
        d.exists() && d.is_dir(),
        "PRD §3.8.7: audits/units/ directory must exist at `{}`",
        d.display()
    );
    let readme = d.join("README.md");
    assert!(
        readme.exists(),
        "PRD §3.8.7: audits/units/README.md must exist (rollout plan)"
    );
    let template = d.join("TEMPLATE.md");
    assert!(
        template.exists(),
        "PRD §3.8.7: audits/units/TEMPLATE.md must exist (authoring template)"
    );
}

#[test]
fn audits_units_infrastructure_is_present() {
    assert_audits_units_dir_exists();
}

#[test]
fn external_audit_doc_floor_is_met() {
    let n = count_md_in("external");
    assert!(
        n >= FLOOR_EXTERNAL,
        "PRD §3.8.7: audits/units/external/ floor is {FLOOR_EXTERNAL} \
         (target {TARGET_EXTERNAL}); currently {n}. Write the missing \
         audit doc(s) and keep this floor synced."
    );
}

#[test]
fn gpk_audit_doc_floor_is_met() {
    let n = count_md_in("gpk");
    assert!(
        n >= FLOOR_GPK,
        "PRD §3.8.7: audits/units/gpk/ floor is {FLOOR_GPK} \
         (target {TARGET_GPK}); currently {n}. Write the missing \
         audit doc(s) and keep this floor synced."
    );
}

#[test]
fn launcher_audit_doc_floor_is_met() {
    // Tautological at FLOOR_LAUNCHER=0 today; the assert exists so a
    // future floor bump surfaces on a docs shrink regression.
    let n = count_md_in("launcher");
    let floor = FLOOR_LAUNCHER;
    assert!(
        n >= floor,
        "PRD §3.8.7: audits/units/launcher/ floor is {floor} \
         (target {TARGET_LAUNCHER}); currently {n}."
    );
}

#[test]
fn tcc_audit_doc_floor_is_met() {
    // Same tautology shield as launcher — see above.
    let n = count_md_in("tcc");
    let floor = FLOOR_TCC;
    assert!(
        n >= floor,
        "PRD §3.8.7: audits/units/tcc/ floor is {floor} \
         (target {TARGET_TCC}); currently {n}."
    );
}

#[test]
fn total_audit_doc_count_progresses_toward_110() {
    let total = count_md_in("external")
        + count_md_in("gpk")
        + count_md_in("launcher")
        + count_md_in("tcc");
    let floor =
        FLOOR_EXTERNAL + FLOOR_GPK + FLOOR_LAUNCHER + FLOOR_TCC;
    assert!(
        total >= floor,
        "PRD §3.8.7: total audit-doc count is {total}; current floor \
         {floor}; eventual target ≥ 110 (99 GPK + 2 external + 7 \
         launcher + 13 TCC, per §5.5). Keep raising the per-category \
         floors as rollout progresses."
    );
}
