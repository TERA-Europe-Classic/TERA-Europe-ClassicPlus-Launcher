# CLAUDE.md

This file provides guidance to Claude Code when working with the TERA Europe Classic+ Launcher.

## Build & Development Commands

All commands run from the `teralaunch/` directory unless otherwise specified.

```bash
# Install frontend dependencies
npm install

# Development mode (hot-reload)
npm run tauri dev

# Production build (creates NSIS installer)
npm run tauri build

# Build with skip-updates feature (no file verification required)
cd src-tauri && cargo build --features skip-updates

# Run tests
npm test                    # Frontend tests (Vitest)
npm run test:watch          # Watch mode
npm run test:coverage       # With coverage report

# Full Windows build pipeline (from repo root)
pwsh ./builder.ps1
```

**Requirements:** Node.js, Rust toolchain, NSIS (for Windows installer)

## v100 API (Classic+ Server)

Base URL: `http://192.168.1.128:8090` (Portal API — LAN dev endpoint; no production yet)

### Authentication

Single-POST login replaces the Classic 4-step cookie chain:

```
POST /tera/LauncherLoginAction
Body: {"login": "username", "password": "password"}
Response: {"Return": true, "ReturnCode": 0, "Msg": "success",
           "CharacterCount": "2800|2800,3|", "Permission": 0,
           "Privilege": 31, "UserNo": 5, "UserName": "imweak",
           "AuthKey": "uuid"}
```

Error response: `{"Return": false, "ReturnCode": 50000, "Msg": "account not exist"}`

### Registration

```
POST /tera/LauncherSignupAction
Body: {"login": "name", "email": "email", "password": "pass"}
Response: {"Return": true, "ReturnCode": 0, "Msg": "success", "UserNo": 19, "AuthKey": "uuid"}
```

### Account Info

```
POST /tera/GetAccountInfoByUserNo
Body: {"userNo": 5, "authKey": "uuid"}
Response: {"Return": true, ..., "CharacterCount": "...", "Permission": 0, "Privilege": 31, "Language": "en", "Banned": false}
```

### Other Endpoints

- `GET /tera/LauncherMaintenanceStatus` - Check maintenance
- `GET /tera/ServerList?lang=en` - Server list (XML format)
- `POST /tera/SetAccountInfoByUserNo` - Set language preference

### Key Differences from Classic API

| Aspect | Classic | Classic+ (v100) |
|--------|---------|-----------------|
| Login | 4-step cookie chain (form POST + 3 GETs) | Single JSON POST |
| Auth | Session cookies | AuthKey in response body |
| Account info | Cookie-based GET | POST with {userNo, authKey} |
| Server list | JSON file | XML endpoint |
| Registration | /accountApi/RegisterNewAccount | /tera/LauncherSignupAction |
| Leaderboard consent | Supported | Not available |
| OAuth | Supported | Not available |
| Hash file / Updates | Available | Not available yet |

## Architecture

Same Tauri three-layer architecture as Classic. Key config file:

- `teralib/src/config/config.json` - All API URLs (points to 192.168.1.128:8090 — LAN dev endpoint; production FQDN + HTTPS required before public launch, tracked by 3.1.13.portal-https)

### Disabled Features

These Classic features are disabled in Classic+ (stubs return safe defaults):
- OAuth login (startOAuth, handleOAuthCallback, checkDeepLink)
- Leaderboard consent (ensureAuthSession, getLeaderboardConsent, setLeaderboardConsent)
- Profile token exchange
- News feed, patch notes (no endpoints yet)

Frontend guards skip empty URLs. Stubs marked with `// Classic+ TODO:` comments.

## Known Gaps

- **Server list XML parsing**: Frontend receives XML but tries to parse as JSON. Server status display is broken. Need XML parser (quick-xml crate or JS DOMParser).
- **Updater**: `tauri.conf.json` updater is `active: true` but endpoint `classicplus/latest.json` doesn't exist. Set to `false` or create the endpoint before release.
- **No hash file**: Update system has no hash file URL. The `skip-updates` feature flag or the runtime graceful fallback handles this.
- **Frontend removed-command errors**: Calls to removed Rust commands (exchange_oauth_token, get_pending_deep_link, get_leaderboard_consent, set_leaderboard_consent) will log console errors. Not critical since they're wrapped in try/catch.

## Cargo Feature Flags

- `custom-protocol` - Required for production builds (Tauri)
- `skip-updates` - Skip all file verification, allow game launch without updates. Opt-in: `cargo build --features skip-updates`

Without the flag, the launcher handles missing hash files gracefully at runtime (returns empty update list, enables Play button).

## Testing

- Frontend: 417 tests (Vitest + jsdom) in `teralaunch/tests/`
- Rust: 736 unit + 4 integration suites (smoke, http_allowlist, self_integrity, multi_client, zeroize_audit, crash_recovery) in `teralaunch/src-tauri/`
- Playwright e2e: 76 tests in 16 files under `teralaunch/tests/e2e/`
- Test credentials for v100 API: `imweak` / `!imweak5483`

## Mod Manager

The launcher ships an in-app mod manager spanning external-app mods (Shinra,
TCC) and GPK-style content mods (TMM-compatible). Feature state, code
layout, and operational constraints live in one place here so Claude
doesn't have to re-derive them each session.

### Feature state (iter 35)

| Area | State |
|---|---|
| External-app install/uninstall | Shipped — downloads zip, SHA-verifies, extracts into `mods/external/<id>/`, spawns on demand. |
| External-app lifecycle | Shipped — attach-once spawn decision, overlay-lifecycle predicate on TERA.exe count (wiring to the game-count watch channel is P1 `fix.overlay-lifecycle-wiring`). |
| GPK install | Shipped — downloads to `mods/gpk/<id>.gpk`, SHA-verifies, deploys via `tmm::install_gpk` (parses mod footer, patches `CompositePackageMapper.dat`, backs up vanilla as `.clean`). Container-filename sandbox prevents path traversal. |
| GPK conflict detection | Predicate shipped (`tmm::detect_conflicts`); modal UI + Tauri command wiring is P1 `fix.conflict-modal-wiring`. |
| Add-mod-from-file | Shipped — picks local `.gpk`, parses + SHA + safe-container check + deploy + registry upsert. |
| Catalog | Shipped — fetches from `raw.githubusercontent.com/TERA-Europe-Classic/external-mod-catalog`, caches for 24h at `<app_data>/mods/catalog-cache.json`. |
| Registry recovery | Shipped — `Registry::load()` auto-flips stranded `Installing` rows to `Error` on every startup. |
| Self-integrity | Shipped — launcher sha-verifies its own exe against sidecar baseline before Tauri boot; MessageBox + exit on mismatch. |
| Anti-reverse (Tauri v2 deps) | Blocked on sec.tauri-v1-eol-plan audit sign-off. |

### Code layout

```
teralaunch/src-tauri/src/
├── commands/mods.rs            Tauri command boundary (install/uninstall/enable/disable/launch/stop/add_mod_from_file/open_mods_folder)
└── services/mods/
    ├── catalog.rs              Remote catalog fetch + 24h disk cache
    ├── external_app.rs         Download + zip extraction + spawn + attach-once predicate + overlay lifecycle predicate + safe-container predicate
    ├── registry.rs             On-disk registry.json; recover_stuck_installs() runs on load
    ├── tmm.rs                  TMM format parser, mapper encrypt/decrypt, install_gpk, apply_mod_patches, detect_conflicts, is_safe_gpk_container_filename
    ├── types.rs                ModEntry, ModKind, ModStatus, CatalogEntry, ModEntry::from_catalog, ModEntry::from_local_gpk
    └── self_integrity.rs       verify_file/verify_self + REINSTALL_PROMPT (referenced by main.rs)

teralaunch/src/                 Frontend — mods.js renders Installed + Browse tabs; import-btn wires to add_mod_from_file
teralaunch/tests/e2e/           Playwright specs (helpers.js shared; per-describe *.spec.js)
teralaunch/src-tauri/tests/     Rust integration tests (http_allowlist, multi_client, crash_recovery, zeroize_audit, self_integrity, smoke)

docs/mod-manager/TROUBLESHOOT.md  10 user-facing error categories; covered 1:1 by scripts/check-troubleshoot-coverage.mjs
docs/PRD/                        PRD + fix-plan + audit docs driving the perfection loop
```

### Build

Same pipeline as the Classic launcher — `npm run tauri dev` for dev, `npm run tauri build` for NSIS installer. The Rust `[profile.release]` has LTO + strip + codegen-units=1 + panic=abort. Installer + updater zip land under `src-tauri/target/release/bundle/nsis/`.

### Deploy

`.github/workflows/deploy.yml` (`workflow_dispatch`):
1. Bump version (`patch` / `minor` / `major`) in `tauri.conf.json` + `Cargo.toml` + `package.json`.
2. Generate changelog from git log since last tag.
3. Build with `builder.ps1`.
4. Generate `latest.json` with minisign signature + downloads URL.
5. Run the **scope gate** (`teralaunch/tests/deploy_scope.spec.js`) — any upload URL outside `/classicplus/` or `/classic/classicplus/` fails the job.
6. Upload artefacts over FTPS to `ftp://${SFTP_HOST}/classicplus/`.
7. Commit version bump, tag `v<version>`, push tag, create GitHub release.

The secret-scan workflow (`.github/workflows/secret-scan.yml`) runs gitleaks 8.30.0 against new commits only — historical baseline was triaged in iter 13 (see `docs/PRD/audits/security/secret-leak-scan.md`).

### Running the perfection loop

The mod-manager perfection loop is tracked in `docs/PRD/fix-plan.md`. Machine-parseable YAML header drives iteration type (WORK / REVALIDATION / RESEARCH / RETROSPECTIVE / BLOCKED-RETRY) by counter. `lessons-learned.md` captures patterns worth remembering across sessions. To resume: read the fix-plan header, compute `N = iteration_counter + 1`, pick the iteration type, and do one item.
