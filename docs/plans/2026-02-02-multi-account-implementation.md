# Multi-Account System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add multi-account management with per-instance isolation, account switching, and per-account game tracking.

**Architecture:** Frontend manages account list in localStorage, uses sessionStorage for instance isolation. Backend tracks running games per account. UI adds account dropdown to header with add/remove/switch functionality.

**Tech Stack:** Vanilla JS frontend, Rust/Tauri backend, localStorage/sessionStorage for persistence.

---

## Phase 1: Storage & Instance Isolation

### Task 1.1: Create Account Manager Module

**Files:**
- Create: `teralaunch/src/accountManager.js`

**Step 1: Create the account manager module with storage functions**

```javascript
// accountManager.js - Multi-account management module

const ACCOUNTS_KEY = 'tera_accounts';
const ACTIVE_ACCOUNT_KEY = 'active_account_id';
const INSTANCE_ID_KEY = 'launcher_instance_id';
const RUNNING_GAMES_KEY = 'running_games';

/**
 * Get or create a unique instance ID for this launcher window.
 * Uses sessionStorage so each window has its own ID.
 */
export function getInstanceId() {
  let instanceId = sessionStorage.getItem(INSTANCE_ID_KEY);
  if (!instanceId) {
    instanceId = crypto.randomUUID();
    sessionStorage.setItem(INSTANCE_ID_KEY, instanceId);
  }
  return instanceId;
}

/**
 * Get all saved accounts from localStorage.
 * @returns {Array} Array of account objects
 */
export function getAccounts() {
  try {
    const data = localStorage.getItem(ACCOUNTS_KEY);
    return data ? JSON.parse(data) : [];
  } catch (e) {
    console.error('Failed to parse accounts:', e);
    return [];
  }
}

/**
 * Save accounts array to localStorage.
 * @param {Array} accounts - Array of account objects
 */
export function saveAccounts(accounts) {
  localStorage.setItem(ACCOUNTS_KEY, JSON.stringify(accounts));
}

/**
 * Add a new account to the list.
 * @param {Object} account - { userNo, userName, credentials }
 * @returns {boolean} true if added, false if duplicate
 */
export function addAccount(account) {
  const accounts = getAccounts();
  const exists = accounts.some(a => a.userNo === account.userNo);
  if (exists) {
    return false;
  }
  accounts.push({
    userNo: account.userNo,
    userName: account.userName,
    credentials: account.credentials,
    lastUsed: Date.now()
  });
  saveAccounts(accounts);
  return true;
}

/**
 * Remove an account from the list.
 * @param {string} userNo - Account ID to remove
 */
export function removeAccount(userNo) {
  const accounts = getAccounts().filter(a => a.userNo !== userNo);
  saveAccounts(accounts);

  // If removed account was active, clear active
  if (getActiveAccountId() === userNo) {
    clearActiveAccount();
  }
}

/**
 * Update an account's credentials.
 * @param {string} userNo - Account ID
 * @param {string} credentials - New base64 encoded credentials
 */
export function updateAccountCredentials(userNo, credentials) {
  const accounts = getAccounts();
  const account = accounts.find(a => a.userNo === userNo);
  if (account) {
    account.credentials = credentials;
    account.lastUsed = Date.now();
    saveAccounts(accounts);
  }
}

/**
 * Get an account by userNo.
 * @param {string} userNo - Account ID
 * @returns {Object|null} Account object or null
 */
export function getAccount(userNo) {
  return getAccounts().find(a => a.userNo === userNo) || null;
}

/**
 * Get the active account ID for this launcher instance.
 * @returns {string|null} userNo or null
 */
export function getActiveAccountId() {
  return sessionStorage.getItem(ACTIVE_ACCOUNT_KEY);
}

/**
 * Set the active account for this launcher instance.
 * @param {string} userNo - Account ID
 */
export function setActiveAccountId(userNo) {
  sessionStorage.setItem(ACTIVE_ACCOUNT_KEY, userNo);

  // Update lastUsed timestamp
  const accounts = getAccounts();
  const account = accounts.find(a => a.userNo === userNo);
  if (account) {
    account.lastUsed = Date.now();
    saveAccounts(accounts);
  }
}

/**
 * Clear the active account.
 */
export function clearActiveAccount() {
  sessionStorage.removeItem(ACTIVE_ACCOUNT_KEY);
}

/**
 * Get the active account object.
 * @returns {Object|null} Account object or null
 */
export function getActiveAccount() {
  const activeId = getActiveAccountId();
  if (!activeId) return null;
  return getAccount(activeId);
}

/**
 * Get running games for this launcher instance.
 * @returns {Object} Map of userNo -> { processId, launchedAt }
 */
export function getRunningGames() {
  try {
    const data = sessionStorage.getItem(RUNNING_GAMES_KEY);
    return data ? JSON.parse(data) : {};
  } catch (e) {
    return {};
  }
}

/**
 * Register a running game for an account.
 * @param {string} userNo - Account ID
 * @param {number} processId - Game process ID
 */
export function registerRunningGame(userNo, processId) {
  const games = getRunningGames();
  games[userNo] = { processId, launchedAt: Date.now() };
  sessionStorage.setItem(RUNNING_GAMES_KEY, JSON.stringify(games));
}

/**
 * Unregister a running game.
 * @param {string} userNo - Account ID
 */
export function unregisterRunningGame(userNo) {
  const games = getRunningGames();
  delete games[userNo];
  sessionStorage.setItem(RUNNING_GAMES_KEY, JSON.stringify(games));
}

/**
 * Check if an account has a running game in this instance.
 * @param {string} userNo - Account ID
 * @returns {boolean}
 */
export function isAccountInGame(userNo) {
  const games = getRunningGames();
  return !!games[userNo];
}

/**
 * Migrate from old single-account storage to new multi-account format.
 * Call this once on app init.
 */
export function migrateFromLegacyStorage() {
  // Check if migration already done
  if (localStorage.getItem(ACCOUNTS_KEY)) {
    return;
  }

  // Check for legacy data
  const legacyUserNo = localStorage.getItem('userNo');
  const legacyUserName = localStorage.getItem('userName');
  const legacyCred = localStorage.getItem('_cred');

  if (legacyUserNo && legacyUserName && legacyCred) {
    // Migrate to new format
    const account = {
      userNo: legacyUserNo,
      userName: legacyUserName,
      credentials: legacyCred,
      lastUsed: Date.now()
    };
    saveAccounts([account]);
    setActiveAccountId(legacyUserNo);

    console.log('Migrated legacy account to multi-account format:', legacyUserName);
  }
}
```

**Step 2: Commit**

```bash
git add teralaunch/src/accountManager.js
git commit -m "feat: add account manager module for multi-account storage"
```

---

### Task 1.2: Integrate Account Manager into App Init

**Files:**
- Modify: `teralaunch/src/app.js`

**Step 1: Import account manager at top of app.js**

Add after other imports (around line 1):
```javascript
import * as AccountManager from './accountManager.js';
```

**Step 2: Call migration on app init**

In the `init()` method, add at the start:
```javascript
// Migrate legacy single-account storage to multi-account
AccountManager.migrateFromLegacyStorage();
AccountManager.getInstanceId(); // Ensure instance ID exists
```

**Step 3: Update storeAuthInfo to use AccountManager**

Find `storeAuthInfo` method and update to also add/update account:
```javascript
// After storing to localStorage, also update AccountManager
const credentials = btoa(JSON.stringify({ u: username, p: password }));
AccountManager.addAccount({
  userNo: response.user_no,
  userName: response.user_name,
  credentials: credentials
});
AccountManager.setActiveAccountId(response.user_no);
```

**Step 4: Commit**

```bash
git add teralaunch/src/app.js
git commit -m "feat: integrate account manager into app initialization"
```

---

## Phase 2: Account Manager UI

### Task 2.1: Add Account Dropdown HTML/CSS

**Files:**
- Modify: `teralaunch/src/index.html`

**Step 1: Add CSS for account dropdown (in style section)**

Add after `.window-controls` styles:
```css
/* ============================================
   ACCOUNT MANAGER DROPDOWN
   ============================================ */
.account-manager {
    position: relative;
    display: flex;
    align-items: center;
    -webkit-app-region: no-drag;
}

.account-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 12px;
    font-size: 14px;
    font-weight: 500;
    color: var(--foreground);
    background: transparent;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    transition: background-color 0.15s ease;
}

.account-btn:hover {
    background: rgba(255, 255, 255, 0.05);
}

.account-status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #22c55e;
    display: none;
}

.account-status-dot.playing {
    display: block;
}

.account-btn-name {
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.account-btn-arrow {
    width: 14px;
    height: 14px;
    color: var(--muted-foreground);
    transition: transform 0.2s ease;
}

.account-manager.open .account-btn-arrow {
    transform: rotate(180deg);
}

.account-dropdown {
    display: none;
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    min-width: 200px;
    background: var(--background);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    overflow: hidden;
    z-index: 2001;
}

.account-manager.open .account-dropdown {
    display: block;
}

.account-dropdown-list {
    max-height: 200px;
    overflow-y: auto;
}

.account-dropdown-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 10px 14px;
    font-size: 14px;
    color: var(--foreground);
    background: none;
    border: none;
    cursor: pointer;
    transition: background-color 0.15s ease;
    text-align: left;
}

.account-dropdown-item:hover {
    background: var(--accent);
}

.account-dropdown-item:hover .account-delete-btn {
    opacity: 1;
}

.account-item-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.account-item-badge {
    font-size: 11px;
    padding: 2px 6px;
    background: rgba(34, 197, 94, 0.2);
    color: #22c55e;
    border-radius: 4px;
}

.account-delete-btn {
    width: 24px;
    height: 24px;
    padding: 4px;
    color: var(--muted-foreground);
    background: none;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s ease, color 0.15s ease, background-color 0.15s ease;
}

.account-delete-btn:hover {
    color: var(--destructive);
    background: rgba(220, 38, 38, 0.1);
}

.account-dropdown-divider {
    height: 1px;
    background: var(--border);
    margin: 4px 0;
}

.account-add-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 10px 14px;
    font-size: 14px;
    color: var(--primary);
    background: none;
    border: none;
    cursor: pointer;
    transition: background-color 0.15s ease;
}

.account-add-btn:hover {
    background: var(--accent);
}

.account-add-btn svg {
    width: 16px;
    height: 16px;
}

/* Add Account Modal */
#add-account-modal {
    display: none;
    position: fixed;
    inset: 0;
    z-index: 3000;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.7);
    backdrop-filter: blur(8px);
}

#add-account-modal.show {
    display: flex;
}

.add-account-content {
    width: 100%;
    max-width: 360px;
    padding: 24px;
    background: var(--background);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.5);
}

.add-account-title {
    font-size: 18px;
    font-weight: 600;
    color: var(--foreground);
    margin: 0 0 16px 0;
}

.add-account-form {
    display: flex;
    flex-direction: column;
    gap: 12px;
}

.add-account-input {
    height: 40px;
    padding: 0 12px;
    font-size: 14px;
    color: var(--foreground);
    background: rgba(26, 31, 40, 0.5);
    border: 1px solid var(--border);
    border-radius: 6px;
    outline: none;
    transition: border-color 0.2s ease;
}

.add-account-input:focus {
    border-color: var(--primary);
}

.add-account-input::placeholder {
    color: var(--muted-foreground);
}

.add-account-error {
    font-size: 13px;
    color: var(--destructive);
    display: none;
}

.add-account-error.show {
    display: block;
}

.add-account-buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
}

.add-account-cancel {
    height: 36px;
    padding: 0 16px;
    font-size: 14px;
    color: var(--muted-foreground);
    background: none;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    transition: color 0.15s ease, background-color 0.15s ease;
}

.add-account-cancel:hover {
    color: var(--foreground);
    background: rgba(255, 255, 255, 0.05);
}

.add-account-submit {
    height: 36px;
    padding: 0 20px;
    font-size: 14px;
    font-weight: 600;
    color: var(--primary-foreground);
    background: var(--primary);
    border: none;
    border-radius: 6px;
    cursor: pointer;
    transition: opacity 0.15s ease;
}

.add-account-submit:hover {
    opacity: 0.9;
}

.add-account-submit:disabled {
    opacity: 0.5;
    cursor: not-allowed;
}

/* Delete Confirmation Modal */
#delete-account-modal {
    display: none;
    position: fixed;
    inset: 0;
    z-index: 3001;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.7);
    backdrop-filter: blur(8px);
}

#delete-account-modal.show {
    display: flex;
}

.delete-account-content {
    width: 100%;
    max-width: 320px;
    padding: 24px;
    background: var(--background);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.5);
    text-align: center;
}

.delete-account-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--foreground);
    margin: 0 0 8px 0;
}

.delete-account-message {
    font-size: 14px;
    color: var(--muted-foreground);
    margin: 0 0 20px 0;
}

.delete-account-buttons {
    display: flex;
    justify-content: center;
    gap: 8px;
}

.delete-account-cancel {
    height: 36px;
    padding: 0 16px;
    font-size: 14px;
    color: var(--muted-foreground);
    background: none;
    border: none;
    border-radius: 6px;
    cursor: pointer;
}

.delete-account-confirm {
    height: 36px;
    padding: 0 16px;
    font-size: 14px;
    font-weight: 600;
    color: #fff;
    background: var(--destructive);
    border: none;
    border-radius: 6px;
    cursor: pointer;
}
```

**Step 2: Add HTML for account dropdown (replace user-display div)**

Find the `user-display` div in header-right and replace with:
```html
<!-- Account Manager -->
<div class="account-manager" id="account-manager">
    <button class="account-btn" id="account-btn">
        <span class="account-status-dot" id="account-status-dot"></span>
        <span class="account-btn-name" id="account-btn-name">Not logged in</span>
        <svg class="account-btn-arrow" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="6 9 12 15 18 9"></polyline>
        </svg>
    </button>
    <div class="account-dropdown" id="account-dropdown">
        <div class="account-dropdown-list" id="account-dropdown-list">
            <!-- Populated by JS -->
        </div>
        <div class="account-dropdown-divider"></div>
        <button class="account-add-btn" id="account-add-btn">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="12" y1="5" x2="12" y2="19"></line>
                <line x1="5" y1="12" x2="19" y2="12"></line>
            </svg>
            Add Account
        </button>
    </div>
</div>
```

**Step 3: Add HTML for modals (before closing body tag)**

```html
<!-- Add Account Modal -->
<div id="add-account-modal">
    <div class="add-account-content">
        <h3 class="add-account-title">Add Account</h3>
        <div class="add-account-form">
            <input type="text" class="add-account-input" id="add-account-username" placeholder="Username">
            <input type="password" class="add-account-input" id="add-account-password" placeholder="Password">
            <p class="add-account-error" id="add-account-error"></p>
            <div class="add-account-buttons">
                <button class="add-account-cancel" id="add-account-cancel">Cancel</button>
                <button class="add-account-submit" id="add-account-submit">Add Account</button>
            </div>
        </div>
    </div>
</div>

<!-- Delete Account Confirmation Modal -->
<div id="delete-account-modal">
    <div class="delete-account-content">
        <h3 class="delete-account-title">Remove Account?</h3>
        <p class="delete-account-message" id="delete-account-message">Remove this account from the launcher?</p>
        <div class="delete-account-buttons">
            <button class="delete-account-cancel" id="delete-account-cancel">Cancel</button>
            <button class="delete-account-confirm" id="delete-account-confirm">Remove</button>
        </div>
    </div>
</div>
```

**Step 4: Commit**

```bash
git add teralaunch/src/index.html
git commit -m "feat: add account manager dropdown HTML and CSS"
```

---

### Task 2.2: Add Account Manager UI Logic

**Files:**
- Modify: `teralaunch/src/app.js`

**Step 1: Add account manager UI methods to TeraApp object**

Add these methods to the TeraApp object:

```javascript
// ========== ACCOUNT MANAGER UI ==========

/**
 * Initialize account manager UI and event listeners.
 */
initAccountManager() {
  const accountBtn = document.getElementById('account-btn');
  const accountManager = document.getElementById('account-manager');
  const accountAddBtn = document.getElementById('account-add-btn');

  if (!accountBtn || !accountManager) return;

  // Toggle dropdown
  accountBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    accountManager.classList.toggle('open');
    if (accountManager.classList.contains('open')) {
      this.renderAccountDropdown();
    }
  });

  // Close on outside click
  document.addEventListener('click', (e) => {
    if (!accountManager.contains(e.target)) {
      accountManager.classList.remove('open');
    }
  });

  // Add account button
  accountAddBtn.addEventListener('click', () => {
    accountManager.classList.remove('open');
    this.openAddAccountModal();
  });

  // Setup modals
  this.setupAddAccountModal();
  this.setupDeleteAccountModal();

  // Initial render
  this.updateAccountDisplay();
},

/**
 * Render the account dropdown list (excluding active account).
 */
renderAccountDropdown() {
  const list = document.getElementById('account-dropdown-list');
  if (!list) return;

  const accounts = AccountManager.getAccounts();
  const activeId = AccountManager.getActiveAccountId();
  const otherAccounts = accounts.filter(a => a.userNo !== activeId);

  if (otherAccounts.length === 0) {
    list.innerHTML = '<div style="padding: 10px 14px; color: var(--muted-foreground); font-size: 13px;">No other accounts</div>';
    return;
  }

  list.innerHTML = otherAccounts.map(account => {
    const isPlaying = AccountManager.isAccountInGame(account.userNo);
    return `
      <div class="account-dropdown-item" data-user-no="${account.userNo}">
        <span class="account-item-name">${account.userName}</span>
        ${isPlaying ? '<span class="account-item-badge">Playing</span>' : ''}
        <button class="account-delete-btn" data-user-no="${account.userNo}" title="Remove account">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M3 6h18M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"></path>
          </svg>
        </button>
      </div>
    `;
  }).join('');

  // Add click handlers for switching
  list.querySelectorAll('.account-dropdown-item').forEach(item => {
    item.addEventListener('click', async (e) => {
      if (e.target.closest('.account-delete-btn')) return;
      const userNo = item.dataset.userNo;
      await this.switchAccount(userNo);
      document.getElementById('account-manager').classList.remove('open');
    });
  });

  // Add click handlers for delete
  list.querySelectorAll('.account-delete-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const userNo = btn.dataset.userNo;
      const account = AccountManager.getAccount(userNo);
      this.openDeleteAccountModal(account);
    });
  });
},

/**
 * Update the account button display with current account info.
 */
updateAccountDisplay() {
  const btnName = document.getElementById('account-btn-name');
  const statusDot = document.getElementById('account-status-dot');

  if (!btnName) return;

  const activeAccount = AccountManager.getActiveAccount();

  if (activeAccount) {
    btnName.textContent = activeAccount.userName;
    const isPlaying = AccountManager.isAccountInGame(activeAccount.userNo);
    statusDot.classList.toggle('playing', isPlaying);
  } else {
    btnName.textContent = 'Not logged in';
    statusDot.classList.remove('playing');
  }
},

/**
 * Switch to a different account.
 */
async switchAccount(userNo) {
  const account = AccountManager.getAccount(userNo);
  if (!account) return;

  AccountManager.setActiveAccountId(userNo);

  // Do silent auth refresh
  try {
    const cred = JSON.parse(atob(account.credentials));
    const success = await this.silentAuthRefresh(cred.u, cred.p);
    if (!success) {
      this.openAddAccountModal(account.userName); // Pre-fill username for re-auth
      window.showUpdateNotification('error', this.t('LOGIN_FAILED'), this.t('PLEASE_REENTER_PASSWORD'));
      return;
    }
  } catch (e) {
    console.error('Failed to switch account:', e);
    window.showUpdateNotification('error', this.t('ERROR'), 'Failed to switch account');
    return;
  }

  this.updateAccountDisplay();
  this.updateLaunchButtonState();
},

/**
 * Update launch button based on whether active account has running game.
 */
updateLaunchButtonState() {
  const launchBtn = document.getElementById('launch-game-btn');
  if (!launchBtn) return;

  const activeAccount = AccountManager.getActiveAccount();
  if (!activeAccount) {
    launchBtn.disabled = true;
    return;
  }

  const isPlaying = AccountManager.isAccountInGame(activeAccount.userNo);
  if (isPlaying) {
    launchBtn.disabled = true;
    launchBtn.textContent = this.t('IN_GAME') || 'In Game';
  } else {
    // Let existing logic handle enabled/disabled based on update status
    this.updateLaunchGameButton();
  }
},

// ========== ADD ACCOUNT MODAL ==========

setupAddAccountModal() {
  const modal = document.getElementById('add-account-modal');
  const cancelBtn = document.getElementById('add-account-cancel');
  const submitBtn = document.getElementById('add-account-submit');
  const usernameInput = document.getElementById('add-account-username');
  const passwordInput = document.getElementById('add-account-password');

  if (!modal) return;

  cancelBtn.addEventListener('click', () => this.closeAddAccountModal());

  modal.addEventListener('click', (e) => {
    if (e.target === modal) this.closeAddAccountModal();
  });

  submitBtn.addEventListener('click', () => this.handleAddAccount());

  // Enter key to submit
  passwordInput.addEventListener('keypress', (e) => {
    if (e.key === 'Enter') this.handleAddAccount();
  });
},

openAddAccountModal(prefillUsername = '') {
  const modal = document.getElementById('add-account-modal');
  const usernameInput = document.getElementById('add-account-username');
  const passwordInput = document.getElementById('add-account-password');
  const errorEl = document.getElementById('add-account-error');

  usernameInput.value = prefillUsername;
  passwordInput.value = '';
  errorEl.textContent = '';
  errorEl.classList.remove('show');

  modal.classList.add('show');

  if (prefillUsername) {
    passwordInput.focus();
  } else {
    usernameInput.focus();
  }
},

closeAddAccountModal() {
  const modal = document.getElementById('add-account-modal');
  modal.classList.remove('show');
},

async handleAddAccount() {
  const usernameInput = document.getElementById('add-account-username');
  const passwordInput = document.getElementById('add-account-password');
  const errorEl = document.getElementById('add-account-error');
  const submitBtn = document.getElementById('add-account-submit');

  const username = usernameInput.value.trim();
  const password = passwordInput.value;

  if (!username || !password) {
    errorEl.textContent = 'Please enter username and password';
    errorEl.classList.add('show');
    return;
  }

  submitBtn.disabled = true;
  errorEl.classList.remove('show');

  try {
    const response = await invoke('login', { username, password });

    if (response && response.success) {
      // Check if account already exists
      const existingAccounts = AccountManager.getAccounts();
      const exists = existingAccounts.some(a => a.userNo === response.user_no);

      if (exists) {
        // Update credentials for existing account
        const credentials = btoa(JSON.stringify({ u: username, p: password }));
        AccountManager.updateAccountCredentials(response.user_no, credentials);
      } else {
        // Add new account
        const credentials = btoa(JSON.stringify({ u: username, p: password }));
        AccountManager.addAccount({
          userNo: response.user_no,
          userName: response.user_name,
          credentials: credentials
        });
      }

      // Set as active and update backend state
      AccountManager.setActiveAccountId(response.user_no);
      await invoke('set_auth_info', {
        authKey: response.auth_key,
        userName: response.user_name,
        userNo: response.user_no,
        characterCount: response.character_count
      });

      this.setState({ isAuthenticated: true });
      this.updateAccountDisplay();
      this.updateLaunchButtonState();
      this.closeAddAccountModal();

      window.showUpdateNotification('success', 'Account Added', response.user_name);
    } else {
      errorEl.textContent = response?.error || 'Login failed';
      errorEl.classList.add('show');
    }
  } catch (e) {
    errorEl.textContent = e.toString();
    errorEl.classList.add('show');
  } finally {
    submitBtn.disabled = false;
  }
},

// ========== DELETE ACCOUNT MODAL ==========

setupDeleteAccountModal() {
  const modal = document.getElementById('delete-account-modal');
  const cancelBtn = document.getElementById('delete-account-cancel');
  const confirmBtn = document.getElementById('delete-account-confirm');

  if (!modal) return;

  cancelBtn.addEventListener('click', () => this.closeDeleteAccountModal());
  confirmBtn.addEventListener('click', () => this.confirmDeleteAccount());

  modal.addEventListener('click', (e) => {
    if (e.target === modal) this.closeDeleteAccountModal();
  });
},

openDeleteAccountModal(account) {
  this._accountToDelete = account;
  const modal = document.getElementById('delete-account-modal');
  const message = document.getElementById('delete-account-message');

  message.textContent = `Remove "${account.userName}" from the launcher?`;
  modal.classList.add('show');
},

closeDeleteAccountModal() {
  const modal = document.getElementById('delete-account-modal');
  modal.classList.remove('show');
  this._accountToDelete = null;
},

confirmDeleteAccount() {
  if (!this._accountToDelete) return;

  AccountManager.removeAccount(this._accountToDelete.userNo);
  this.closeDeleteAccountModal();

  // If we deleted the active account, switch to another or show login
  if (!AccountManager.getActiveAccount()) {
    const accounts = AccountManager.getAccounts();
    if (accounts.length > 0) {
      this.switchAccount(accounts[0].userNo);
    } else {
      this.setState({ isAuthenticated: false });
      this.updateAccountDisplay();
    }
  }

  this.renderAccountDropdown();
  window.showUpdateNotification('info', 'Account Removed', this._accountToDelete.userName);
},
```

**Step 2: Call initAccountManager in init()**

In the `init()` method, add after other initialization:
```javascript
this.initAccountManager();
```

**Step 3: Commit**

```bash
git add teralaunch/src/app.js
git commit -m "feat: add account manager UI logic and modals"
```

---

## Phase 3: Game Launch Integration

### Task 3.1: Update Game Launch to Track Account

**Files:**
- Modify: `teralaunch/src/app.js`

**Step 1: Update launchGame to register running game**

In the game launch success handler, after getting process ID:
```javascript
// Register this game as running for this account
const activeAccount = AccountManager.getActiveAccount();
if (activeAccount) {
  AccountManager.registerRunningGame(activeAccount.userNo, processId);
  this.updateAccountDisplay();
  this.updateLaunchButtonState();
}
```

**Step 2: Update game exit handler to unregister**

In the game status change handler (when game exits):
```javascript
// Unregister running game
const activeAccount = AccountManager.getActiveAccount();
if (activeAccount) {
  AccountManager.unregisterRunningGame(activeAccount.userNo);
  this.updateAccountDisplay();
  this.updateLaunchButtonState();
}
```

**Step 3: Update pre-launch auth check**

Before launching, check if account already in game:
```javascript
const activeAccount = AccountManager.getActiveAccount();
if (activeAccount && AccountManager.isAccountInGame(activeAccount.userNo)) {
  window.showUpdateNotification('warning', 'Already Running', 'This account already has a game running');
  return;
}
```

**Step 4: Update silent auth refresh failure handling**

If silent auth fails before launch:
```javascript
const activeAccount = AccountManager.getActiveAccount();
if (activeAccount) {
  try {
    const cred = JSON.parse(atob(activeAccount.credentials));
    const success = await this.silentAuthRefresh(cred.u, cred.p);
    if (!success) {
      window.showUpdateNotification('error', this.t('LOGIN_FAILED'), this.t('PLEASE_REENTER_PASSWORD'));
      this.openAddAccountModal(activeAccount.userName);
      return; // Don't launch
    }
  } catch (e) {
    window.showUpdateNotification('error', this.t('LOGIN_FAILED'), this.t('PLEASE_REENTER_PASSWORD'));
    this.openAddAccountModal(activeAccount.userName);
    return;
  }
}
```

**Step 5: Commit**

```bash
git add teralaunch/src/app.js
git commit -m "feat: integrate game launch with account tracking"
```

---

### Task 3.2: Add Translation Keys

**Files:**
- Modify: `teralaunch/src/translations.json`

**Step 1: Add new translation keys to all languages**

Add to EUR (English):
```json
"IN_GAME": "In Game",
"LOGIN_FAILED": "Login Failed",
"PLEASE_REENTER_PASSWORD": "Please re-enter your password",
"ACCOUNT_ADDED": "Account Added",
"ACCOUNT_REMOVED": "Account Removed",
"ALREADY_RUNNING": "Already Running",
"ACCOUNT_ALREADY_RUNNING": "This account already has a game running"
```

Add equivalent translations for GER, FRA, RUS.

**Step 2: Commit**

```bash
git add teralaunch/src/translations.json
git commit -m "feat: add translation keys for multi-account system"
```

---

### Task 3.3: Final Integration and Testing

**Step 1: Test migration from legacy storage**
- Have old format data in localStorage
- Load app, verify migration creates account entry
- Verify active account is set

**Step 2: Test account switching**
- Add multiple accounts
- Switch between them
- Verify auth state updates
- Verify launch button reflects correct account's status

**Step 3: Test game launch tracking**
- Launch game for Account A
- Verify Account A shows "Playing" badge
- Verify launch button disabled for Account A
- Switch to Account B, verify launch enabled
- Launch game for Account B
- Both should show as playing

**Step 4: Test auth failure handling**
- Change password externally
- Try to launch game
- Verify error toast and re-auth modal appears
- Verify game does NOT launch

**Step 5: Final commit**

```bash
git add -A
git commit -m "feat: complete multi-account system implementation"
```

---

## Summary

| Phase | Tasks | Estimated Complexity |
|-------|-------|---------------------|
| Phase 1 | Storage & Isolation | Medium |
| Phase 2 | Account Manager UI | Medium-High |
| Phase 3 | Game Launch Integration | Medium |

Total: ~8 tasks with incremental commits.
