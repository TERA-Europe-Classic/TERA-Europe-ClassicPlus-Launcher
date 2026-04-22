//! PRD §3.3.1 — every catalog entry's lifecycle-gate invariants.
//!
//! The PRD's 3.3.1 success criterion reads "every catalog id: install
//! → enable → game-launch-spawn → game-exit-cleanup → uninstall →
//! mapper-restored exits 0". The full lifecycle runs on a live TERA
//! install and cannot execute in CI. What CAN run in CI is the
//! **pre-lifecycle predicate gate** — every entry's metadata must
//! satisfy the invariants every install-path / enable-path / sandbox
//! predicate depends on. If a catalog entry fails any gate, the live
//! lifecycle for that entry would fail too.
//!
//! Scope per entry (both kinds):
//!   - non-empty id + name + author + version
//!   - kind ∈ {external, gpk}
//!   - download_url is https://
//!   - sha256 is 64 hex chars
//!   - size_bytes > 0
//!
//! Scope per `kind = gpk`:
//!   - URL's basename passes `tmm::is_safe_gpk_container_filename`
//!     (CookedPC-sandbox gate)
//!
//! Scope per `kind = external`:
//!   - executable_relpath is non-empty
//!   - executable_relpath is a relative path (no `\\` UNC prefix, no
//!     `/` absolute prefix, no `..` component)
//!
//! A failing entry is flagged with its id so the catalog maintainer
//! knows exactly which to fix. Target: 101/101 entries pass.
//!
//! The fixture `tests/fixtures/catalog-snapshot.json` is a byte-for-
//! byte snapshot of the upstream catalog at iter 229. Refresh by
//! re-downloading from the CATALOG_URL in
//! `src/services/mods/catalog.rs` and committing the diff.

use std::fs;
use std::path::Path;

use serde_json::Value;

const CATALOG_FIXTURE: &str = "tests/fixtures/catalog-snapshot.json";

fn load_catalog_mods() -> Vec<Value> {
    let body =
        fs::read_to_string(CATALOG_FIXTURE).unwrap_or_else(|e| panic!("{CATALOG_FIXTURE}: {e}"));
    let v: Value = serde_json::from_str(&body).expect("catalog snapshot must parse as JSON");
    let mods = v["mods"]
        .as_array()
        .expect("catalog must carry a top-level `mods` array")
        .clone();
    assert!(
        !mods.is_empty(),
        "catalog snapshot must carry at least one mod"
    );
    mods
}

fn s<'a>(entry: &'a Value, key: &str) -> &'a str {
    entry[key].as_str().unwrap_or("")
}

fn is_hex_64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Borrowed from `tmm::is_safe_gpk_container_filename` — we
/// duplicate here so this integration test doesn't depend on the
/// crate's internal visibility. If the predicate changes
/// upstream, this test's copy must be updated alongside.
fn is_safe_gpk_container_filename_basename(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 {
        return false;
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return false;
    }
    if name.starts_with('.') || name.ends_with('.') {
        return false;
    }
    // Reject NTFS ADS markers + other separators.
    for ch in name.chars() {
        if ch.is_control() || "<>:\"|?*".contains(ch) {
            return false;
        }
    }
    true
}

fn url_basename(url: &str) -> &str {
    url.rsplit('/').next().unwrap_or("")
}

fn check_base_invariants(entry: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    let id = s(entry, "id");

    if id.is_empty() {
        errors.push("empty id".into());
    }
    if s(entry, "name").is_empty() {
        errors.push(format!("{id}: empty name"));
    }
    if s(entry, "author").is_empty() {
        errors.push(format!("{id}: empty author"));
    }
    if s(entry, "version").is_empty() {
        errors.push(format!("{id}: empty version"));
    }
    let kind = s(entry, "kind");
    if kind != "external" && kind != "gpk" {
        errors.push(format!("{id}: kind must be external|gpk, got {kind:?}"));
    }
    let url = s(entry, "download_url");
    if !url.starts_with("https://") {
        errors.push(format!("{id}: download_url must be https://, got {url:?}"));
    }
    let sha = s(entry, "sha256");
    if !is_hex_64(sha) {
        errors.push(format!(
            "{id}: sha256 must be 64 hex chars, got {:?}",
            sha.len()
        ));
    }
    let size = entry["size_bytes"].as_u64().unwrap_or(0);
    if size == 0 {
        errors.push(format!("{id}: size_bytes must be > 0"));
    }

    errors
}

fn check_gpk_specific(entry: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    let id = s(entry, "id");
    let url = s(entry, "download_url");
    let basename = url_basename(url);

    if basename.is_empty() {
        errors.push(format!("{id}: gpk download_url has no basename"));
    }
    // The URL basename isn't strictly the on-disk container name (that
    // comes from the `.gpk` footer), but if the URL's basename itself
    // would fail the sandbox predicate, the catalog entry is broken.
    if !is_safe_gpk_container_filename_basename(basename) {
        errors.push(format!(
            "{id}: gpk URL basename `{basename}` fails sandbox predicate"
        ));
    }

    errors
}

fn check_external_specific(entry: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    let id = s(entry, "id");
    let relpath = s(entry, "executable_relpath");

    if relpath.is_empty() {
        errors.push(format!("{id}: external executable_relpath is empty"));
        return errors;
    }
    // UNC / absolute / traversal rejection — mirrors the container
    // sandbox. An external executable relpath that escapes the mod's
    // slot directory would let a post-unzip spawn target an arbitrary
    // host-filesystem binary.
    let p = Path::new(relpath);
    if p.is_absolute()
        || relpath.starts_with('/')
        || relpath.starts_with('\\')
        || relpath.starts_with("\\\\")
        || relpath.split(['/', '\\']).any(|part| part == "..")
    {
        errors.push(format!(
            "{id}: external executable_relpath `{relpath}` is not a safe relative path"
        ));
    }

    errors
}

/// PRD §3.3.1 — 101/101 catalog entries must pass every lifecycle
/// predicate gate. A single failure here means the live lifecycle
/// for that entry would also fail — catalog maintainer must fix.
#[test]
fn every_catalog_entry_satisfies_lifecycle_predicate_gates() {
    let mods = load_catalog_mods();

    let mut all_errors: Vec<String> = Vec::new();
    for entry in &mods {
        all_errors.extend(check_base_invariants(entry));
        match s(entry, "kind") {
            "gpk" => all_errors.extend(check_gpk_specific(entry)),
            "external" => all_errors.extend(check_external_specific(entry)),
            _ => { /* base-invariant check already surfaced the bad kind */ }
        }
    }

    assert!(
        all_errors.is_empty(),
        "PRD §3.3.1: {} catalog entr{} failed lifecycle-gate predicates \
         ({}/{} green). Fix in the catalog repo and refresh \
         tests/fixtures/catalog-snapshot.json:\n  - {}",
        all_errors.len(),
        if all_errors.len() == 1 { "y" } else { "ies" },
        mods.len() - all_errors.len(),
        mods.len(),
        all_errors.join("\n  - ")
    );
}

/// PRD §3.3.1: at least 100 entries must be present. The catalog
/// grew to 101 at iter 229; a sudden shrink below 100 indicates a
/// malformed fixture update or a real catalog regression.
#[test]
fn catalog_snapshot_carries_at_least_100_entries() {
    let mods = load_catalog_mods();
    assert!(
        mods.len() >= 100,
        "PRD §3.3.1: catalog snapshot has only {} entries; expected \
         ≥100 (101 at iter 229). Shrink suggests a broken fixture \
         refresh — re-download from CATALOG_URL.",
        mods.len()
    );
}

/// PRD §3.3.1: every id is unique. Duplicate ids cause registry
/// upsert collisions — later entries overwrite earlier ones on
/// install, and the earlier entry is no longer reachable from the
/// catalog grid. This is a catalog-side invariant but the predicate
/// gate catches it cheaply.
#[test]
fn every_catalog_entry_has_a_unique_id() {
    let mods = load_catalog_mods();
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for entry in &mods {
        let id = s(entry, "id").to_string();
        *seen.entry(id).or_insert(0) += 1;
    }
    let dupes: Vec<_> = seen.iter().filter(|(_, n)| **n > 1).collect();
    assert!(
        dupes.is_empty(),
        "PRD §3.3.1: catalog has {} duplicate id(s): {:?}",
        dupes.len(),
        dupes
    );
}

/// PRD §3.3.1: both kinds must be represented in the catalog. The
/// launcher ships UX for both external and gpk; a catalog with only
/// one kind would starve the other UI path and regressions in the
/// unused path would ship undetected.
#[test]
fn catalog_carries_both_external_and_gpk_kinds() {
    let mods = load_catalog_mods();
    let kinds: std::collections::HashSet<&str> = mods.iter().map(|m| s(m, "kind")).collect();
    assert!(
        kinds.contains("external"),
        "PRD §3.3.1: catalog must carry at least one external-kind \
         entry. Kinds found: {kinds:?}"
    );
    assert!(
        kinds.contains("gpk"),
        "PRD §3.3.1: catalog must carry at least one gpk-kind entry. \
         Kinds found: {kinds:?}"
    );
}

/// Guard self-test — the `is_hex_64` + `is_safe_gpk_container_filename_basename`
/// predicates must bite on obvious bad shapes. Without this, a
/// regression in either predicate would silently let a malformed
/// entry through.
#[test]
fn lifecycle_predicate_helpers_self_test() {
    // is_hex_64 positives + negatives.
    assert!(is_hex_64(
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    ));
    assert!(!is_hex_64(""));
    assert!(!is_hex_64("short"));
    assert!(!is_hex_64(
        "g0000000000000000000000000000000000000000000000000000000000000000"
    ));

    // Safe-container predicate positives.
    assert!(is_safe_gpk_container_filename_basename("mymod.gpk"));
    assert!(is_safe_gpk_container_filename_basename("my-mod_v2.gpk"));

    // Safe-container predicate negatives (each is a distinct attack vector).
    assert!(!is_safe_gpk_container_filename_basename(""));
    assert!(!is_safe_gpk_container_filename_basename("../evil"));
    assert!(!is_safe_gpk_container_filename_basename("a/b.gpk"));
    assert!(!is_safe_gpk_container_filename_basename("a\\b.gpk"));
    assert!(!is_safe_gpk_container_filename_basename("..hidden"));
    assert!(!is_safe_gpk_container_filename_basename("evil:stream"));
}
