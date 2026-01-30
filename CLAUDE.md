# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Development Commands

All commands run from the `teralaunch/` directory unless otherwise specified.

```bash
# Install frontend dependencies
npm install

# Development mode (hot-reload)
npm run tauri dev

# Production build (creates NSIS installer)
npm run tauri build

# Run tests
npm test                    # Single run
npm run test:watch          # Watch mode
npm run test:coverage       # With coverage report

# Full Windows build pipeline (from repo root)
pwsh ./builder.ps1
```

**Requirements:** Node.js, Rust toolchain, NSIS (for Windows installer)

## Architecture

This is a Tauri-based Windows game launcher with three layers:

```
┌─────────────────────────────────────────────────────────┐
│  Frontend (Vanilla JS/HTML/CSS)                         │
│  teralaunch/src/                                        │
│  - Multi-page SPA with router.js                        │
│  - 4 languages: DE, EN, FR, RU (translations.json)      │
└────────────────────────┬────────────────────────────────┘
                         │ Tauri IPC (window.__TAURI__.invoke)
┌────────────────────────▼────────────────────────────────┐
│  Tauri Backend (Rust)                                   │
│  teralaunch/src-tauri/src/main.rs                       │
│  - #[tauri::command] functions for auth, downloads,     │
│    file verification, config management                 │
│  - Parallel chunked downloads with resume support       │
└────────────────────────┬────────────────────────────────┘
                         │ Library import
┌────────────────────────▼────────────────────────────────┐
│  teralib (Rust Library)                                 │
│  teralib/src/                                           │
│  - game/: Windows process launching, message loop       │
│  - av/: Windows Defender exclusion checking             │
│  - config.rs: Server URLs from config/config.json       │
│  - Built as both cdylib and rlib                        │
└─────────────────────────────────────────────────────────┘
```

### Key Data Flows

**Authentication:** Frontend → `invoke('login')` → Tauri → auth.tera-europe.net API

**Game Launch:** Frontend → `invoke('launch_game')` → Tauri (file verification) → `teralib::run_game()` → Windows API

**Updates:** Frontend checks `hash-file.json` → Tauri downloads delta from dl.tera-europe.net → Parallel chunked download with progress events

### Important Files

| File | Purpose |
|------|---------|
| `teralaunch/src/app.js` | Main app logic, update orchestration |
| `teralaunch/src/router.js` | SPA routing between pages |
| `teralaunch/src-tauri/src/main.rs` | All Tauri commands (login, download, launch) |
| `teralib/src/game/mod.rs` | Windows process injection and monitoring |
| `teralib/src/config/config.json` | Server URLs and API endpoints |
| `teralaunch/src/translations.json` | All UI strings in 4 languages |
| `teralaunch/src-tauri/tauri.conf.json` | Window settings, updater, bundle config |

### Frontend-Backend Communication

All Tauri commands are invoked via `window.__TAURI__.invoke()`. Key commands:

- `login(username, password)` - Authenticates against REST API
- `launch_game(game_path)` - Validates files and starts game via teralib
- `start_download(files, game_path)` - Downloads with progress events
- `verify_files(game_path)` - Hash-based file integrity check
- `save_config(key, value)` / `load_config(key)` - INI file persistence

Progress updates are sent via `emit('download-progress', {...})` events.

## Testing

Tests use Vitest with jsdom environment. Located in `teralaunch/tests/`:
- `app.test.js` - Application logic tests
- `router.test.js` - SPA routing tests
- `utils.test.js` - Utility function tests

Run a single test file:
```bash
npx vitest run tests/router.test.js
```

## Localization

Edit `teralaunch/src/translations.json` to add/modify translations. Language keys: `GER`, `EUR` (English), `FRA`, `RUS`. The UI updates live without restart when the user switches languages.

## Updater Signing

The builder supports auto-signing for Tauri's updater. Provide keys via:
1. Environment variables: `TAURI_PRIVATE_KEY`, `TAURI_KEY_PASSWORD`
2. Files next to builder.ps1: `tauri_private_key.txt`, `tauri_private_key_password.txt`
3. `.env` files in repo root, teralaunch/, or src-tauri/
