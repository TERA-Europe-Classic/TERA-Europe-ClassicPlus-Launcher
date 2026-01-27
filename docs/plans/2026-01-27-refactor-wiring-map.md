# Legacy Logic Map (main.rs)

This file documents runtime logic still defined in `teralaunch/src-tauri/src/main.rs` after the refactor, so it can be tracked and intentionally retained or moved.

## Remaining Runtime Logic

- `game_state` module: defines `GameState` for Tauri managed state (status receiver + is_launching).
- `should_auto_install_updater`: small helper for env-gated auto-update checks.
- `main`: Tauri setup, window show/hide logic during updater check, and command registration.

## Intended Targets

- `GameState` stays in `main.rs` (used only for Tauri state wiring).
- `should_auto_install_updater` stays in `main.rs` (setup helper).
- No command implementations remain in `main.rs`.

