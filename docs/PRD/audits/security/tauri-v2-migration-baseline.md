# Tauri v1 → v2 Migration Baseline Snapshot

**Captured:** 2026-04-19 / iter 63, M0.
**Source branch:** `main` @ commit 6860d86.
**Worktree branch:** `tauri-v2-migration` (forked from main at this commit).

This file pins the pre-migration state so later milestones can be
diff-audited. Every row is a claim about main at M0 — if a v2
milestone changes it unintentionally, M8 validation catches the
regression.

## Toolchain

| Tool | Version |
|---|---|
| rustc | 1.89.0 (29483883e 2025-08-04) |
| cargo | 1.89.0 (c24e10642 2025-06-23) |
| node | v24.1.0 |
| npm | 11.3.0 |

## Tauri pins (v1)

| Dep | Pin | Source |
|---|---|---|
| `tauri` (Rust) | `1.0` + 15 features incl. `window-start-dragging`, `http-request`, `shell-open`, etc. | `teralaunch/src-tauri/Cargo.toml` |
| `tauri-build` | `1` | `teralaunch/src-tauri/Cargo.toml` |
| `@tauri-apps/cli` | `^1.6.0` (installed 1.6.3) | `teralaunch/package.json` |
| `@tauri-apps/cli-win32-x64-msvc` | `^2.9.6` (already v2 — native arch stub only) | `teralaunch/package.json` |
| `@tauri-apps/api` | — | not currently installed as a top-level dep; imports use `window.__TAURI__` via `withGlobalTauri: true` |

The `@tauri-apps/cli-win32-x64-msvc` pin at `^2.9.6` is a red herring — it's only the platform-specific stub the top-level CLI dispatches to, and it happens to have advanced past v2 independently. The actual CLI dispatcher is still v1.6.3.

## Command surface

41 `#[tauri::command]` annotations across:

| File | Count |
|---|---|
| `commands/auth.rs` | 5 |
| `commands/config.rs` | 6 |
| `commands/download.rs` | 4 |
| `commands/game.rs` | 4 |
| `commands/hash.rs` | 6 |
| `commands/mods.rs` | 10 |
| `commands/util.rs` | 6 |
| `main.rs` | — (registration only) |

Every one must still accept the same frontend invoke shape after M3. Arg + return types pinned by serde — behaviourally unchanged.

## Allowlist categories (11)

From `tauri.conf.json`:

1. `fs` — scoped to `$APP/*` and `$RESOURCE/*`, 6 methods
2. `path` — all
3. `dialog` — all
4. `shell` — `open` + custom `open-url` scope
5. `window` — 7 of 9 methods (close, hide, show, minimize, maximize, unmaximize, startDragging; setFocus off)
6. `process` — `exit` + `relaunch`
7. `http` — `request` + 9-entry scope
8. `notification` — all
9. `globalShortcut` — not used (omitted)
10. `clipboard` — not used (omitted)
11. `os` — not used (omitted)

M1 `cargo tauri migrate` parses these and generates `src-tauri/capabilities/*.toml`. Each capability file scopes the permissions to a specific window; our launcher has one main window, so we expect one capability file.

## HTTP scope (9 URLs)

```
https://*.tera-europe.net/*
https://*.tera-germany.de/*
https://*.crazy-esports.com/*
https://auth.tera-europe.net/*
https://dl.tera-europe.net/*
https://web.tera-germany.de/*
https://tera-europe-classic.com/*
https://raw.githubusercontent.com/*
http://192.168.1.128:8090/*   ← LAN dev portal (3.1.13 dormant)
```

M1 carries these into the capability file. M2 regression check: scope still enforces `tauri-plugin-http` in-process before reqwest leaves the allowlist.

## Updater config (v1)

- `active: true`
- endpoint: `https://web.tera-germany.de/classic/classicplus/latest.json`
- `dialog: true` (built-in update prompt)
- `pubkey`: base64 minisign public key (same one that stays in v2)

M4 enables `bundle.createUpdaterArtifacts: "v1Compatible"` so the same pubkey keeps signing both format artifacts.

## Test counts (pre-migration)

Launcher Rust (cargo test --release):

| Target | Tests |
|---|---|
| bin (unit) | 764 |
| `tests/crash_recovery.rs` | 3 |
| `tests/disk_full.rs` | 4 |
| `tests/http_allowlist.rs` | 3 |
| `tests/multi_client.rs` | 4 |
| `tests/parallel_install.rs` | 4 |
| `tests/self_integrity.rs` | 2 |
| `tests/smoke.rs` | 2 |
| `tests/zeroize_audit.rs` | 4 |
| **Total** | **790** |

Launcher JS (vitest):

| | |
|---|---|
| Test files | 10 |
| Tests | 431 |

Combined: **790 Rust + 431 JS = 1221 tests** that must still pass at M8.

Clippy baseline: `cargo clippy --all-targets --release -- -D warnings` exits 0. Must still exit 0 after every milestone.

## Bundle-size baseline (v0.1.10 release)

Artifact sizes from the most recent full release in
`teralaunch/src-tauri/target/release/bundle/nsis/`:

| Artifact | Size (bytes) | Size (MB) |
|---|---|---|
| `...0.1.10_x64-setup.exe` | 54,577,192 | 52.05 |
| `...0.1.10_x64-setup.nsis.zip` | 54,577,390 | 52.05 |
| `...0.1.10_x64-setup.nsis.zip.sig` | 456 | — |

M8 growth budget: ≤5% per iter-54 bundle-size gate. Expected v2 growth ~2-4 MB (plugin crate split adds a few hundred KB per plugin, offset by slightly leaner core). If the first v2 artifact exceeds 57.3 MB setup size, the gate fires — at which point we investigate whether a plugin was pulled in unnecessarily.

## CI gates (all green at M0)

From scripts/ and .github/workflows/:

1. `check-bundle-size.mjs` (10 self-tests, runs on deploy)
2. `check-changelog-plain-english.mjs` (6 self-tests, 126 lines scanned, 0 leaks)
3. `check-mods-crate-docs.mjs` (5 self-tests, 6 files covered)
4. `check-troubleshoot-coverage.mjs` (5 self-tests, 51/51 templates)
5. `external-mod-catalog/scripts/check-readme-schema.mjs` (9 self-tests, 21↔21)
6. `.github/workflows/secret-scan.yml` (gitleaks 8.30.0 on commit ranges)
7. `.github/workflows/deploy.yml` scope-gate step (2 upload URLs, 11 self-tests)

Every gate must still fire green after migration. M8 validation re-runs all seven.

## Minisign keys (unchanged)

- Public key fingerprint: `RWSEL+9/IIo3Gw3Vn1pXMl8p+ykWyKsZ/dzjmVrs0Ll2v1v9rE0yed2L` (from `tauri.conf.json` `updater.pubkey`).
- Private key stored in `TAURI_PRIVATE_KEY` + `TAURI_KEY_PASSWORD` GitHub secrets.

No rotation during migration. v2 updater plugin uses the same key pair.

## Git state

```
$ git rev-parse HEAD
6860d86d...   (docs(sec): tauri v1→v2 migration plan @ iter 62)

$ git log --oneline main -5
6860d86 docs(sec): tauri v1→v2 migration plan with 10 tool-assisted milestones @ iter 62
9b220bc fix(docs): restore troubleshoot coverage for catalog envelope errors @ iter 61
c5a58b1 chore: iter 60 research + revalidation + retrospective
930af20 test(mods): pin progress-rate ≥10Hz on chunked-stream downloads @ iter 59
6860d86 docs(sec): tauri v1→v2 migration plan @ iter 62
```

Worktree branch `tauri-v2-migration` forks from main at `6860d86`. All M1–M8 commits land on this branch; main stays untouched until the M8 squash merge.

## Rollback target

If the migration fails after M8 merge, revert with:

```bash
git revert --no-edit <M8-squash-sha>
```

The minisign keys continue signing v1-format manifests for any hotfix release cut on the reverted state. Existing 0.1.x installs keep auto-updating.
