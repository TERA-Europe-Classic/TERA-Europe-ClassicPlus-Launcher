# Research sweep — iter 210

Date: 2026-04-20
Previous sweep: iter 190 (`sweep-iter-190.md`)
Previous revalidation: iter 200 (`revalidation-iter-200.md`) — all-gates-green
Worktree commit at start of iter 210: `b8d45c2`

## Headline

**Zero new advisories, zero dep drift, zero regressions across 20
iterations.** Rust test count 1188 (iter 190) → 1278 (iter 210) = **+90** —
same +5/WORK cadence as iter 161-179 and 181-199. Advisory total
holds at 19 allowed warnings. Both documented ignores (rand 0.9.2,
bytes 1.11.0) still in force — neither retired since iter 190.

## Part A — Advisory cadence

### A.1 `cargo audit` totals

```
$ cargo audit --ignore RUSTSEC-2026-0097 --ignore RUSTSEC-2026-0007
warning: 19 allowed warnings found
```

Exit 0. Same 19 upstream-locked warnings as iter 170 / 180 / 190.

| Advisory | Exit criterion | Status @ iter 210 |
|---|---|---|
| RUSTSEC-2026-0097 (rand 0.9.2) | tauri-plugin-notification bumps rand ≥ 0.10 | not yet |
| RUSTSEC-2026-0007 (bytes 1.11.0) | tower-http / tokio-util / reqwest chain bumps bytes ≥ 1.11.1 | not yet |

### A.2 Allowed-warning inventory (unchanged)

The 19 warnings are the same transitive upstream-locked
unmaintained-crate notices pinned in iter 170:

- **gtk-rs GTK3 bindings** (atk, atk-sys, gdk, gdk-sys, gdkwayland-sys,
  gdkx11, gdkx11-sys, gtk, gtk-sys, gtk3-macros, pango, pango-sys,
  cairo-rs, cairo-sys-rs, gdk-pixbuf, gdk-pixbuf-sys, glib-sys,
  gobject-sys) — RUSTSEC-2024-0411..0418 no-longer-maintained. Linux
  WebKit2GTK transitive. Windows builds unaffected.
- **fxhash 0.2.1** — RUSTSEC-2025-0057 no-longer-maintained. Transitive
  via tauri → raw-window-handle chain.

No new advisories in the iter 190-210 window against any direct or
transitive dep of `teralaunch/src-tauri` or `teralib`.

## Part B — Dep drift

### B.1 `Cargo.toml` / `Cargo.lock` delta since iter 190

```
$ git log --oneline main..tauri-v2-migration -- \
    teralaunch/src-tauri/Cargo.toml teralaunch/src-tauri/Cargo.lock
```

Cargo.lock was touched once in the iter 190-210 window:

- `935dd9b` (iter 202) — Cargo.lock side-effect from `cargo test`
  build-graph refresh. No version-number changes; just metadata
  updates. (Confirmed by `git diff` showing only checksum and
  dependency-graph adjustments, no `version = "..."` edits.)

Cargo.toml: zero changes in the window. Last real version bump was
`b17ab33` (time 0.3.45 → 0.3.47) and `e52cad4` (rustls-webpki
0.103.9 → 0.103.12) — both pre-iter-190.

No major-version bumps, no added deps, no removed deps.

### B.2 Direct-dep version pin review

Spot check of key security-relevant dep version lines in `Cargo.toml`:

| Dep | Version | Status |
|---|---|---|
| tauri | `2` | Stable; pinned major |
| tauri-plugin-shell | `2` | Stable; pinned major (2.3.5 via lock) |
| tauri-plugin-updater | `2` | Stable; pinned major |
| tauri-plugin-http | `2` | Stable; pinned major |
| reqwest | `0.12` | Stable; pinned minor |
| zeroize | `1` with `zeroize_derive` feature | Stable |

No drift vs iter 190 notes.

## Part C — Iteration-window summary (iter 190-210)

| Window | Iters | Work type | Tests added | File touched |
|---|---|---|---|---|
| 191-199 | 9 | WORK | +45 | 9 tests/*.rs extended (small-baseline revisit) |
| 200 | 1 | REVALIDATION | 0 | revalidation-iter-200.md (N%20=0, 200-iter milestone) |
| 201-209 | 9 | WORK | +45 | 9 tests/*.rs extended (≥ 10 pins milestone + 11-count sweep) |
| 210 | 1 | RESEARCH SWEEP | 0 | sweep-iter-210.md (this doc, N%10=0) |

Net: **+90 Rust tests, zero source-code changes, zero advisory
changes, zero dep version changes** across 20 iterations.

## Part D — Tauri 2.x ecosystem check

No new CVE-class advisories against Tauri 2.x in the 2026-04-01 to
2026-04-20 window (checked via cargo audit + RustSec database lag
indicator). CVE-2025-31477 defence chain (iter 86, 148, 188, 206)
remains the last major Tauri-specific regression — shell-plugin is
current at 2.3.5, well above the 2.2.1 fix floor.

Tauri-plugin-updater has not shipped a new advisory since iter 190;
the M7 updater-downgrade refusal (3.1.9, commit `9898af0`) remains
authoritative.

## Part E — Outstanding backlog (unchanged since iter 190)

- RUSTSEC-2026-0097 (rand) — upstream-gated; no upstream activity
- RUSTSEC-2026-0007 (bytes) — upstream-gated; no upstream activity
- §3.3.1 `every_catalog_entry_lifecycle.rs` — genuinely unshipped
- §3.8.7 `audits/units/` — genuinely unshipped
- C# pins (TCC / Shinra hardening) — documented-deferred

**Zero new actionable items surface from this sweep.**

## Summary

Iter 210 research sweep confirms the worktree remains in a stable,
advisory-clean state. Advisory total (19), ignore set (2), and dep
versions all identical to iter 190. +90 Rust tests delivered via
19 `test(...)` commits, all pure additive pins. No upstream movement
on the two open-ignore exit criteria.

Next research sweep: iter 220. Next revalidation: iter 220 (N%20=0).
`ready_for_squash_merge: true` unchanged — status remains user-gated
per standing policy.
