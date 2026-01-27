# Launcher Stability & Warning Cleanup Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Restore login functionality and eliminate all build warnings (including tarpaulin cfg warnings) introduced by the refactor.

**Architecture:** Keep the refactor structure but re-enable required HTTP behavior for auth (cookies + User-Agent). Suppress or eliminate warnings by declaring tarpaulin cfg and moving test-only utilities behind test cfg or explicit allow blocks where intentionally unused.

**Tech Stack:** Tauri (Rust), reqwest, JavaScript frontend.

### Task 1: Reproduce and inventory warnings in the worktree

**Files:**
- Modify: none

**Step 1: Capture current Rust warnings (best-effort)**

Run: `cargo check --manifest-path teralaunch/src-tauri/Cargo.toml`

Expected: Warning list includes `unexpected_cfgs` (tarpaulin) and many `dead_code` warnings.

**Step 2: Capture frontend build/lint warnings (optional)**

Run: `cd teralaunch && npm test` (optional)

Expected: No new warnings beyond Rust build.

### Task 2: Fix login regression by restoring session cookies and User-Agent

**Files:**
- Modify: `teralaunch/src-tauri/src/infrastructure/http.rs`
- Modify: `teralaunch/src-tauri/src/commands/auth.rs`

**Step 1: Add a default User-Agent constant and enable cookie store in ReqwestClient defaults**

```rust
const DEFAULT_USER_AGENT: &str = "Tera Game Launcher";
```

Update `ReqwestClient::with_defaults` to set `.cookie_store(true)` and `.user_agent(DEFAULT_USER_AGENT)`.

**Step 2: Ensure auth requests consistently use the same client instance**

Confirm `login_with_client` uses the same `ReqwestClient` for login/account/auth-key/character-count.

**Step 3: Run a quick login flow sanity check**

Run: `cargo check --manifest-path teralaunch/src-tauri/Cargo.toml`

Expected: No auth compile errors.

### Task 3: Silence tarpaulin cfg warnings cleanly

**Files:**
- Modify: `teralaunch/src-tauri/Cargo.toml`

**Step 1: Declare tarpaulin cfg in Cargo lints**

Add:

```toml
[lints.rust]
unexpected_cfgs = { level = "warn", check-cfg = ["cfg(tarpaulin_include)"] }
```

**Step 2: Verify warnings removed**

Run: `cargo check --manifest-path teralaunch/src-tauri/Cargo.toml`

Expected: No `unexpected_cfgs` warnings.

### Task 4: Remove dead_code warnings in refactor-only modules

**Files:**
- Modify: `teralaunch/src-tauri/src/services/*.rs`
- Modify: `teralaunch/src-tauri/src/commands/*.rs` (if needed)
- Modify: `teralaunch/src-tauri/src/utils/*.rs`

**Step 1: Mark test-only helpers as test-only**

For functions only used by tests, gate with `#[cfg(test)]` and move them into `#[cfg(test)] mod tests` blocks where possible.

**Step 2: For intentionally public but unused helpers, add explicit allow**

Use `#[allow(dead_code)]` with a brief reason comment when the function is part of an in-progress refactor but must remain.

**Step 3: Verify all warnings eliminated**

Run: `cargo check --manifest-path teralaunch/src-tauri/Cargo.toml`

Expected: Zero dead_code warnings.

### Task 5: Verify login is wired end-to-end in JS

**Files:**
- Modify: `teralaunch/src/app.js` (only if needed)

**Step 1: Ensure login response parsing aligns with backend JSON**

Confirm frontend expects `Msg: "success"` and `Return.AuthKey` etc.

**Step 2: If mismatch is found, adjust frontend to new response shape**

Make the minimal alignment change and re-run `cargo check`.

### Task 6: Summarize changes and next verification steps

**Files:**
- Modify: none

**Step 1: Provide summary and suggest runtime verification**

Run: `npm run tauri dev` (on Windows) and verify login works.

**Step 2: If login still fails, capture logs and iterate**

