# GPK Catalog Audit Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a repeatable audit pipeline for every catalog GPK mod so v100.02-safe installs, structural rebuilds, and republished x64 versions are tracked explicitly.

**Architecture:** Start with a pure Rust catalog classifier that needs no game files and cannot mutate CookedPC. A small CLI reads `external-mod-catalog/catalog.json`, emits a Markdown report, and marks each GPK as x64 candidate, x32 rebuild-required, or known structural blocker. Later phases can attach downloaded package headers and vanilla diff summaries to the same row model.

**Tech Stack:** Rust 2021, audit-local serde catalog structs, `serde_json`, Cargo tests.

---

## File Structure

| Path | Status | Responsibility |
|---|---|---|
| `teralaunch/src-tauri/src/services/mods/catalog_audit.rs` | new | Pure classification of catalog GPK entries into migration buckets, package hints, publish policy, and smoke-test text. |
| `teralaunch/src-tauri/src/services/mods/mod.rs` | modify | Export `catalog_audit` for tests and CLI reuse. |
| `teralaunch/src-tauri/src/bin/gpk-catalog-audit.rs` | new | CLI: read catalog JSON, run classifier, write Markdown report. |
| `docs/mod-manager/audits/gpk-catalog-audit.md` | generated | First report from the current sibling `external-mod-catalog/catalog.json`. |

---

## Task 1: Pure catalog audit classifier

**Files:**
- Create: `teralaunch/src-tauri/src/services/mods/catalog_audit.rs`
- Modify: `teralaunch/src-tauri/src/services/mods/mod.rs`

- [x] **Step 1: Write failing unit tests**

Add tests in `catalog_audit.rs` that prove:

```rust
#[test]
fn marks_v32_mods_as_publish_x64_rebuild_required() {
    let entry = gpk_entry("pantypon.pink-chat-window", "v32.04", "https://example.com/S1UI_Chat2.gpk");
    let row = audit_entry(&entry).expect("gpk row");
    assert_eq!(row.arch, AuditArch::X32);
    assert_eq!(row.migration_status, MigrationStatus::PublishX64RebuildRequired);
    assert!(row.notes.iter().any(|note| note.contains("cannot be loaded directly")));
}

#[test]
fn recognizes_flight_gauge_structural_blocker() {
    let entry = gpk_entry(
        "foglio1024.ui-remover-flight-gauge",
        "v100.02",
        "https://raw.githubusercontent.com/foglio1024/UI-Remover/master/remove_FlightGauge/S1UI_ProgressBar.gpk",
    );
    let row = audit_entry(&entry).expect("gpk row");
    assert_eq!(row.package_hints, vec!["S1UI_ProgressBar.gpk"]);
    assert_eq!(row.migration_status, MigrationStatus::StructuralManifestRequired);
    assert!(row.required_operations.iter().any(|op| op.contains("ObjectRedirector -> Texture2D")));
}
```

- [x] **Step 2: Run the tests and verify RED**

Run:

```bash
cd teralaunch/src-tauri && cargo test catalog_audit
```

Expected: compile failure because `audit_entry`, `AuditArch`, and `MigrationStatus` do not exist yet.

- [x] **Step 3: Implement minimal classifier**

Add `AuditRow`, `AuditArch`, `MigrationStatus`, `audit_catalog`, `audit_entry`, package-hint extraction, and known blocker matching. Keep it metadata-only: no HTTP downloads and no filesystem writes.

- [x] **Step 4: Run unit tests and verify GREEN**

Run:

```bash
cd teralaunch/src-tauri && cargo test catalog_audit
```

Expected: all `catalog_audit` tests pass.

---

## Task 2: CLI report generator

**Files:**
- Create: `teralaunch/src-tauri/src/bin/gpk-catalog-audit.rs`
- Modify: `teralaunch/src-tauri/src/services/mods/catalog_audit.rs`

- [x] **Step 1: Write failing CLI integration test**

Create `teralaunch/src-tauri/tests/gpk_catalog_audit_cli.rs` with a temp catalog containing one x32 and one Flight Gauge entry. Assert the CLI exits 0 and writes a Markdown report with both statuses.

- [x] **Step 2: Run and verify RED**

Run:

```bash
cd teralaunch/src-tauri && cargo test --test gpk_catalog_audit_cli
```

Expected: compile/runtime failure because `gpk-catalog-audit` does not exist yet.

- [x] **Step 3: Implement CLI**

Support:

```bash
gpk-catalog-audit --catalog <catalog.json> --out <report.md>
```

The report must include summary counts, a status table, and per-row smoke-test text. Invalid arguments exit 1 with usage text.

- [x] **Step 4: Run CLI test and unit tests**

Run:

```bash
cd teralaunch/src-tauri && cargo test catalog_audit && cargo test --test gpk_catalog_audit_cli
```

Expected: both pass.

---

## Task 3: Generate the first current-catalog report

**Files:**
- Create/update: `docs/mod-manager/audits/gpk-catalog-audit.md`

- [x] **Step 1: Run the CLI against the sibling catalog checkout**

Run:

```bash
cd teralaunch/src-tauri && cargo run --bin gpk-catalog-audit -- --catalog "../../external-mod-catalog/catalog.json" --out "../../TERA-Europe-ClassicPlus-Launcher/docs/mod-manager/audits/gpk-catalog-audit.md"
```

Expected: report generated with 166 GPK rows.

- [x] **Step 2: Inspect report summary**

Verify it calls out the 40 x32 rebuild-required rows, Flight Gauge structural blocker, and missing `gpk_files` rows.

- [x] **Step 3: Run verification**

Run:

```bash
cd teralaunch/src-tauri && cargo test catalog_audit && cargo test --test gpk_catalog_audit_cli
```

Expected: pass.

---

## Next Phase After This Plan

Use the generated report to prioritize binary package diff auditing:

1. Download/cache each catalog GPK by SHA.
2. Read GPK header/file version.
3. Resolve v100.02 vanilla package from current client.
4. Run package diff and attach required operations.
5. For x32 rows, rebuild/re-export x64 packages and publish new catalog artifacts under TERA-Europe-Classic ownership.

## Self-Review

- Spec coverage: covers audit matrix, publish-new-x64 decision, Flight Gauge structural blocker, and first generated report.
- Placeholder scan: no placeholder steps; later binary diffing is explicitly scoped as the next phase, not a missing step.
- Type consistency: task names use the same `AuditRow`, `AuditArch`, and `MigrationStatus` terms throughout.
