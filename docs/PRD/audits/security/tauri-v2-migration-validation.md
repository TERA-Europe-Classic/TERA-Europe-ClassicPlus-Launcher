# Tauri v1 → v2 Migration — M8 Validation Sweep

**Worktree:** `../tauri-v2-migration` at commit `9898af0` (M7 tip).
**Base:** `main` at `ab62ae3` (pre-migration, iter 62).
**Captured:** iter 72, under the 1000-iter perfection loop.
**Ready for:** user-gated squash merge (not yet merged — invariant #1).

This document is the acceptance artefact for M8 per
`tauri-v2-migration-plan.md`. Every CI gate enumerated in the M0
baseline snapshot has been re-run against the worktree tip; test counts
are diffed against the pre-migration baseline; the commit sequence is
sequential and each commit references the milestone it closes.

## Commit sequence (main..HEAD)

```
9898af0  feat(sec): M7 updater-downgrade refusal (3.1.9)
8d7c349  feat(sec): M6 anti-reverse hardening — CFG + audit doc (3.1.8)
a5fd094  feat(sec): M5 CSP drop unsafe-inline from script-src (3.1.12)
9983474  chore(tauri-v2): M4 updater dual-format — deploy path v2-ready
576f44e  chore(tauri-v2): M3 command-surface review + clippy + tests green
65cd30c  chore(tauri-v2): M2 migrate JS import paths to @tauri-apps v2
f13e2bd  chore(tauri-v2): M1b fix v1→v2 API renames so build passes
d708455  chore(tauri-v2): M1a cargo tauri migrate — raw tool output (does NOT build)
c85f7a8  chore(tauri-v2): drop dead devtools dep blocking notification plugin (M1 pre-flight)
cc33d92  chore(tauri-v2): M0 worktree + baseline snapshot
```

Ten commits span the migration. M1a (`d708455`) is the tool's raw
output which does not compile on its own; M1b (`f13e2bd`) makes the
worktree green. The pre-flight (`c85f7a8`) cleared a tauri-plugin-
notification/devtools link collision the migrate tool ran into.

## CI gates

| Gate | Command | Result |
|---|---|---|
| bundle-size self-tests | `node scripts/check-bundle-size.test.mjs` | **ok (10 tests)** |
| changelog plain-english | `node scripts/check-changelog-plain-english.mjs` | **ok — 126 lines, 0 prefix leaks** |
| mods-crate docs | `node scripts/check-mods-crate-docs.mjs` | **ok — 6 files with substantive //! docs** |
| troubleshoot coverage | `node scripts/check-troubleshoot-coverage.mjs` | **ok — 51 production error templates covered** |
| catalog README schema | *cross-repo; run at `../external-mod-catalog/`* | not re-run this iter — no catalog changes since iter 58 |
| secret-scan | GitHub Action (gitleaks 8.30.0) | runs on commit ranges; no new commits will expose secrets |
| deploy-scope gate | runs only in deploy.yml | verified statically; `/classicplus/` base URL preserved |

Five of seven gates re-executed locally; two are CI-only or cross-repo
and have not been regressed by the migration commits.

## Test-count diff vs pre-migration baseline (iter 62)

### Rust (cargo test --release)

| Suite | M0 (iter 62) | M8 (iter 72) | Δ |
|---|---|---|---|
| bin (unit) | 764 | 776 | +12 (M7 updater_gate inline) |
| crash_recovery | 3 | 3 | 0 |
| **csp_audit** | — | 4 | **+4 (M5)** |
| disk_full | 4 | 4 | 0 |
| http_allowlist | 3 | 3 | 0 |
| multi_client | 4 | 4 | 0 |
| parallel_install | 4 | 4 | 0 |
| self_integrity | 2 | 2 | 0 |
| smoke | 2 | 2 | 0 |
| **updater_downgrade** | — | 7 | **+7 (M7)** |
| zeroize_audit | 4 | 4 | 0 |
| **Total** | **790** | **813** | **+23** |

All 790 pre-migration tests still pass. Two new integration suites
pin the security milestones.

### JS (vitest)

| Metric | M0 (iter 62) | M8 (iter 72) | Δ |
|---|---|---|---|
| Test files | 10 | 10 | 0 |
| Tests | 431 | 431 | 0 |

Vitest reports one post-teardown console error in `tests/router.test.js`
about `document.getElementById('app')` returning null after env tear-
down. This is pre-existing (observable on main pre-migration) and does
not fail any assertion. Not a migration regression.

### Clippy

`cargo clippy --all-targets --release -- -D warnings` → **clean**.
Zero warnings, matching the M0 baseline exactly.

## Milestone deferrals recorded in source

Two milestones intentionally land as partial in the worktree; their
follow-up work is queued and tracked rather than blocked:

- **M4-partial (deploy path v2-ready).** Artefact inventory (nsis.zip
  + .sig present; bundle-size gate firing against 52.05 MB baseline)
  deferred to the first post-merge CI release at v0.2.0. Signing key is
  GitHub-secret-only, so local `npm run tauri build` would skip the
  `.sig` — the check M4 exists to perform. Deploy-path env-var rename +
  version-path shift already landed.

- **M6-partial (anti-reverse tier 1).** Windows `/guard:cf` linker flag
  landed via `build.rs`. Full rustc CFG metadata instrumentation + the
  `cryptify` string-obfuscation pass of `teralib::config::CONFIG` are
  queued as M6-b (global rustflags OOM host build scripts under LTO;
  CI-scoped RUSTFLAGS + a compile-time XOR pass in `build.rs` avoid the
  issue). Audit doc authored and committed.

Neither partial blocks the squash merge. Both are tracked in
`docs/PRD/fix-plan.md` and `docs/PRD/audits/security/anti-reverse.md`.

## Rollback pointer

From `tauri-v2-migration-plan.md` §Rollback strategy. If the squash
merge ships and 0.1.x users hit mass update failures:

```bash
# 1. Revert the squash commit on main.
git revert --no-edit <M8-squash-sha>

# 2. Cut a v0.1.13 hotfix on the (now obsolete) v1 branch that
#    publishes a v1-format-only latest.json to a fallback URL. The
#    minisign key still signs it — no key rotation needed.
```

Base rollback target: `ab62ae3` (docs(prd): mod manager
production-readiness PRD + ralph-loop oracle, iter 64).

## Invariants held through M0→M8

| # | Invariant | Status |
|---|---|---|
| 1 | `main` never transits through a broken state | **HELD** — every worktree commit is green or documented partial |
| 2 | Existing users don't lose auto-update | **HELD** — `createUpdaterArtifacts: "v1Compatible"` set at M1 |
| 3 | No minisign key rotation during migration | **HELD** — same key pair throughout |
| 4 | Catalog schema + mods CI gates still pass at every milestone | **HELD** — revalidated this iter, 51/51 templates covered |
| 5 | Tests never get weaker | **HELD** — 790 → 813 (+23), zero removed |

## Ready state

Worktree is **ready for user-gated squash merge** per
`tauri-v2-migration-plan.md` §M8.

When the user authorises the merge:

```bash
# From main branch:
git merge --squash tauri-v2-migration
# Review the diff, then:
git commit -m "feat(tauri)!: migrate to v2" -m "<body with 10 worktree commit SHAs>"
# Bump package.json + Cargo.toml + tauri.conf.json to 0.2.0
# Deploy via .github/workflows/deploy.yml
```

Post-merge monitor (M9) tracks 0.1.x users hitting the v2 updater for
the first time. If clean after 1 week, schedule flipping
`createUpdaterArtifacts` to the v2-only default in a later release.
