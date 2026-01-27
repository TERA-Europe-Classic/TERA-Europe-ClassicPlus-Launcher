# Refactor Wiring Design

**Goal:** Fully wire the refactor so all runtime behavior flows through `commands/*` → `services/*` → `infrastructure/*`/`state/*`, with `main.rs` reduced to setup + command registration only. Preserve current behavior and UI contracts; only non-breaking improvements.

## Architecture

`main.rs` becomes a thin shell that only configures Tauri, sets up logging/window behavior, and registers commands. All actual behavior lives in `commands/*`, which serve as the API boundary for the UI. Commands validate inputs, call `services/*` for pure logic, use `infrastructure/*` for IO, update `state/*`, and emit events.

No business logic or IO should remain in `main.rs`. Any old implementations in `main.rs` are removed once their corresponding command paths are verified.

## Data Flow and Module Responsibilities

- **Auth:** `commands/auth.rs` performs multi-step login using `infrastructure/http.rs` (cookie + UA enabled), validates/parses via `services/auth_service.rs`, stores auth in `state/auth_state.rs`, and returns the same JSON payload as the legacy flow.
- **Config:** `commands/config.rs` uses `services/config_service.rs` for INI parsing and updates, filesystem IO via `infrastructure/filesystem.rs`, and emits `game_path_changed` when appropriate.
- **Download/Update:** `commands/download.rs` uses `services/download_service.rs` for calculations and planning, `infrastructure/http.rs` + `filesystem.rs` for transfers, and `state/download_state.rs` for progress. Events emitted must match existing payload shapes.
- **Hash/Integrity:** `commands/hash.rs` uses `services/hash_service.rs` (hashing logic) plus filesystem reads; update checks remain functionally identical.
- **Game Launch:** `commands/game.rs` uses `services/game_service.rs` for validation/argument assembly and `teralib` for process launch + status.
- **Constants/Types:** shared config and types live in `domain/*`.

## Error Handling

Commands are the translation boundary. Services can return typed errors, but commands must map them to the exact string codes or messages the UI expects (e.g., `INVALID_CREDENTIALS`, `SERVER_CONNECTION_ERROR`). Event names and payload shapes must remain identical.

## Non‑Breaking Guarantees

- Preserve JSON keys and event names.
- Preserve route timing/flow on login and update checks.
- UI should not need changes unless a strict mismatch is discovered.

## Testing and Verification

- Keep or add unit tests in services for any newly wired logic.
- Minimal command‑level tests only where payload shaping is critical.
- Manual runtime verification: login success/failure, update check + download, game launch, path change, hash check.

