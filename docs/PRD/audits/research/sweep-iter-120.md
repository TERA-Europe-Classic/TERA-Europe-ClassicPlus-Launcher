# Research sweep + revalidation — iter 120 (DOUBLE-DUTY)

Date: 2026-04-19
Previous sweep: iter 110 (`sweep-iter-110.md`)
Previous revalidation: iter 100 (all-gates-green)
Worktree commit at start of iter 120: `bc89fb3`

## Headline

**Zero new advisories, zero new dep drift, zero test regressions.**

The iter-112 `--ignore RUSTSEC-2026-0097` (rand) and
`--ignore RUSTSEC-2026-0007` (bytes) flags still stand — neither exit
criterion fired (tauri-plugin-notification hasn't bumped past rand 0.9.2;
the tower-http / tokio-util / reqwest chain hasn't bumped bytes to
>= 1.11.1). Confirmed via fresh `cargo audit` run on both workspaces.

Iter 100's audit doc stands as the last full revalidation narrative;
iter 120 reaffirms every claim.

## Part A — Research sweep

### A.1 cargo tree -d delta vs iter 110

| Crate | Versions iter 110 | Versions iter 120 | Delta |
|---|---|---|---|
| reqwest | 0.12.28 + 0.13.2 | 0.12.28 + 0.13.2 | unchanged |
| cookie | 0.16.2 + 0.18.1 | 0.16.2 + 0.18.1 | unchanged |
| cookie_store | 0.21.1 + 0.22.0 | 0.21.1 + 0.22.0 | unchanged |
| env_logger | 0.10.2 + 0.11.8 | 0.10.2 + 0.11.8 | unchanged |
| bitflags | 1.3.2 + 2.10.0 | 1.3.2 + 2.10.0 | unchanged |
| getrandom | 0.1.16 + 0.2.17 + 0.3.4 | 0.1.16 + 0.2.17 + 0.3.4 | unchanged |
| hashbrown | 0.12.3 + 0.14.5 + 0.16.1 | 0.12.3 + 0.14.5 + 0.16.1 | unchanged |
| rustls-webpki | 0.103.12 | 0.103.12 | unchanged |
| time | 0.3.47 | 0.3.47 | unchanged |

No new dups. Same upstream-gated state as iter 110. The reqwest 0.12/0.13
deferral from iter 87 continues to hold.

### A.2 cargo audit — teralaunch/src-tauri

With iter-112 ignores applied:

```
$ cargo audit --ignore RUSTSEC-2026-0097 --ignore RUSTSEC-2026-0007
Scanning Cargo.lock for vulnerabilities (662 crate dependencies)
warning: 19 allowed warnings found
```

Exit 0. Same 19 upstream-locked warnings as iter 112 (gtk/gdk/atk
webview chain, unic-* transitives, proc-macro-error, fxhash,
number_prefix).

**Exit criteria check for the two ignored advisories:**

| Advisory | Exit criterion | Status @ iter 120 |
|---|---|---|
| RUSTSEC-2026-0097 (rand 0.9.2) | tauri-plugin-notification bumps rand >= 0.10 | not yet — tauri-plugin-notification 2.3.3 still on rand 0.9.2 |
| RUSTSEC-2026-0007 (bytes 1.11.0) | tower-http / tokio-util / reqwest chain bumps bytes to >= 1.11.1 | not yet — bytes 1.11.0 still the resolved version |

Neither ignore can be retired this sweep. Both retained.

### A.3 cargo audit — teralib

```
Scanning Cargo.lock for vulnerabilities (233 crate dependencies)
```

Zero findings. Unchanged from iter 111 (after dotenv drop).

### A.4 Upstream release notes delta since iter 110

| Package | Our pin | Delta |
|---|---|---|
| tauri | 2.10.3 | no 2.11 yet |
| tauri-plugin-notification | 2.3.3 | unchanged — still pulls rand 0.9.2 |
| tauri-plugin-http | 2.5.8 | unchanged — still on reqwest 0.12 |
| tauri-plugin-updater | 2.10.1 | unchanged — still alone on reqwest 0.13 / zip 4 |
| reqwest | 0.12.28 | unchanged |
| rustls | via 0.103.12 | unchanged |
| zip | 4.x | unchanged |

The Rust / Tauri ecosystem remained quiet in the iter 110-120 window.
Consistent with the crates.io pace observed in iters 90-110.

### A.5 No new P-slot candidates surfaced

Iter 120's sweep discovered nothing actionable. Outstanding backlog
from iter 110:
- P2 `dep.rand-advisory-ignore-2026-0097` — DONE at iter 112
- P2 `dep.teralib-dotenv-to-dotenvy` — DONE at iter 111 (as dotenv drop)
- P3 `infra.cargo-audit-tuning` — absorbed at iter 112

Backlog is clean. The loop has consumed every iter-110 item.

## Part B — Revalidation

### B.1 Full test + lint suite

| Suite | iter 100 | iter 120 | Delta |
|---|---|---|---|
| Rust tests | 860/860 (18 binaries) | 899/899 (28 binaries) | **+39 tests, +10 binaries** |
| JS tests | 449/449 (13 files) | 449/449 (13 files) | unchanged |
| Clippy `-D warnings` | clean | clean | unchanged |
| teralaunch cargo audit (w/ ignores) | clean | clean | unchanged |
| teralib cargo audit | clean (post-iter-111) | clean | unchanged |

Rust test-count delta breakdown across iters 101-119:
- iter 104 `crate_comment_guard` +2
- iter 106 `architecture_doc_guard` +3
- iter 107 `claude_md_guard` +3
- iter 108 `lessons_learned_guard` +4
- iter 109 `changelog_guard` +3
- iter 113 `add_mod_from_file_wiring` +6
- iter 114 `secret_scan_guard` +4
- iter 115 `deploy_scope_infra_guard` +4
- iter 116 drift-guard PINS extension (body-only, +0)
- iter 117 drift-guard PINS extension (body-only, +0)
- iter 118 `anti_reverse_guard` +7
- iter 119 `portal_https_guard` +3

Total: +39 (matches 860 → 899 delta exactly).

### B.2 Regression scan

`git log main..tauri-v2-migration --oneline | wc -l` = 60 commits
since divergence (was ~40 at iter 100).

Scanned for regression patterns (`regress`, `revert`, `bug`, `broke`)
— zero matches. All 60 commits are additive: test pin additions,
structural guards, drift fixes, advisory-clearing dep tweaks.

### B.3 Structural guard inventory (iters 86-119)

13 integration-test drift-guard files shipped pre-iter-120:

| File | Iter | Coverage |
|---|---|---|
| `shell_scope_pinned.rs` | 86 | PRD plugins.shell.open pin |
| `tampered_catalog.rs` | 96 | §5.3 adv.tampered-catalog wiring |
| `prd_path_drift_guard.rs` | 97 + extensions | 35 PRD §3 path pins |
| `crate_comment_guard.rs` | 104 | §3.8.2 crate-level //! comment |
| `architecture_doc_guard.rs` | 106 | §3.8.4 per-subsystem sections |
| `claude_md_guard.rs` | 107 | §3.8.1 Mod Manager section |
| `lessons_learned_guard.rs` | 108 | §3.8.8 200-line cap + archive |
| `changelog_guard.rs` | 109 | §3.8.5 no conv-commit prefixes |
| `add_mod_from_file_wiring.rs` | 113 | §3.3.4 5-wire Rust-side |
| `secret_scan_guard.rs` | 114 | §3.1.6 gitleaks workflow + config |
| `deploy_scope_infra_guard.rs` | 115 | §3.1.14 scope-gate ordering |
| `anti_reverse_guard.rs` | 118 | §3.1.8 LTO/strip/CFG/obfuscation |
| `portal_https_guard.rs` | 119 | §3.1.13 config URL shape |

Every guard is running and passing.

### B.4 Status: **all-gates-green**

Zero REGRESSED items. Zero new drift. All iter-100 pinned invariants
still pass spot-check. All iters 104-119 drift guards active.

## Summary

Double-duty iter 120 confirms the worktree is in a stable, advisory-
clean (modulo documented ignores), regression-free state. Total
delta since iter 100: +39 Rust tests, +10 test binaries, 20 commits,
zero regressions. The squash merge remains user-gated but the state
is demonstrably ready.

Net iter-120 risk delta: **zero**. No new P-slot items surfaced.
`ready_for_squash_merge: true` status unchanged.
