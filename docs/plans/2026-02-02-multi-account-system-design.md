# Multi-Account System Design

**Date:** 2026-02-02
**Status:** Approved

## Problem Statement

When multiple launcher instances are open with different accounts, the shared localStorage causes session mixup. Launcher A's "silent auth refresh" before game launch reads Launcher B's credentials, launching the wrong account.

Additionally, users want to manage multiple accounts from a single launcher window.

## Goals

1. Fix the localStorage isolation bug between launcher instances
2. Add account manager UI for switching between saved accounts
3. Track running game clients per account
4. Support multi-client from single launcher (one game per account)

## Design

### Data Storage

**Account List (localStorage)**
```javascript
// Key: "tera_accounts"
[
  {
    userNo: "12345",
    userName: "PlayerOne",
    credentials: "base64_encoded_u:p",  // for silent re-auth
    lastUsed: 1706900000000
  },
  {
    userNo: "67890",
    userName: "AltAccount",
    credentials: "base64_encoded_u:p",
    lastUsed: 1706800000000
  }
]
```

**Instance Isolation (sessionStorage)**
```javascript
// Key: "launcher_instance_id"
// Value: crypto.randomUUID() - generated on page load
// Per-window, so multiple launcher windows don't conflict

// Key: "active_account_id"
// Value: userNo of currently selected account
// Per-window, each launcher instance can have different account selected

// Key: "running_games"
// Value: { "12345": { processId: 1234, launchedAt: timestamp } }
// Only games launched by THIS launcher window
```

### Account Manager UI

**Header Account Display**
- Replaces current username-only display
- Shows: [green dot if playing] + username + dropdown chevron
- Click opens dropdown

**Dropdown Contents**
- Lists OTHER accounts (not the currently selected one)
- Each row: account name + "Playing" badge if in-game + delete button (on hover)
- Bottom: "+ Add Account" button

**Add Account Modal**
- Small centered modal (matches first-launch modal styling)
- Username and password fields
- Cancel and "Add Account" buttons
- On success: account added, becomes selected, modal closes

**Remove Account**
- Hover over account row reveals delete (trash) icon
- Click shows confirmation dialog
- Always requires confirmation

**In-Game Status Display**
- Selected account: green dot next to username + launch button disabled with "In Game" text
- Dropdown accounts: "Playing" badge shown on row

### Backend Changes

**Game State Extension**
```rust
pub struct RunningGame {
    pub process_id: u32,
    pub account_id: String,  // userNo
    pub launched_at: u64,
}

// GameState gains:
pub running_games: Arc<Mutex<HashMap<String, RunningGame>>>
```

**New Commands**
```rust
#[tauri::command]
pub fn register_running_game(account_id: String, process_id: u32) -> Result<(), String>

#[tauri::command]
pub fn unregister_running_game(account_id: String) -> Result<(), String>

#[tauri::command]
pub fn get_running_games() -> Result<HashMap<String, RunningGame>, String>

#[tauri::command]
pub fn is_process_alive(process_id: u32) -> Result<bool, String>
```

### Frontend Flows

**On App Load**
1. Generate instance ID if not exists → sessionStorage
2. Load accounts from localStorage
3. Get active account from sessionStorage (or default to first/last-used)
4. If active account exists → silent auth refresh with its credentials
5. Update UI

**On Account Switch**
1. Set sessionStorage.active_account_id
2. Silent auth refresh with new account's credentials
3. Update backend auth state via set_auth_info()
4. Update header display
5. Update launch button state

**On Login (Add Account)**
1. Call login() command
2. Add account to localStorage.tera_accounts
3. Set as active account
4. Update UI

**On Game Launch**
1. Check if active account has running game → block if yes
2. Silent auth refresh
3. Launch game, get process ID
4. Register running game
5. Update UI (green dot, disable launch)

**On Game Exit**
1. Remove from running_games
2. Update UI (remove green dot, enable launch)

### Error Handling

**Silent Auth Refresh Fails**
- Toast: "Session expired for [username]. Please re-login."
- Keep account but mark as needing re-auth

**No Accounts / All Removed**
- Show login form directly (current fresh-install behavior)

**Process Monitoring**
- Game crash → detected by polling → update UI
- Launcher closes → game continues (no tracking needed)

**Duplicate Account**
- Error: "This account is already added"

### Cross-Launcher Behavior

- Each launcher tracks only its own launched games
- Multiple launchers can launch same account (server handles conflicts)
- No IPC between launcher instances

## Implementation Notes

- Use existing modal styling from first-launch modal
- Use existing dropdown patterns from language selector
- Minimal backend changes - frontend manages account list
- Backend only needs active session for game launch
