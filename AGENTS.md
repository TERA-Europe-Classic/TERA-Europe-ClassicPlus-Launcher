# Repository Guidelines

## Project Structure & Module Organization
- `teralaunch/` holds the Tauri app frontend (HTML/CSS/JS) plus configuration.
- `teralaunch/src/` contains pages, styles, router, and assets; images live in `teralaunch/src/assets/`.
- `teralaunch/tests/` contains Vitest unit tests (e.g., `app.test.js`).
- `teralaunch/src-tauri/` is the Rust Tauri backend and bundling config.
- `teralib/` is a Rust library/binary used by the launcher; binaries/assets are under `teralib/`.

## Build, Test, and Development Commands
- `npm install` (run in `teralaunch/`): install frontend dependencies.
- `npm run tauri build` (in `teralaunch/`): build the Tauri app/bundles.
- `npm test` (in `teralaunch/`): run Vitest once in CI mode.
- `npm run test:coverage` (in `teralaunch/`): run tests with coverage.
- `pwsh ./builder.ps1` (repo root): Windows build pipeline with NSIS and optional signing.

## Coding Style & Naming Conventions
- Use 2-space indentation for HTML/CSS/JS in `teralaunch/src/`.
- Rust follows `rustfmt` defaults (Tauri in `teralaunch/src-tauri/`, library in `teralib/`).
- Tests live under `teralaunch/tests/` and use `*.test.js` naming.

## Testing Guidelines
- Test framework: Vitest with jsdom (see `teralaunch/vitest.config.js`).
- Keep tests close to features (e.g., router tests in `teralaunch/tests/router.test.js`).
- Run `npm test` before PRs; use `npm run test:coverage` when changing logic-heavy code.

## Commit & Pull Request Guidelines
- Follow the existing git history style: short, imperative messages; use prefixes like `chore:` when applicable.
- PRs should include a clear description, link relevant issues, and add screenshots for UI changes.

## Security & Configuration Notes
- `tera_config.ini` is created on first run in the OS config directory (see `teralaunch/README.md`).
- Do not commit signing keys or secrets; `builder.ps1` supports env-based or local key files.
