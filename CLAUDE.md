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

Base URL: `http://88.99.102.67:8090` (Portal API)

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

- `teralib/src/config/config.json` - All API URLs (points to 88.99.102.67:8090)

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
- Rust: 693 tests in `teralaunch/src-tauri/`
- Test credentials for v100 API: `imweak` / `!imweak5483`
