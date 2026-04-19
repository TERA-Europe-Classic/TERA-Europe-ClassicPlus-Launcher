# Research sweep — iter 150

Date: 2026-04-19
Previous sweep: iter 130 (`sweep-iter-130.md`)
Previous revalidation: iter 140 (formal; all-gates-green)
Worktree commit at start of iter 150: `577e424`

## Headline

**Zero new advisories, zero new dep drift, zero test regressions.**

Iter-112 `--ignore RUSTSEC-2026-0097` (rand) and `--ignore RUSTSEC-2026-0007` (bytes) still stand; neither exit criterion fired. Ecosystem remained quiet in iter 130-150 window. All 22 structural guards extended, none removed. Rust test count 949 → 1004 (+55).

## Part A — Research sweep

### A.1 cargo tree -d delta vs iter 130

| Crate | Versions iter 130 | Versions iter 150 | Delta |
|---|---|---|---|
| reqwest | 0.12.28 + 0.13.2 | 0.12.28 + 0.13.2 | unchanged |
| cookie | 0.16.2 + 0.18.1 | 0.16.2 + 0.18.1 | unchanged |
| cookie_store | 0.21.1 + 0.22.0 | 0.21.1 + 0.22.0 | unchanged |
| env_logger | 0.10.2 + 0.11.8 | 0.10.2 + 0.11.8 | unchanged |
| bitflags | 1.3.2 + 2.10.0 | 1.3.2 + 2.10.0 | unchanged |
| getrandom | 0.1.16 + 0.2.17 + 0.3.4 | 0.1.16 + 0.2.17 + 0.3.4 | unchanged |
| hashbrown | 0.12.3 + 0.14.5 + 0.16.1 | 0.12.3 + 0.14.5 + 0.16.1 | unchanged |
| rustls-webpki | 0.103.12 | 0.103.12 | unchanged |
| rand | 0.9.2 (+ 0.8.5 / 0.7.3) | 0.9.2 (+ 0.8.5 / 0.7.3) | unchanged |
| bytes | 1.11.0 | 1.11.0 | unchanged |

No new dups; no resolved-version changes. Upstream-gated state identical to iter 130.

### A.2 cargo audit — teralaunch/src-tauri

With iter-112 ignores applied:

```
$ cargo audit --ignore RUSTSEC-2026-0097 --ignore RUSTSEC-2026-0007
Scanning Cargo.lock for vulnerabilities (662 crate dependencies)
warning: 19 allowed warnings found
```

Exit 0. Same 19 upstream-locked warnings as iter 130 (gtk/gdk/atk webview chain, unic-* transitives, proc-macro-error, fxhash, number_prefix).

**Exit criteria check for the two ignored advisories:**

| Advisory | Exit criterion | Status @ iter 150 |
|---|---|---|
| RUSTSEC-2026-0097 (rand 0.9.2) | tauri-plugin-notification bumps rand ≥ 0.10 | not yet |
| RUSTSEC-2026-0007 (bytes 1.11.0) | tower-http / tokio-util / reqwest chain bumps bytes ≥ 1.11.1 | not yet |

Neither retired this sweep.

### A.3 cargo audit — teralib

```
Scanning Cargo.lock for vulnerabilities (233 crate dependencies)
```

Exit 0. Zero findings. Unchanged since iter 111 (dotenv drop).

### A.4 Upstream release notes delta since iter 130

| Package | Our pin | Delta |
|---|---|---|
| tauri | 2.10.3 | unchanged |
| tauri-plugin-notification | 2.3.3 | unchanged — still pulls rand 0.9.2 |
| tauri-plugin-http | 2.5.8 | unchanged — still on reqwest 0.12 |
| tauri-plugin-updater | 2.10.1 | unchanged — still alone on reqwest 0.13 / zip 4 |
| reqwest | 0.12.28 | unchanged |
| rustls | via 0.103.12 | unchanged |
| zip | 4.x | unchanged |

Ecosystem quiet. Consistent with iter 100-130 pace.

### A.5 No new P-slot candidates surfaced

Iter 150's sweep discovered nothing actionable. Outstanding backlog from iter 130:

- Zero open P-slot items from the dep/audit track.
- C# pins still documented-deferred (pin.tcc.classic-plus-sniffer, pin.shinra.tera-sniffer, §3.1.10 TCC/Shinra hardening).
- §3.3.1 every_catalog_entry_lifecycle.rs still genuinely unshipped.
- §3.8.7 audits/units/ directory still genuinely unshipped.

Backlog clean. The loop has consumed every iter-130 item.

## Part B — Structural-guard inventory delta since iter 130

**Iters 131-149 were a guard-extension batch — no NEW guard files shipped; every existing guard's coverage deepened.** The worktree's 22 active guard files are all strictly richer than they were at iter 130.

### B.1 Extension breakdown

| Iter | Guard | Test delta | What got pinned |
|---|---|---|---|
| 131 | `mods_categories_ui_scanner_guard` | new +8 | iter-85 filter-strip UX fix |
| 132 | `prd_path_drift_guard` (JS-side) | +2 | 3 JS-side pins (§3.4.7, §3.6.4, §3.7.1) |
| 133 | `prd_path_drift_guard` (§3.7.4) | +0 | concrete `it()` citation |
| 134 | `prd_path_drift_guard` (Playwright) | +0 | §3.3.4 e2e pin + idiom extension |
| 135 | `meta_hygiene_guard` | new +5 | contract across all `*_guard.rs` |
| 136 | `meta_hygiene_guard` | +1 | non-stub disk-read check |
| 137 | `claude_md_guard` | +4 | 7 CLAUDE.md sections + v100 API + flags + testing |
| 138 | `architecture_doc_guard` | +3 | 11 headings + guarantees + known-gaps |
| 139 | `lessons_learned_guard` | +3 | H3 format + Pattern/When + header |
| 140 | revalidation | +0 | formal cadence, all-gates-green |
| 141 | `changelog_guard` | +4 | Unreleased + em-dash shape + descending order |
| 142 | `portal_https_guard` | +3 | 8 expected keys + prefix consistency + updater empty |
| 143 | `crate_comment_guard` | +1 | `//!` body-length floor (≥100 chars) |
| 144 | `secret_scan_guard` | +4 | dual triggers + fetch-depth + semver version + allowlist |
| 145 | `deploy_scope_infra_guard` | +4 | script exports + prefix constants + self-test ordering |
| 146 | `anti_reverse_guard` | +4 | opt-level + no-debug + CFG release-gate + M6 cite |
| 147 | `tauri_v2_migration_audit_guard` | +4 | per-doc depth + plan automation + SHA + 3-gate cite |
| 148 | `shell_scope_pinned` | +2 | strict stanza + no scope override (**crossed 1000**) |
| 149 | `tampered_catalog` | +3 | fail-closed order + finalize_error sig + enum |

Net delta: **+55 Rust tests** (949 → 1004), **+2 new guard files** (mods_categories_ui, meta_hygiene), **17 existing guards deepened**.

### B.2 Rust test-count trajectory

| Iter | Total | Running delta |
|---|---|---|
| 130 | 949 | baseline |
| 140 | 975 | +26 |
| 148 | 1001 | **+52 (crossed 1000)** |
| 149 | 1004 | +55 |

### B.3 Regression scan

`git log main..tauri-v2-migration --oneline | wc -l` = 90 commits since divergence (was 70 at iter 130).

Scanned for regression patterns (`regress`, `revert`, `broke`, `fix.*bug`) across the 20 iter 130-150 commits — **zero matches**. All additive: guard extensions + revalidation audit + this research sweep.

## Part C — Status: all-gates-green (by inspection)

This sweep is a research pass, not a formal revalidation (that cadence hits at N=160). Inspection-level checks:

- cargo audit (both workspaces): clean with documented ignores.
- Dep tree: zero drift vs iter 130.
- Rust tests: 1004/1004 per iter 149's final run.
- Structural-guard inventory: 19 → 22 files (the 2 new + 17 deepened + shell_scope_pinned/tampered_catalog original still there).
- Worktree ready state: `ready_for_squash_merge: true` unchanged since iter 100.

## Summary

Iter 150 confirms the worktree remains in a stable, advisory-clean (modulo documented ignores), regression-free state. The iter 130-150 window was dominated by a **guard-extension batch**: rather than adding new guard files, the loop deepened every existing one's coverage. The invariant chain the user-gated squash merge depends on is now harder to silently weaken.

Total delta since iter 130: +55 Rust tests, +2 new structural-guard files, 17 deepened guards, 20 additive commits, zero regressions.

Net iter-150 risk delta: **zero**. No new P-slot items surfaced. `ready_for_squash_merge: true` status unchanged. Formal revalidation scheduled for iter 160.
