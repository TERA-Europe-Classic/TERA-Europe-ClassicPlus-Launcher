# Research sweep — iter 110

Date: 2026-04-19
Previous sweep: iter 100 (`sweep-iter-100.md`)
Worktree commit at start of iter 110: `609d659`

## Headline

**Two real advisories surfaced:**

1. **RUSTSEC-2026-0097** — `rand 0.9.2` unsound when used with a custom logger via `rand::rng()`. Landed **2026-04-09** (post iter-100 sweep). Pulled in by `tauri-plugin-notification 2.3.3`, `chamox 0.1.4`, and `quinn-proto 0.11.14` via `reqwest 0.12.28`. **Not exploitable in our code** — we don't call `rand::rng()` with a custom logger — but the advisory will fail CI under the new iter-101 `cargo-audit --deny warnings` gate.
2. **RUSTSEC-2021-0141** — `dotenv 0.15.0` unmaintained since 2021-12-24. Used by `teralib`. Migration path is `dotenvy` (actively maintained drop-in replacement, same API).

This sweep was the first with `cargo-audit` actually installed locally (iter 101 install retry succeeded — AV interference cleared). Iter 100 flagged the missing tool; iter 110 now has data.

## Part A — Dep tree delta vs iter 100

| Crate | iter 100 versions | iter 110 versions | Notes |
|---|---|---|---|
| reqwest | 0.12.28 + 0.13.2 | 0.12.28 + 0.13.2 | unchanged; upstream-gated per iter-87 deferral |
| cookie | 0.16.2 + 0.18.1 | 0.16.2 + 0.18.1 | unchanged |
| cookie_store | 0.21.1 + 0.22.0 | 0.21.1 + 0.22.0 | unchanged |
| env_logger | 0.10.2 + 0.11.8 | 0.10.2 + 0.11.8 | unchanged |
| bitflags | 1.3.2 + 2.10.0 | 1.3.2 + 2.10.0 | unchanged (pervasive) |
| getrandom | 0.1.16 + 0.2.17 | 0.1.16 + 0.2.17 + **0.3.4 NEW** | pulled in by rand 0.9.2 (tauri-plugin-notification / chamox / quinn-proto) |
| hashbrown | 0.12.3 + 0.14.5 | 0.12.3 + 0.14.5 + **0.16.1 NEW** | pulled in by indexmap 2.13.0 via h2/hyper/reqwest chain |
| rustls-webpki | 0.103.12 | 0.103.12 | unchanged — fresh since iter 81 |
| time | 0.3.47 | 0.3.47 | unchanged — fresh since iter 91 |

Both new triples (`getrandom 0.3.4`, `hashbrown 0.16.1`) are upstream-driven and non-actionable from our side — same deferral pattern as the reqwest 0.12/0.13 chain. Cost is binary-size only; no new advisories tied to either.

## Part B — cargo-audit findings

### teralaunch/src-tauri

```
Scanning Cargo.lock for vulnerabilities (476 crate dependencies)
error: 1 vulnerability found!
warning: 23 allowed warnings found
```

The single `error` is **RUSTSEC-2026-0097** on `rand 0.9.2`:

- **Title**: Rand is unsound with a custom logger using `rand::rng()`
- **Date**: 2026-04-09
- **URL**: https://rustsec.org/advisories/RUSTSEC-2026-0097
- **Upstream paths**:
  - `rand 0.9.2 ← tauri-plugin-notification 2.3.3`
  - `rand 0.9.2 ← quinn-proto 0.11.14 ← quinn 0.11.9 ← reqwest 0.12.28 ← (our code)`
  - `rand 0.9.2 ← chamox 0.1.4` (anti-reverse tooling)

**Applicability to our code:** We do not call `rand::rng()` with a custom logger anywhere in our code or in `teralib`. The unsoundness is behavioural — `rand::rng()` called from a non-default-logger context can emit incorrect randomness. Our direct random-number use (ticket generation, `AuthKey`) is via `OsRng` / `thread_rng()` in zeroize-backed paths; neither trips the unsound branch.

**But:** the iter-101 `cargo-audit --deny warnings` CI gate will fail on this advisory as soon as the worktree squash-merges. Options:

- **(A)** Wait for `tauri-plugin-notification 2.3.x` to bump to `rand 0.10+`. No release signals yet. Blocks first post-squash CI pass.
- **(B)** Add `--ignore RUSTSEC-2026-0097` with rationale comment to `cargo-audit.yml`. Keeps CI green; revisit when upstream bumps.
- **(C)** Fork `tauri-plugin-notification` to drop `rand`. Excessive for a notification plugin.

**Chosen direction:** option (B) for P2 follow-up (iter 111 or 112). Track as `dep.rand-advisory-ignore-2026-0097`.

### 23 "allowed warnings"

These are the `unmaintained` / `yanked` informational warnings cargo-audit emits at warning-level. The `--deny warnings` flag in `cargo-audit.yml` would normally promote these to hard failures. Most of the 23 are in the ancient `selectors 0.24.0` / `cssparser 0.29.6` chain pulled in by `kuchikiki` (Tauri's HTML parser) — upstream-locked at those majors.

For iter-110 purposes we focus on the one direct advisory (RUSTSEC-2026-0097). The warning noise is a separate `cargo-audit.yml` tuning question; may need `--deny warnings` relaxed or a broader ignore list. Queue as `infra.cargo-audit-tuning` (P3).

### teralib

```
Loaded 1049 security advisories
Scanning Cargo.lock for vulnerabilities (234 crate dependencies)
Crate:     dotenv
Version:   0.15.0
Warning:   unmaintained
Title:     dotenv is Unmaintained
Date:      2021-12-24
ID:        RUSTSEC-2021-0141
```

**RUSTSEC-2021-0141** — `dotenv 0.15.0` unmaintained since 2021-12-24 (over 4 years). `teralib` is the sole consumer. The advisory recommends `dotenvy` as a drop-in replacement.

**Scope:** 1 dep line in `teralib/Cargo.toml`, ~5-10 `use dotenv::` → `use dotenvy::` changes. Fully backward-compatible. Tests should pass unchanged.

**Chosen direction:** queue as P1 `dep.teralib-dotenv-to-dotenvy` — tight scope, clears the warning on the teralib side, ships value.

## Part C — Upstream release notes delta

Scanned since iter 100 for: tauri 2.x, reqwest, rustls, zip, tokio, serde.

| Package | Our pin | Latest | Delta |
|---|---|---|---|
| tauri | 2.10.3 | 2.10.x series, no 2.11 yet | hold |
| tauri-plugin-* | 2.x pins | unchanged | hold |
| reqwest | 0.12.28 | 0.12.x line still current | hold — dep-dedup deferral still in force |
| rustls | via 0.103.12 | stable | hold |
| zip | 4.x (via pin.external) | stable | hold |
| tokio | (transitive) | stable | hold |
| serde | (transitive) | stable | hold |

No upstream unblock of the reqwest 0.12/0.13 chain landed in the iter 100-110 window. Dep-dedup deferral remains accurate.

## Part D — New P-slot candidates

| Prio | Label | Scope | Ship value |
|---|---|---|---|
| P1 | `dep.teralib-dotenv-to-dotenvy` | replace dotenv 0.15.0 with dotenvy in teralib; 1 dep + ~10 import lines | clears RUSTSEC-2021-0141; keeps cargo-audit gate green on teralib post-squash |
| P2 | `dep.rand-advisory-ignore-2026-0097` | add `--ignore RUSTSEC-2026-0097` to `cargo-audit.yml` with cite comment | keeps cargo-audit gate green on teralaunch/src-tauri; revisit when tauri-plugin-notification bumps past rand 0.9.2 |
| P3 | `infra.cargo-audit-tuning` | decide whether `--deny warnings` is right default given 23 upstream-locked informational warnings | low urgency — warnings are non-blocking with current --deny policy; only relevant if policy tightens |

## Summary

First sweep with working cargo-audit. Surfaced two real advisories (one new since iter 100, one longstanding). Neither exploitable in our code, but both will trip the iter-101 CI gate post-squash. Queued P1 + P2 follow-ups to clear both before the user squash-merges. Revalidation not required this iter (N%20 != 0); next revalidation is iter 120 double-duty.

Net iter-110 risk delta: moderate — two advisories in the CI path need handling before squash. Worktree `ready_for_squash_merge: true` stands but with a caveat: **CI will fail the first run post-squash unless iter 111/112 clear these advisories first.**
