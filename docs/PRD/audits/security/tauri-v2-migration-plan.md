# Tauri v1 → v2 Migration Plan

**Status:** Draft
**Author:** perfection loop @ iter 62
**Supersedes:** the "recommendation = migrate" conclusion at the end of `tauri-v2-migration.md` (iter 18 audit).
**Unblocks:** PRD §3.1.8 anti-reverse-hardening, §3.1.9 updater-downgrade-refuse, §3.1.12 csp-unsafe-inline.

## Why this plan exists

Iter 57 the user approved migration. Iter 59 I caught myself about to execute a flawed milestone sequence: "M1: port frontend JS imports only" would have put main into a broken runtime state — v2 JS speaks a different invoke protocol than v1 Rust.

Iter 62 Context7 lookup surfaced two facts that reshape the migration:

1. **`cargo tauri migrate`** — an official automated migration command that parses the v1 allowlist, generates v2 capability files, updates Cargo.toml deps to split out plugin crates, and fixes most frontend import paths. Most of the hand-porting I planned is already tooled.

2. **`bundle.createUpdaterArtifacts: "v1Compatible"`** — a single flag in `tauri.conf.json` that emits both v1-format and v2-format updater artifacts during the transition window. Existing 0.1.x installs continue to auto-update; new installs run on v2. The "dual-format window" concern from iter 57 collapses to "set the flag, leave it on until 0.1.x user count hits zero."

Both are documented at `v2.tauri.app/start/migrate/from-tauri-1`.

## Migration invariants (never violated)

1. **`main` never transits through a broken state.** Every milestone commit must leave `cargo build --release` + `npm run build` + the unit tests green on the worktree branch. Merges to main happen only at M-final.
2. **Existing users don't lose auto-update.** `createUpdaterArtifacts: "v1Compatible"` is ON from M4 until the user signals 0.1.x user count is negligible.
3. **No minisign key rotation during migration.** The same signing key pair is used for both v1 and v2 manifests.
4. **The catalog schema + mods CI gates still pass at every milestone.** Migration is a Tauri cutover, not a re-architecture.
5. **Tests never get weaker.** If a v1 test can't trivially port to v2, it stays as a pinned archived test until its v2 replacement lands.

## Target versions

- **Tauri:** latest stable v2 at time of execution (currently 2.x — M1 `cargo tauri migrate` pins it).
- **Launcher semver:** bump to **0.2.0** (minor). Reason: invoke protocol is user-visible via the updater manifest; minor signals "compat break for the updater path, not for end-user data."
- **Dual-format window:** indefinite until we explicitly disable it in a future release. Cost is a slightly larger bundle + one extra manifest on the CDN — cheap insurance for stragglers.

## Milestones

Each milestone is **one commit** on the `tauri-v2-migration` worktree branch. The branch merges to `main` in a single squash at M-final.

### M0 — Worktree + baseline snapshot (no code change)

- `git worktree add ../tauri-v2-migration -b tauri-v2-migration`
- Snapshot current `cargo --version` / `tauri --version` / `node --version` to `docs/PRD/audits/security/tauri-v2-migration-baseline.md`. Snapshots the exact test counts, clippy output, bundle size for rollback reference.
- **Acceptance:** worktree exists at known commit; baseline doc committed.

### M1 — `cargo tauri migrate`

- Install v2 CLI: `cargo install tauri-cli --version "^2" --locked`
- Run the tool on the worktree: `cd teralaunch/src-tauri && cargo tauri migrate`
- Inspect what it changed. Expected: Cargo.toml dep split (`tauri = "2"` + plugin crates); `tauri.conf.json` shape change; `src-tauri/capabilities/*.toml` generated from old allowlist; main.rs `.plugin(...)` calls appended.
- Do **not** accept any manual edits this milestone. Pure tool output.
- **Acceptance:** `cargo build --release` passes on the worktree. No other check yet — this milestone exists specifically to pin the tool's output as a reviewable diff.

### M2 — JS import path migration

- `npm install @tauri-apps/api@^2 @tauri-apps/plugin-dialog @tauri-apps/plugin-shell @tauri-apps/plugin-fs @tauri-apps/plugin-http @tauri-apps/plugin-updater @tauri-apps/plugin-process` (whichever subset the allowlist categorised).
- Rewrite imports across `teralaunch/src/*.js`:
  - `@tauri-apps/api/tauri` → `@tauri-apps/api/core`
  - `@tauri-apps/api/dialog` → `@tauri-apps/plugin-dialog`
  - `@tauri-apps/api/shell` → `@tauri-apps/plugin-shell`
  - `@tauri-apps/api/fs` → `@tauri-apps/plugin-fs` (if used)
  - `@tauri-apps/api/http` → `@tauri-apps/plugin-http` (if used)
  - `@tauri-apps/api/event` → `@tauri-apps/api/event` (unchanged)
- Grep for stragglers: `grep -rn "@tauri-apps/api/" teralaunch/src/ | grep -v '/api/core\|/api/event'`.
- **Acceptance:** `npm run build` passes; Vitest 10 files / 431+ tests pass; `npm run tauri dev` boots.

### M3 — Custom-command surface review

- The migrate tool handles `#[tauri::command]` state changes, but our codebase has 40 commands + custom state objects (`mods_state`, `auth_state`, `download_state`). Audit each for v2-specific issues:
  - `State<'_, T>` → still works, but confirm the new `tauri::State` import path.
  - `Window` → `WebviewWindow` (where applicable).
  - `emit_all` / `emit` → API path changes. Grep and update.
- Re-run `cargo clippy --all-targets --release -- -D warnings` + `cargo test --release`.
- **Acceptance:** clippy clean + 764+ unit + all integration tests pass on the worktree.

### M4 — Updater dual-format + v2 signature

- Set `bundle.createUpdaterArtifacts: "v1Compatible"` in `tauri.conf.json`.
- Build a release artifact with the v2 CLI: `npm run tauri build`.
- Verify the `target/release/bundle/nsis/` directory contains BOTH `*-setup.nsis.zip` (v2 format) AND a v1-compatible manifest.
- Cross-check the v2 `latest.json` shape against Context7's documented schema.
- Re-verify the bundle-size gate from iter 54 still fires on the new artifacts. May need to regenerate the size baseline for the first v2 build.
- **Acceptance:** both formats present; v1-format manifest successfully parses in a current 0.1.x launcher running against a local HTTPS fixture.

### M5 — CSP tightening (3.1.12)

- Drop `unsafe-inline` from `script-src` in `tauri.conf.json`.
- Fix any inline `<script>` tags by moving to external modules.
- Author `teralaunch/src-tauri/tests/csp_audit.rs::csp_denies_inline_scripts` per PRD §3.1.12 acceptance.
- **Acceptance:** CSP audit test passes; modal-open smoke test still works.

### M6 — Anti-reverse hardening (3.1.8)

- Apply release-profile flags per PRD §3.1.8: LTO (already on), strip (already on), CFG (/guard:cf in build.rs linker args for Windows), stack-canary.
- `cryptify` or `chamox` string obfuscation on the small set of sensitive literals: portal URLs, AuthKey-adjacent code paths, updater URL, deploy paths.
- Author `docs/PRD/audits/security/anti-reverse.md` with build-output inspection evidence.
- **Acceptance:** audit doc signed off; release binary string-grep for `192.168.1.128` returns zero hits in the obfuscated sections.

### M7 — Updater-downgrade refusal (3.1.9)

- Patch the updater plugin call to check `semver::Version::parse(new) > semver::Version::parse(current)` before accepting an update.
- Author `teralaunch/src-tauri/tests/updater_downgrade.rs::refuses_older_latest_json` with a signed older fixture.
- **Acceptance:** test passes with a deliberate older `latest.json`.

### M8 — Full validation sweep + squash merge

- Re-run every CI gate: launcher Rust + Vitest + catalog schema + bundle-size + deploy-scope + changelog + mods-crate-docs + troubleshoot-coverage.
- Playwright e2e on a pre-warmed cargo build (timeout already bumped to 10 min in iter 54).
- Tag: bump `package.json` + `Cargo.toml` + `tauri.conf.json` to `0.2.0`.
- Squash-merge the worktree branch into main with a single `feat(tauri)!: migrate to v2` commit referencing this plan and every milestone commit SHA.
- Deploy a release via the existing `.github/workflows/deploy.yml` path (scope gate + bundle-size gate already in place).
- **Acceptance:** every automated gate green; a fresh user install from the v2 artifact runs end-to-end; a 0.1.x install successfully auto-upgrades via the dual-format manifest.

### M9 — Post-merge monitor (1 iter, no code)

- Watch for downstream regressions reported by 0.1.x users hitting the v2 updater for the first time.
- If clean after 1 week of real user upgrades, schedule a future iter to flip `createUpdaterArtifacts` back to the v2-only default.
- Close PRD §3.1.8, §3.1.9, §3.1.12 as DONE with the M-commit SHAs as proof.

## Rollback strategy

Every milestone is its own commit on the worktree. If M3 breaks and we can't fix it within 2 iterations, `git reset --hard` to M2 and try a different approach. The worktree is the firewall — main is never touched until M8.

If the squash merge itself ships and 0.1.x users report mass update failures: the fix is cutting a 0.1.13 hotfix release on the (now obsolete) v1 branch that publishes a v1-format-only `latest.json` to a fallback URL. The minisign key still signs it — no key rotation needed.

## What this plan explicitly does NOT do

- Does not try to port v1 tests to v2 one-by-one as a separate step. `cargo tauri migrate` + M3's command-surface review handles it.
- Does not add any new features during the migration. CSP tightening, anti-reverse, and downgrade-refuse are all PRD-specified items already on the backlog — they happen during migration only because they're v2-native capabilities.
- Does not delete v1-format updater support at the end. That's a separate future decision once 0.1.x user count hits ~0.
- Does not re-generate the `classicplus-signing.key` minisign pair. Same keys, same `dW50cnVzdGVkIGNvbW1lbnQ6...` pubkey.

## Estimated iteration cost

- M0: 1 iter (worktree + baseline doc)
- M1: 1 iter (cargo tauri migrate + review)
- M2: 1 iter (JS imports + smoke)
- M3: 1–2 iters (command review, depending on what breaks)
- M4: 1 iter (updater dual-format)
- M5: 1 iter (CSP + test)
- M6: 1–2 iters (anti-reverse + audit)
- M7: 1 iter (downgrade refusal)
- M8: 1 iter (squash merge + deploy)
- M9: 1 iter (monitor)

**Total: 10–12 iterations.** Roughly 1/10th of the 1000-iter cap.

## Open questions (deferred to each milestone)

- **M1:** does `cargo tauri migrate` handle our custom Windows `ShellExecuteExW` spawn code in `external_app.rs`? If it doesn't, M3 picks it up.
- **M3:** do any of our 40 Tauri commands use a pattern the v2 macro rejects? Find out at clippy time.
- **M4:** does the v1-compatible manifest format include the v2 signature alongside the v1 signature, or are they separate files? Verify against the `bundle/nsis/` output.
- **M6:** is `cryptify` 3.1.1 (our pinned version) still the right tool for v2? Check at milestone time.
