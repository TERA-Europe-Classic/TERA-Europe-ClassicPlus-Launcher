# Refactor Wiring Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fully wire the refactor so runtime behavior flows through `commands/*` → `services/*` → `infrastructure/*`/`state/*`, with `main.rs` reduced to setup and command registration only.

**Architecture:** Treat `main.rs` as a thin shell. Each Tauri command in `commands/*` becomes the only entry point for UI calls and delegates to `services/*` for pure logic, `infrastructure/*` for IO, and `state/*` for shared state. Remove all legacy implementations from `main.rs` after parity is validated.

**Tech Stack:** Rust (Tauri, reqwest), JS frontend.

### Task 1: Map and document legacy runtime logic still in `main.rs`

**Files:**
- Modify: `teralaunch/src-tauri/src/main.rs`
- Create: `docs/plans/2026-01-27-refactor-wiring-map.md`

**Step 1: Write the failing test**

Add a small test in `teralaunch/src-tauri/src/main.rs` that asserts a sentinel function no longer exists (use `#[cfg(test)]` and `compile_error!` to force failure until removed).

**Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path teralaunch/src-tauri/Cargo.toml main::tests::legacy_guard`
Expected: FAIL with compile_error message.

**Step 3: Write minimal implementation**

Create `docs/plans/2026-01-27-refactor-wiring-map.md` listing each legacy function still in `main.rs` and its target module (`commands/*` or `services/*`).

**Step 4: Run test to verify it passes**

Update the test to assert the map exists (read file in test). Run the same test.
Expected: PASS.

**Step 5: Commit**

```bash
git add docs/plans/2026-01-27-refactor-wiring-map.md teralaunch/src-tauri/src/main.rs
git commit -m "docs: map legacy main.rs logic"
```

### Task 2: Wire auth flow through commands/services only

**Files:**
- Modify: `teralaunch/src-tauri/src/commands/auth.rs`
- Modify: `teralaunch/src-tauri/src/services/auth_service.rs`
- Modify: `teralaunch/src-tauri/src/state/auth_state.rs`
- Modify: `teralaunch/src-tauri/src/main.rs`
- Test: `teralaunch/src-tauri/src/commands/auth.rs`

**Step 1: Write the failing test**

Add a test in `commands/auth.rs` asserting `login` returns the same JSON shape as the old flow and that auth state is updated.

**Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path teralaunch/src-tauri/Cargo.toml commands::auth::tests::login_shape`
Expected: FAIL with missing shape/state.

**Step 3: Write minimal implementation**

Ensure `commands/auth.rs` calls `services/auth_service` and writes to `state/auth_state`. Remove any legacy auth logic from `main.rs`.

**Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path teralaunch/src-tauri/Cargo.toml commands::auth::tests::login_shape`
Expected: PASS.

**Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/commands/auth.rs teralaunch/src-tauri/src/services/auth_service.rs teralaunch/src-tauri/src/state/auth_state.rs teralaunch/src-tauri/src/main.rs
git commit -m "feat: wire auth commands to services"
```

### Task 3: Wire config flow through services and remove legacy code

**Files:**
- Modify: `teralaunch/src-tauri/src/commands/config.rs`
- Modify: `teralaunch/src-tauri/src/services/config_service.rs`
- Modify: `teralaunch/src-tauri/src/main.rs`
- Test: `teralaunch/src-tauri/src/commands/config.rs`

**Step 1: Write the failing test**

Add tests for `save_game_path_to_config` to ensure it emits `game_path_changed` and uses `config_service` for parsing.

**Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path teralaunch/src-tauri/Cargo.toml commands::config::tests::path_change`
Expected: FAIL.

**Step 3: Write minimal implementation**

Use `services/config_service` helpers, remove duplicated path logic from `main.rs`, and ensure event emission stays the same.

**Step 4: Run test to verify it passes**

Run the same test.

**Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/commands/config.rs teralaunch/src-tauri/src/services/config_service.rs teralaunch/src-tauri/src/main.rs
git commit -m "feat: wire config commands to services"
```

### Task 4: Wire download/update flow through services/state

**Files:**
- Modify: `teralaunch/src-tauri/src/commands/download.rs`
- Modify: `teralaunch/src-tauri/src/services/download_service.rs`
- Modify: `teralaunch/src-tauri/src/state/download_state.rs`
- Modify: `teralaunch/src-tauri/src/main.rs`
- Test: `teralaunch/src-tauri/src/commands/download.rs`

**Step 1: Write the failing test**

Add a test for `download_all_files` that asserts progress events use `download_service` calculations and state updates.

**Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path teralaunch/src-tauri/Cargo.toml commands::download::tests::progress_flow`
Expected: FAIL.

**Step 3: Write minimal implementation**

Route all progress calculations through `services/download_service` and shared counters in `state/download_state`. Remove any equivalent logic from `main.rs`.

**Step 4: Run test to verify it passes**

Run the same test.

**Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/commands/download.rs teralaunch/src-tauri/src/services/download_service.rs teralaunch/src-tauri/src/state/download_state.rs teralaunch/src-tauri/src/main.rs
git commit -m "feat: wire download commands to services"
```

### Task 5: Wire hash/check flow through services

**Files:**
- Modify: `teralaunch/src-tauri/src/commands/hash.rs`
- Modify: `teralaunch/src-tauri/src/services/hash_service.rs`
- Modify: `teralaunch/src-tauri/src/main.rs`
- Test: `teralaunch/src-tauri/src/commands/hash.rs`

**Step 1: Write the failing test**

Add a test for `get_files_to_update` that asserts hash calculation uses `hash_service` and respects ignored paths.

**Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path teralaunch/src-tauri/Cargo.toml commands::hash::tests::files_to_update`
Expected: FAIL.

**Step 3: Write minimal implementation**

Ensure `commands/hash.rs` delegates hashing to `services/hash_service` and remove corresponding logic from `main.rs`.

**Step 4: Run test to verify it passes**

Run the same test.

**Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/commands/hash.rs teralaunch/src-tauri/src/services/hash_service.rs teralaunch/src-tauri/src/main.rs
git commit -m "feat: wire hash commands to services"
```

### Task 6: Wire game launch flow through services

**Files:**
- Modify: `teralaunch/src-tauri/src/commands/game.rs`
- Modify: `teralaunch/src-tauri/src/services/game_service.rs`
- Modify: `teralaunch/src-tauri/src/main.rs`
- Test: `teralaunch/src-tauri/src/commands/game.rs`

**Step 1: Write the failing test**

Add a test to assert `handle_launch_game` uses `game_service` validation and returns expected errors.

**Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path teralaunch/src-tauri/Cargo.toml commands::game::tests::launch_validation`
Expected: FAIL.

**Step 3: Write minimal implementation**

Route validation/argument construction through `services/game_service` and remove duplicated logic from `main.rs`.

**Step 4: Run test to verify it passes**

Run the same test.

**Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/commands/game.rs teralaunch/src-tauri/src/services/game_service.rs teralaunch/src-tauri/src/main.rs
git commit -m "feat: wire game commands to services"
```

### Task 7: Final cleanup of `main.rs`

**Files:**
- Modify: `teralaunch/src-tauri/src/main.rs`

**Step 1: Write the failing test**

Add a test asserting `main.rs` contains no command implementations (e.g., by checking a list of forbidden function names).

**Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path teralaunch/src-tauri/Cargo.toml main::tests::no_legacy_impls`
Expected: FAIL.

**Step 3: Write minimal implementation**

Remove remaining legacy implementations, keeping only setup + `invoke_handler` registration.

**Step 4: Run test to verify it passes**

Run the same test.

**Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/main.rs
git commit -m "refactor: slim main.rs to wiring only"
```

### Task 8: Frontend contract verification

**Files:**
- Modify: `teralaunch/src/app.js` (only if mismatch found)

**Step 1: Write the failing test**

Add a unit test in `teralaunch/tests/app.test.js` to validate login response handling for the current JSON shape.

**Step 2: Run test to verify it fails**

Run: `cd teralaunch && npm test -- app.test.js`
Expected: FAIL if mismatch exists.

**Step 3: Write minimal implementation**

Adjust parsing only if required to match backend shape.

**Step 4: Run test to verify it passes**

Run the same test.

**Step 5: Commit**

```bash
git add teralaunch/src/app.js teralaunch/tests/app.test.js
git commit -m "test: verify login response contract"
```

