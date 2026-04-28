// accountManager.js - Multi-account management module

const ACCOUNTS_KEY = 'tera_accounts';
const ACTIVE_ACCOUNT_KEY = 'active_account_id';
const INSTANCE_ID_KEY = 'launcher_instance_id';
const RUNNING_GAMES_KEY = 'running_games';

function normalizeUserNo(userNo) {
  return String(userNo);
}

function normalizeAccount(account) {
  return {
    ...account,
    userNo: normalizeUserNo(account.userNo),
  };
}

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
    return data ? JSON.parse(data).map(normalizeAccount) : [];
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
  localStorage.setItem(ACCOUNTS_KEY, JSON.stringify(accounts.map(normalizeAccount)));
}

/**
 * Add a new account to the list.
 * @param {Object} account - { userNo, userName, credentials, authMethod?, provider? }
 * @returns {boolean} true if added, false if duplicate
 */
export function addAccount(account) {
  const normalizedAccount = normalizeAccount(account);
  const accounts = getAccounts();
  const exists = accounts.some(a => a.userNo === normalizedAccount.userNo);
  if (exists) {
    return false;
  }
  const entry = {
    userNo: normalizedAccount.userNo,
    userName: normalizedAccount.userName,
    credentials: normalizedAccount.credentials,
    lastUsed: Date.now(),
  };
  if (normalizedAccount.authMethod) entry.authMethod = normalizedAccount.authMethod;
  if (normalizedAccount.provider) entry.provider = normalizedAccount.provider;
  accounts.push(entry);
  saveAccounts(accounts);
  return true;
}

/**
 * Remove an account from the list.
 * @param {string} userNo - Account ID to remove
 */
export function removeAccount(userNo) {
  const accountId = normalizeUserNo(userNo);
  const accounts = getAccounts().filter(a => a.userNo !== accountId);
  saveAccounts(accounts);

  // If removed account was active, clear active
  if (getActiveAccountId() === accountId) {
    clearActiveAccount();
  }
}

/**
 * Update an account's credentials.
 * @param {string} userNo - Account ID
 * @param {string} credentials - New base64 encoded credentials
 */
export function updateAccountCredentials(userNo, credentials) {
  const accountId = normalizeUserNo(userNo);
  const accounts = getAccounts();
  const account = accounts.find(a => a.userNo === accountId);
  if (account) {
    account.credentials = credentials;
    account.lastUsed = Date.now();
    saveAccounts(accounts);
  }
}

/**
 * Update an account's OAuth info (e.g., after re-auth via different provider).
 * @param {string} userNo - Account ID
 * @param {string} authMethod - 'password' or 'oauth'
 * @param {string|null} provider - OAuth provider name (null for password accounts)
 */
export function updateAccountAuthMethod(userNo, authMethod, provider = null) {
  const accountId = normalizeUserNo(userNo);
  const accounts = getAccounts();
  const account = accounts.find(a => a.userNo === accountId);
  if (account) {
    account.authMethod = authMethod;
    account.provider = provider;
    account.lastUsed = Date.now();
    saveAccounts(accounts);
  }
}

/**
 * Check if an account uses OAuth (no local credentials).
 * @param {string} userNo - Account ID
 * @returns {boolean} true if the account is OAuth-based
 */
export function isOAuthAccount(userNo) {
  const account = getAccount(userNo);
  return account?.authMethod === 'oauth';
}

/**
 * Get an account by userNo.
 * @param {string} userNo - Account ID
 * @returns {Object|null} Account object or null
 */
export function getAccount(userNo) {
  const accountId = normalizeUserNo(userNo);
  return getAccounts().find(a => a.userNo === accountId) || null;
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
  const accountId = normalizeUserNo(userNo);
  sessionStorage.setItem(ACTIVE_ACCOUNT_KEY, accountId);

  // Update lastUsed timestamp
  const accounts = getAccounts();
  const account = accounts.find(a => a.userNo === accountId);
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
 * Clear all running games for this launcher instance.
 * Called when backend reports no game processes running.
 */
export function clearAllRunningGames() {
  sessionStorage.setItem(RUNNING_GAMES_KEY, JSON.stringify({}));
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
