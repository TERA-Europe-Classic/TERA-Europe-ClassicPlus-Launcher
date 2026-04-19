# Research sweep — iter 130

Date: 2026-04-19
Previous sweep: iter 120 (`sweep-iter-120.md`)
Previous revalidation: iter 120 (all-gates-green, double-duty)
Worktree commit at start of iter 130: `adaf2cc`

## Headline

**Zero new advisories, zero new dep drift, zero test regressions.**

The iter-112 `--ignore RUSTSEC-2026-0097` (rand) and
`--ignore RUSTSEC-2026-0007` (bytes) flags still stand. Neither exit
criterion fired — tauri-plugin-notification hasn't bumped past rand
0.9.2; the tower-http / tokio-util / reqwest chain hasn't bumped
bytes to ≥ 1.11.1. Confirmed via fresh `cargo audit` runs on both
workspaces.

The iter-110/120 baseline stands. Iter 130 reaffirms every claim.

## Part A — Research sweep

### A.1 cargo tree -d delta vs iter 120

| Crate | Versions iter 120 | Versions iter 130 | Delta |
|---|---|---|---|
| reqwest | 0.12.28 + 0.13.2 | 0.12.28 + 0.13.2 | unchanged |
| cookie | 0.16.2 + 0.18.1 | 0.16.2 + 0.18.1 | unchanged |
| cookie_store | 0.21.1 + 0.22.0 | 0.21.1 + 0.22.0 | unchanged |
| env_logger | 0.10.2 + 0.11.8 | 0.10.2 + 0.11.8 | unchanged |
| bitflags | 1.3.2 + 2.10.0 | 1.3.2 + 2.10.0 | unchanged |
| getrandom | 0.1.16 + 0.2.17 + 0.3.4 | 0.1.16 + 0.2.17 + 0.3.4 | unchanged |
| hashbrown | 0.12.3 + 0.14.5 + 0.16.1 | 0.12.3 + 0.14.5 + 0.16.1 | unchanged |
| rustls-webpki | 0.103.12 | 0.103.12 | unchanged |
| rand | 0.9.2 (+ 0.8.5, 0.7.3) | 0.9.2 (+ 0.8.5, 0.7.3) | unchanged |
| bytes | 1.11.0 | 1.11.0 | unchanged |

No new dups. Same upstream-gated state as iter 120. The reqwest
0.12/0.13 deferral from iter 87 continues to hold.

### A.2 cargo audit — teralaunch/src-tauri

With iter-112 ignores applied:

```
$ cargo audit --ignore RUSTSEC-2026-0097 --ignore RUSTSEC-2026-0007
Scanning Cargo.lock for vulnerabilities (662 crate dependencies)
warning: 19 allowed warnings found
```

Exit 0. Same 19 upstream-locked warnings as iter 120 (gtk/gdk/atk
webview chain, unic-* transitives, proc-macro-error, fxhash,
number_prefix).

**Exit criteria check for the two ignored advisories:**

| Advisory | Exit criterion | Status @ iter 130 |
|---|---|---|
| RUSTSEC-2026-0097 (rand 0.9.2) | tauri-plugin-notification bumps rand ≥ 0.10 | not yet — rand 0.9.2 still resolved |
| RUSTSEC-2026-0007 (bytes 1.11.0) | tower-http / tokio-util / reqwest chain bumps bytes to ≥ 1.11.1 | not yet — bytes 1.11.0 still resolved |

Neither ignore can be retired this sweep. Both retained.

### A.3 cargo audit — teralib

```
Scanning Cargo.lock for vulnerabilities (233 crate dependencies)
```

Exit 0. Zero findings. Unchanged from iter 120 (post-iter-111 dotenv
drop).

### A.4 Upstream release notes delta since iter 120

| Package | Our pin | Delta |
|---|---|---|
| tauri | 2.10.3 | no 2.11 yet |
| tauri-plugin-notification | 2.3.3 | unchanged — still pulls rand 0.9.2 |
| tauri-plugin-http | 2.5.8 | unchanged — still on reqwest 0.12 |
| tauri-plugin-updater | 2.10.1 | unchanged — still alone on reqwest 0.13 / zip 4 |
| reqwest | 0.12.28 | unchanged |
| rustls | via 0.103.12 | unchanged |
| zip | 4.x | unchanged |

The Rust / Tauri ecosystem remained quiet in the iter 120-130
window. Consistent with the crates.io pace observed across
iters 90-120.

### A.5 No new P-slot candidates surfaced

Iter 130's sweep discovered nothing actionable. Outstanding backlog
from iter 120:
- Zero open P-slot items from the dep/audit track.
- C# pins still documented-deferred (pin.tcc.classic-plus-sniffer,
  pin.shinra.tera-sniffer, §3.1.10 TCC/Shinra hardening).
- §3.3.1 every_catalog_entry_lifecycle.rs still genuinely unshipped.
- §3.8.7 audits/units/ directory still genuinely unshipped.

Backlog is clean on the track this sweep covers. The loop has
consumed every iter-120 item.

## Part B — Structural-guard inventory delta since iter 120

Iters 121-129 shipped **6 new guard files** (plus 2 drift-guard
extensions that didn't add files):

| File | Iter | Coverage |
|---|---|---|
| — (body-only ext) | 121 | §3.2.7 + §3.2.10 drift-guard extensions (35 → 37 pins) |
| `tauri_v2_migration_audit_guard.rs` | 122 | M0-M8 audit-doc quartet presence |
| — (body-only ext) | 123 | drift-guard inventory sweep (37 → 41 pins) |
| `i18n_no_hardcoded_guard.rs` | 124 | PRD §3.7.4 scanner structural pin |
| `i18n_scanner_guard.rs` | 125 | PRD §3.4.7 jargon + §3.7.1 parity scanner pins (batched) |
| `shell_open_callsite_guard.rs` | 126 | PRD §3.1.5 CVE-2025-31477 call-site scanner |
| `search_perf_guard.rs` | 127 | PRD §3.6.4 search-one-frame perf-bench integrity |
| `classicplus_guards_scanner_guard.rs` | 128 | Classic+ disabled-features contract scanner |
| `offline_banner_scanner_guard.rs` | 129 | iter-84 fix.offline-empty-state scanner |

**Running total:** 13 pre-iter-120 guard files + 6 new = **19
structural-guard integration-test files** active in the worktree.

### B.1 Rust test-count trajectory

| Iter | Total Rust tests | Delta |
|---|---|---|
| 100 | 860 | baseline |
| 120 | 899 | +39 (iters 104-119) |
| 122 | 903 | +4 (tauri-v2 quartet) |
| 123 | 903 | +0 (body-only extension) |
| 124 | 911 | +8 (i18n_no_hardcoded_guard) |
| 125 | 921 | +10 (i18n_scanner_guard — jargon+parity batched) |
| 126 | 928 | +7 (shell_open_callsite_guard) |
| 127 | 935 | +7 (search_perf_guard) |
| 128 | 942 | +7 (classicplus_guards_scanner_guard) |
| 129 | 949 | +7 (offline_banner_scanner_guard) |

Net iter 120 → 130: **+50 Rust tests** (899 → 949). JS tests
stable at 449/449.

### B.2 Regression scan

`git log main..tauri-v2-migration --oneline | wc -l` = 70 commits
since divergence (was 60 at iter 120).

Scanned for regression patterns (`regress`, `revert`, `bug`,
`broke`) across the iter 120-130 delta — zero matches. All 10
commits are additive: structural guards, research-sweep docs, and
the drift-guard inventory extension.

## Part C — Status: all-gates-green (by inspection)

This sweep is a research pass, not a formal revalidation (that
cadence hits at N=140). But inspection-level checks:

- cargo audit (both workspaces): clean with documented ignores.
- Dep tree: zero drift vs iter 120.
- Structural-guard inventory: 13 → 19 files, all running and
  passing per iter 129's final `cargo test` run (949/949).
- Worktree ready state: `ready_for_squash_merge: true` since iter
  100, unchanged.

## Summary

Iter 130 confirms the worktree remains in a stable, advisory-clean
(modulo documented ignores), regression-free state. Total delta
since iter 120: +50 Rust tests, +6 new structural-guard files, 10
additive commits, zero regressions. The Rust / Tauri ecosystem
remained quiet; no exit criteria fired.

Net iter-130 risk delta: **zero**. No new P-slot items surfaced.
`ready_for_squash_merge: true` status unchanged. Formal
revalidation scheduled for iter 140.
