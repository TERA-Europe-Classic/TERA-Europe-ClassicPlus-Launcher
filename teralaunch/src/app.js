import { calculateResumeSnapshot } from "./utils/download.js";
import {
  getDlStatusKey,
  getProgressUpdateMode,
  getStatusKey,
  getUpdateErrorMessage,
  getPathChangeResetState,
  shouldDisableLaunch,
} from "./utils/updateState.js";
import { localizeForumUrl } from "./utils/forumLinks.js";
import * as AccountManager from './accountManager.js';

const { invoke } = window.__TAURI__.tauri;
const { listen } = window.__TAURI__.event;
const { appWindow, WebviewWindow } = window.__TAURI__.window;
const { message, ask } = window.__TAURI__.dialog;

/**
 * Application URL configuration.
 * Centralized location for all external URLs used by the launcher.
 */
const URLS = {
  // Launcher update endpoints
  // Classic+ TODO: Re-enable when launcher update infrastructure is available
  launcher: {
    download: "",
    versionCheck: "",
    versionInfo: "",
  },

  // Game content endpoints
  // Classic+ TODO: Re-enable news/patchNotes when endpoints are available
  content: {
    news: "",
    patchNotes: "",
    serverStatus: "http://192.168.1.128:8090/tera/ServerList?lang=en",
  },

  // External links
  // Classic+ TODO: Re-enable registration, forum, privacy, profile when available
  external: {
    register: "",
    forum: "",
    discord: "https://discord.com/invite/crazyesports",
    support: "https://helpdesk.crazy-esports.com",
    privacy: "",
    profile: "",
  },
  // Classic+ NOTE: Leaderboard consent removed -- no leaderboard API on v100
};

const REQUIRED_PRIVILEGE_LEVEL = 3;
const UPDATE_CHECK_ENABLED = true;
// Local launcher release date used for update comparisons
const CURRENT_RELEASE_DATE = "2024-06-07";

// ========== HOME PAGE STATUS UI FUNCTIONS ==========
// These functions update the status area and launch button in home.html.
// They must be in app.js (not home.html) because innerHTML doesn't execute scripts.

// Helper to get translation - uses App.t() if available, otherwise returns key
function getTranslation(key) {
  if (window.App && typeof App.t === 'function') {
    return App.t(key);
  }
  // Fallback defaults for when App isn't ready yet
  const fallbacks = {
    'CHECKING_BTN': 'CHECKING',
    'LAUNCH_GAME': 'LAUNCH',
    'DOWNLOADING_FILES': 'Downloading...',
    'PAUSED': 'Paused',
    'INITIALIZING': 'Initializing...',
    'FILES': 'files'
  };
  return fallbacks[key] || key;
}

function hideAllStatusStates() {
  document.querySelectorAll('#home-status-area .status-state').forEach(s => {
    s.classList.remove('active');
  });
}
window.hideAllStatusStates = hideAllStatusStates;

function showCheckingState(checked, total) {
  hideAllStatusStates();
  const statusChecking = document.getElementById('status-checking');
  if (statusChecking) statusChecking.classList.add('active');

  // Show spinner on launch button
  const playIcon = document.getElementById('launch-play-icon');
  const spinnerIcon = document.getElementById('launch-spinner-icon');
  const btnText = document.getElementById('launch-btn-text');
  const launchBtn = document.getElementById('launch-game-btn');

  if (playIcon) playIcon.classList.add('hidden-icon');
  if (spinnerIcon) spinnerIcon.classList.remove('hidden-icon');
  if (btnText) btnText.textContent = getTranslation('CHECKING_BTN');
  if (launchBtn) {
    launchBtn.classList.add('disabled');
    launchBtn.disabled = true;
  }

  // Hide pause button
  const pauseBtn = document.getElementById('btn-pause-resume');
  if (pauseBtn) pauseBtn.classList.remove('active');

  updateCheckingProgress(checked, total);
}
window.showCheckingState = showCheckingState;

function updateCheckingProgress(checked, total) {
  const checkedEl = document.getElementById('checked-files');
  const totalEl = document.getElementById('total-files');
  const progressBar = document.getElementById('check-progress-bar');
  const percentEl = document.getElementById('check-percent');
  const progressFooter = document.querySelector('#status-checking .progress-footer');

  const checkedNum = checked || 0;
  const totalNum = total || 0;
  const percent = totalNum > 0 ? Math.round((checkedNum / totalNum) * 100) : 0;

  // Hide file count when initializing (0/0), show "Initializing..."
  if (totalNum === 0) {
    if (progressFooter) progressFooter.innerHTML = '<span>Initializing...</span>';
    if (percentEl) percentEl.textContent = '';
  } else {
    if (progressFooter) {
      progressFooter.innerHTML = `<span><span id="checked-files">${checkedNum.toLocaleString()}</span> / <span id="total-files">${totalNum.toLocaleString()}</span> files</span>`;
    }
    if (percentEl) percentEl.textContent = `${percent}%`;
  }
  if (progressBar) progressBar.style.width = `${percent}%`;
}
window.updateCheckingProgress = updateCheckingProgress;

function hideCheckingState() {
  // Clear all status states first to ensure clean transition
  hideAllStatusStates();

  // Reset launch button icons
  const playIcon = document.getElementById('launch-play-icon');
  const spinnerIcon = document.getElementById('launch-spinner-icon');
  const btnText = document.getElementById('launch-btn-text');

  if (playIcon) playIcon.classList.remove('hidden-icon');
  if (spinnerIcon) spinnerIcon.classList.add('hidden-icon');
  if (btnText) btnText.textContent = getTranslation('LAUNCH_GAME');

  // Show appropriate status based on authentication
  const isAuthenticated = localStorage.getItem('authKey') !== null;
  const statusLoginRequired = document.getElementById('status-login-required');
  const statusReady = document.getElementById('status-ready');
  const launchBtn = document.getElementById('launch-game-btn');

  if (isAuthenticated) {
    if (statusReady) statusReady.classList.add('active');
    if (launchBtn) {
      launchBtn.classList.remove('disabled');
      launchBtn.disabled = false;
    }
  } else {
    if (statusLoginRequired) statusLoginRequired.classList.add('active');
    if (launchBtn) {
      launchBtn.classList.add('disabled');
      launchBtn.disabled = true;
    }
  }
}
window.hideCheckingState = hideCheckingState;

function showDownloadingState() {
  hideAllStatusStates();
  const statusDownloading = document.getElementById('status-downloading');
  if (statusDownloading) statusDownloading.classList.add('active');

  // Show pause button
  const pauseBtn = document.getElementById('btn-pause-resume');
  if (pauseBtn) pauseBtn.classList.add('active');

  // Update pause button to show pause icon
  const pauseIcon = document.getElementById('pause-icon');
  const resumeIcon = document.getElementById('resume-icon');
  if (pauseIcon) pauseIcon.classList.remove('hidden-icon');
  if (resumeIcon) resumeIcon.classList.add('hidden-icon');

  // Update download label
  const dlLabel = document.getElementById('dl-status-label');
  if (dlLabel) dlLabel.textContent = getTranslation('DOWNLOADING_FILES');

  // Show download speed (may have been hidden when paused)
  const speedEl = document.getElementById('download-speed');
  if (speedEl) speedEl.style.display = '';

  // Disable launch button during download
  const launchBtn = document.getElementById('launch-game-btn');
  const playIcon = document.getElementById('launch-play-icon');
  const spinnerIcon = document.getElementById('launch-spinner-icon');
  const btnText = document.getElementById('launch-btn-text');

  if (launchBtn) {
    launchBtn.classList.add('disabled');
    launchBtn.disabled = true;
  }
  if (playIcon) playIcon.classList.remove('hidden-icon');
  if (spinnerIcon) spinnerIcon.classList.add('hidden-icon');
  if (btnText) btnText.textContent = getTranslation('LAUNCH_GAME');
}
window.showDownloadingState = showDownloadingState;

function updateDownloadProgress(progress, downloaded, total, speed) {
  const progressBar = document.getElementById('progress-percentage-div');
  const percentEl = document.getElementById('progress-percentage');
  const downloadedEl = document.getElementById('downloaded-size');
  const totalEl = document.getElementById('total-size');
  const speedEl = document.getElementById('download-speed');

  const percent = Math.round(progress || 0);

  if (progressBar) progressBar.style.width = `${percent}%`;
  if (percentEl) percentEl.textContent = `${percent}%`;
  if (downloadedEl) downloadedEl.textContent = downloaded || '0';
  if (totalEl) totalEl.textContent = total || '0';
  if (speedEl) speedEl.textContent = speed || '--';
}
window.updateDownloadProgress = updateDownloadProgress;

function showPausedState() {
  // Keep downloading state visible but update UI
  const dlLabel = document.getElementById('dl-status-label');
  if (dlLabel) dlLabel.textContent = getTranslation('PAUSED');

  // Show resume icon on pause button
  const pauseIcon = document.getElementById('pause-icon');
  const resumeIcon = document.getElementById('resume-icon');
  if (pauseIcon) pauseIcon.classList.add('hidden-icon');
  if (resumeIcon) resumeIcon.classList.remove('hidden-icon');

  // Hide download speed when paused (shadcn behavior)
  const speedEl = document.getElementById('download-speed');
  if (speedEl) speedEl.style.display = 'none';
}
window.showPausedState = showPausedState;

/**
 * Initializes the status UI based on current authentication state.
 * Called at app startup to ensure correct initial state is shown.
 * This replaces the HTML default state with the appropriate state.
 */
function initializeStatusUI() {
  const isAuthenticated = localStorage.getItem('authKey') !== null;
  hideAllStatusStates();

  const statusLoginRequired = document.getElementById('status-login-required');
  const statusReady = document.getElementById('status-ready');
  const launchBtn = document.getElementById('launch-game-btn');
  const playIcon = document.getElementById('launch-play-icon');
  const spinnerIcon = document.getElementById('launch-spinner-icon');
  const btnText = document.getElementById('launch-btn-text');

  if (isAuthenticated) {
    // Show ready state for authenticated users
    if (statusReady) statusReady.classList.add('active');
    if (launchBtn) {
      launchBtn.classList.remove('disabled');
      launchBtn.disabled = false;
    }
  } else {
    // Show login required state for non-authenticated users
    if (statusLoginRequired) statusLoginRequired.classList.add('active');
    if (launchBtn) {
      launchBtn.classList.add('disabled');
      launchBtn.disabled = true;
    }
  }

  // Ensure play icon is shown (not spinner) by default
  if (playIcon) playIcon.classList.remove('hidden-icon');
  if (spinnerIcon) spinnerIcon.classList.add('hidden-icon');
  if (btnText) btnText.textContent = getTranslation('LAUNCH_GAME');
}
window.initializeStatusUI = initializeStatusUI;

function showReadyState() {
  hideAllStatusStates();

  // Check authentication to show correct status
  const isAuthenticated = localStorage.getItem('authKey') !== null;
  const statusReady = document.getElementById('status-ready');
  const statusLoginRequired = document.getElementById('status-login-required');

  if (isAuthenticated) {
    if (statusReady) statusReady.classList.add('active');
  } else {
    if (statusLoginRequired) statusLoginRequired.classList.add('active');
  }

  // Hide pause button
  const pauseBtn = document.getElementById('btn-pause-resume');
  if (pauseBtn) pauseBtn.classList.remove('active');

  // Set launch button icons and text
  const launchBtn = document.getElementById('launch-game-btn');
  const playIcon = document.getElementById('launch-play-icon');
  const spinnerIcon = document.getElementById('launch-spinner-icon');
  const btnText = document.getElementById('launch-btn-text');

  if (playIcon) playIcon.classList.remove('hidden-icon');
  if (spinnerIcon) spinnerIcon.classList.add('hidden-icon');
  if (btnText) btnText.textContent = getTranslation('LAUNCH_GAME');

  // Enable launch button based on both authentication AND active account
  // The multi-account system requires an active account to be selected
  if (launchBtn) {
    const hasActiveAccount = typeof AccountManager !== 'undefined' && AccountManager.getActiveAccount();
    if (isAuthenticated && hasActiveAccount) {
      launchBtn.classList.remove('disabled');
      launchBtn.disabled = false;
    } else {
      launchBtn.classList.add('disabled');
      launchBtn.disabled = true;
    }
  }
}
window.showReadyState = showReadyState;

function showErrorState(errorMessage) {
  hideAllStatusStates();

  // Show the dedicated error state
  const statusError = document.getElementById('status-error');
  if (statusError) {
    statusError.classList.add('active');
  }

  // Show generic "Error" in the status area (not the specific message)
  const errorText = document.getElementById('status-error-text');
  if (errorText) {
    // Use simple "Error" text - the details are in the toast
    errorText.textContent = 'Error';
  }

  // Show specific error details in a PERSISTENT toast (won't auto-hide, has X to dismiss)
  if (errorMessage && typeof showUpdateNotification === 'function') {
    showUpdateNotification('error', 'Update Error', errorMessage, true);
  }

  // Keep launch button disabled
  const launchBtn = document.getElementById('launch-game-btn');
  if (launchBtn) {
    launchBtn.classList.add('disabled');
    launchBtn.disabled = true;
  }

  // Ensure spinner is hidden, show play icon on launch button
  const playIcon = document.getElementById('launch-play-icon');
  const spinnerIcon = document.getElementById('launch-spinner-icon');
  const btnText = document.getElementById('launch-btn-text');
  if (playIcon) playIcon.classList.remove('hidden-icon');
  if (spinnerIcon) spinnerIcon.classList.add('hidden-icon');
  if (btnText) btnText.textContent = getTranslation('LAUNCH_GAME');
}
window.showErrorState = showErrorState;

function updateHeaderAuthState(isLoggedIn, username) {
  const statusLoginRequired = document.getElementById('status-login-required');
  const statusReady = document.getElementById('status-ready');
  const statusChecking = document.getElementById('status-checking');
  const statusDownloading = document.getElementById('status-downloading');
  const launchBtn = document.getElementById('launch-game-btn');

  // Don't change status states if we're in the middle of checking or downloading
  // Only update auth-related UI (login button state)
  const isInProgress = (statusChecking && statusChecking.classList.contains('active')) ||
                       (statusDownloading && statusDownloading.classList.contains('active'));

  if (isLoggedIn) {
    // Only switch to ready state if not in progress
    if (!isInProgress) {
      hideAllStatusStates();
      if (statusReady) statusReady.classList.add('active');
    }

    // Enable launch button (unless checking/downloading)
    if (launchBtn && !isInProgress) {
      launchBtn.classList.remove('disabled');
      launchBtn.disabled = false;
    }
  } else {
    // Switch to login required state (but not during progress)
    if (!isInProgress) {
      hideAllStatusStates();
      if (statusLoginRequired) statusLoginRequired.classList.add('active');
    }

    // Disable launch button
    if (launchBtn) {
      launchBtn.classList.add('disabled');
      launchBtn.disabled = true;
    }
  }

  // Also update the index.html header (login form, user display, settings dropdown)
  if (typeof window.updateIndexHeaderAuthState === 'function') {
    window.updateIndexHeaderAuthState(isLoggedIn, username);
  }
}
window.updateHeaderAuthState = updateHeaderAuthState;

/**
 * Applies background images from data-bg attributes.
 * This is needed because inline style attributes don't work properly
 * when HTML is loaded via innerHTML in WebView2 release mode.
 */
function applyDataBackgrounds() {
  const elements = document.querySelectorAll('[data-bg]');
  elements.forEach(el => {
    const bgUrl = el.dataset.bg;
    if (bgUrl) {
      el.style.backgroundImage = `url('${bgUrl}')`;
    }
  });
}
window.applyDataBackgrounds = applyDataBackgrounds;

function initBackgroundCarousel() {
  // First, apply background images from data-bg attributes
  applyDataBackgrounds();

  const bgContainer = document.getElementById('home-bg-container');
  const bgImages = Array.from(document.querySelectorAll('#home-bg-container .bg-image'));

  if (bgImages.length === 0) {
    // No images, just mark as initialized
    if (bgContainer) bgContainer.classList.add('initialized');
    return;
  }

  if (bgImages.length === 1) {
    // Single image - just show it
    bgImages[0].classList.add('active');
    if (bgContainer) bgContainer.classList.add('initialized');
    return;
  }

  // Create shuffled order at launch (Fisher-Yates shuffle)
  const order = bgImages.map((_, i) => i);
  for (let i = order.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [order[i], order[j]] = [order[j], order[i]];
  }

  // Start with first image in shuffled order
  let orderIndex = 0;
  bgImages.forEach((img, i) => {
    img.classList.toggle('active', i === order[0]);
  });

  // Mark carousel as initialized (triggers fade-in)
  if (bgContainer) bgContainer.classList.add('initialized');

  // Cycle through shuffled order every 15 minutes
  setInterval(() => {
    bgImages[order[orderIndex]].classList.remove('active');
    orderIndex = (orderIndex + 1) % order.length;
    bgImages[order[orderIndex]].classList.add('active');
  }, 15 * 60 * 1000); // 15 minutes
}
window.initBackgroundCarousel = initBackgroundCarousel;

// ========== SETTINGS MENU HANDLERS ==========
// These must be in app.js (not inline script) because WebView2 release mode
// doesn't properly execute inline script code.

// Store original path to restore on cancel
let originalGamePath = '';

// Track toast auto-hide timeout so we can cancel it
let toastAutoHideTimeout = null;

/**
 * Shows the update notification toast with the given state, title, and subtitle.
 * @param {string} state - The state: 'checking', 'upToDate', 'success', 'error', or 'warning'
 * @param {string} title - The main title text
 * @param {string} subtitle - The subtitle text
 * @param {boolean} persistent - If true, toast won't auto-hide (shows close button)
 */
function showUpdateNotification(state, title, subtitle, persistent = false) {
  const toast = document.getElementById('update-toast');
  const icon = document.getElementById('toast-icon');
  const titleEl = document.getElementById('toast-title');
  const subtitleEl = document.getElementById('toast-subtitle');
  const closeBtn = document.getElementById('toast-close-btn');

  if (!toast) {
    console.error('update-toast element not found!');
    return;
  }

  // Clear any existing auto-hide timeout
  if (toastAutoHideTimeout) {
    clearTimeout(toastAutoHideTimeout);
    toastAutoHideTimeout = null;
  }

  // Update text
  if (titleEl) titleEl.textContent = title || 'Checking...';
  if (subtitleEl) subtitleEl.textContent = subtitle || '';

  // Update icon based on state
  if (icon) {
    icon.className = 'update-toast-icon ' + state;
    if (state === 'checking') {
      icon.innerHTML = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12a9 9 0 1 1-6.219-8.56"></path></svg>';
    } else if (state === 'upToDate' || state === 'success') {
      icon.className = 'update-toast-icon success';
      icon.innerHTML = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"></polyline></svg>';
    } else if (state === 'error') {
      icon.innerHTML = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="8" x2="12" y2="12"></line><line x1="12" y1="16" x2="12.01" y2="16"></line></svg>';
    } else if (state === 'warning') {
      icon.innerHTML = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>';
    }
  }

  // Show/hide close button based on persistent mode
  if (closeBtn) {
    closeBtn.style.display = persistent ? 'flex' : 'none';
  }

  // Mark toast as persistent or not (for styling)
  toast.classList.toggle('persistent', persistent);

  // Show toast
  toast.classList.add('show');

  // Auto-hide after 4 seconds for non-checking, non-persistent states
  if (state !== 'checking' && !persistent) {
    toastAutoHideTimeout = setTimeout(function() {
      toast.classList.remove('show');
    }, 4000);
  }
}
window.showUpdateNotification = showUpdateNotification;

/**
 * Hides the update notification toast.
 */
function hideUpdateNotification() {
  const toast = document.getElementById('update-toast');
  if (toast) {
    toast.classList.remove('show');
  }
}
window.hideUpdateNotification = hideUpdateNotification;

// Set up toast close button event listener (WebView2 compatible - no inline onclick)
document.addEventListener('DOMContentLoaded', () => {
  const toastCloseBtn = document.getElementById('toast-close-btn');
  if (toastCloseBtn) {
    toastCloseBtn.addEventListener('click', () => {
      hideUpdateNotification();
    });
  }
});

/**
 * Handler for Check Launcher Update menu item.
 */
async function handleCheckLauncherUpdate() {
  // Close dropdown immediately for visual feedback
  if (typeof window.closeSettingsDropdown === 'function') {
    window.closeSettingsDropdown();
  }

  // Show notification IMMEDIATELY before any async operations
  showUpdateNotification('checking', 'Checking for updates...', 'Please wait...');

  try {
    // Get current launcher version
    let currentVersion = 'unknown';
    try {
      if (window.__TAURI__?.app?.getVersion) {
        currentVersion = await window.__TAURI__.app.getVersion();
      }
    } catch (e) {
      console.warn('Could not get version:', e);
    }

    if (window.__TAURI__ && window.__TAURI__.updater) {
      const { checkUpdate } = window.__TAURI__.updater;
      if (checkUpdate) {
        const { shouldUpdate, manifest } = await checkUpdate();
        if (shouldUpdate) {
          showUpdateNotification('upToDate', 'Update available: ' + (manifest?.version || 'new version'), 'Current version: ' + currentVersion);
        } else {
          showUpdateNotification('upToDate', 'Launcher is up to date (v' + currentVersion + ')', 'No updates available');
        }
      } else {
        // checkUpdate not available
        showUpdateNotification('upToDate', 'Launcher v' + currentVersion, 'Update check not available');
      }
    } else {
      // Fallback if Tauri updater not available
      showUpdateNotification('upToDate', 'Launcher v' + currentVersion, 'Updater not available');
    }
  } catch (error) {
    console.error('Error checking for updates:', error);
    showUpdateNotification('error', 'Update check failed', error.message || 'Unknown error');
  }
}
window.handleCheckLauncherUpdate = handleCheckLauncherUpdate;

/**
 * Handler for Check & Repair Files menu item.
 */
function handleCheckRepairFiles() {
  // Close dropdown immediately for visual feedback
  if (typeof window.closeSettingsDropdown === 'function') {
    window.closeSettingsDropdown();
  }
  try {
    if (window.App && typeof App.revalidateAndUpdateGame === 'function') {
      App.revalidateAndUpdateGame();
    } else {
      console.error('App.revalidateAndUpdateGame not available');
    }
  } catch (error) {
    console.error('Error in handleCheckRepairFiles:', error);
  }
}
window.handleCheckRepairFiles = handleCheckRepairFiles;

/**
 * Handler for View Profile menu item.
 * Classic+ TODO: Re-enable when profile page and launcher-token API are available
 */
async function handleViewProfile() {
  // Close dropdown immediately for visual feedback
  if (typeof window.closeSettingsDropdown === 'function') {
    window.closeSettingsDropdown();
  }

  // Classic+ TODO: Re-enable when profile endpoint is available
  if (!URLS.external.profile) {
    console.log("[Classic+] Profile URL not configured");
    return;
  }

  const PROFILE_URL = URLS.external.profile;

  try {
    if (window.App) {
      App.openExternal(PROFILE_URL);
    }
  } catch (error) {
    console.error('Error opening profile:', error);
    if (window.App) {
      App.openExternal(PROFILE_URL);
    }
  }
}
window.handleViewProfile = handleViewProfile;

// Pending OAuth action — set when an OAuth account needs re-auth before an action
let _pendingOAuthAction = null; // 'launch' | 'switch' | null

/**
 * Opens the system browser for OAuth login with the given provider.
 * The website will redirect back via teraclassicplus:// deep link with a token.
 * Classic+ TODO: Re-enable when OAuth infrastructure is available
 * @param {string} provider - OAuth provider name
 * @param {string} [pendingAction] - Optional action to execute after OAuth completes
 */
function startOAuth(provider, pendingAction = null) {
  // Classic+ TODO: Re-enable when OAuth endpoint is available
  console.log("[Classic+] OAuth not available on Classic+ server");
  return;
}
window.startOAuth = startOAuth;

/**
 * Handle OAuth callback from deep link (teraclassicplus://auth?token=...).
 * Exchanges the token for a TERA auth bundle and completes login.
 * Classic+ TODO: Re-enable when OAuth infrastructure is available
 */
async function handleOAuthCallback(token, oauthProvider = null) {
  // Classic+ TODO: Re-enable when OAuth endpoint is available
  console.log("[Classic+] OAuth callback not available on Classic+ server");
  return;
}
window.handleOAuthCallback = handleOAuthCallback;

/**
 * Check for pending deep link on app startup and window focus.
 * Called by the Tauri backend when a teraclassicplus:// URL is received.
 * Classic+ TODO: Re-enable when deep link / OAuth infrastructure is available
 */
async function checkDeepLink() {
  // Classic+ TODO: Re-enable when OAuth deep link is available
  return;
}
window.checkDeepLink = checkDeepLink;

// Check for deep link on startup
document.addEventListener('DOMContentLoaded', () => {
  checkDeepLink();
});

// Check for deep link on window focus (launcher re-focused after browser OAuth)
window.addEventListener('focus', () => {
  checkDeepLink();
});

/**
 * Handler for Logout menu item.
 */
function handleLogout() {
  if (window.App && App.logout) {
    App.logout();
  }
}
window.handleLogout = handleLogout;

/**
 * Opens the game directory dialog.
 * @param {string|Object} [arg] - Legacy: pre-fill path string. New: options object.
 * @param {string} [arg.currentPath] - Path to pre-fill in the input.
 * @param {boolean} [arg.required] - If true, cancel/close is disabled until
 *   a valid folder is saved. Used when the launcher refuses to start without
 *   a valid game folder (first launch or folder gone invalid).
 * @param {string} [arg.errorMessage] - Banner shown at the top of the dialog
 *   (e.g. "TERA.exe not found in Binaries folder"). Rendered inside an
 *   #game-dir-error-banner node that is injected if missing.
 */
async function openGameDirectoryDialog(arg) {
  // Normalize legacy string arg to options object.
  const opts = (typeof arg === 'string') ? { currentPath: arg } : (arg || {});
  const { currentPath, required = false, errorMessage = '' } = opts;

  if (typeof window.closeSettingsDropdown === 'function') {
    window.closeSettingsDropdown();
  }
  try {
    const dialog = document.getElementById('game-directory-dialog');
    const input = document.getElementById('game-directory-input');
    const cancelBtn = document.getElementById('btn-cancel-game-dir');

    if (!dialog) {
      console.error('game-directory-dialog element not found');
      return;
    }

    dialog.dataset.required = required ? 'true' : 'false';
    if (cancelBtn) cancelBtn.style.display = required ? 'none' : '';

    // Inject / update the error banner slot.
    const content = dialog.querySelector('.game-dir-content');
    let banner = document.getElementById('game-dir-error-banner');
    if (errorMessage && content) {
      if (!banner) {
        banner = document.createElement('div');
        banner.id = 'game-dir-error-banner';
        banner.style.cssText = 'margin:0 0 12px;padding:10px 14px;border-radius:8px;background:rgba(220,38,38,0.12);border:1px solid rgba(220,38,38,0.5);color:#fecaca;font-size:13px;';
        content.insertBefore(banner, content.firstChild);
      }
      banner.textContent = errorMessage;
      banner.style.display = '';
    } else if (banner) {
      banner.style.display = 'none';
    }

    if (currentPath) {
      originalGamePath = currentPath;
      if (input) input.value = currentPath;
    } else if (window.App && typeof App.loadConfig === 'function') {
      try {
        const path = await App.loadConfig('gamePath');
        originalGamePath = path || '';
        if (input) input.value = originalGamePath;
      } catch (e) {
        console.warn('Could not load gamePath:', e);
        originalGamePath = '';
        if (input) input.value = '';
      }
    }

    dialog.classList.add('show');
  } catch (error) {
    console.error('Error in openGameDirectoryDialog:', error);
  }
}
window.openGameDirectoryDialog = openGameDirectoryDialog;

/**
 * Closes the game directory dialog and restores original path.
 * In required mode (set by openGameDirectoryDialog({required:true})),
 * close is suppressed — the user must pick a valid folder.
 */
function closeGameDirectoryDialog() {
  const dialog = document.getElementById('game-directory-dialog');
  const input = document.getElementById('game-directory-input');

  if (dialog && dialog.dataset.required === 'true') {
    return;
  }

  if (input) input.value = originalGamePath;

  if (dialog) {
    dialog.classList.remove('show');
  }
}
window.closeGameDirectoryDialog = closeGameDirectoryDialog;

/**
 * Opens file browser to select game directory.
 */
async function browseGameDirectory() {
  if (window.__TAURI__) {
    try {
      const selected = await window.__TAURI__.dialog.open({
        directory: true,
        multiple: false,
        title: 'Select Game Directory'
      });
      if (selected) {
        // Only update the input field, don't save yet
        document.getElementById('game-directory-input').value = selected;
      }
    } catch (e) {
      console.error('Failed to open directory dialog:', e);
    }
  }
}
window.browseGameDirectory = browseGameDirectory;

/**
 * Saves the game directory from the dialog input.
 * The backend (save_game_path_to_config) rejects folders that don't contain
 * Binaries/TERA.exe; on rejection the dialog stays open with the error toast.
 * When the dialog was opened in required mode (first launch or folder gone
 * invalid), a successful save clears the required flag and resumes launcher
 * initialization.
 */
async function saveGameDirectory() {
  const input = document.getElementById('game-directory-input');
  const dialog = document.getElementById('game-directory-dialog');
  const path = input?.value?.trim();

  if (!path) {
    showUpdateNotification('error', 'Invalid path', 'Please enter a valid game directory');
    return;
  }

  if (!(window.App && App.saveConfig)) {
    showUpdateNotification('error', 'App not ready', 'Please wait and try again');
    return;
  }

  const wasRequired = dialog && dialog.dataset.required === 'true';

  try {
    await App.saveConfig('gamePath', path);
  } catch (e) {
    console.error('Failed to save game directory:', e);
    showUpdateNotification('error', 'Failed to save', e.message || e.toString() || 'Unknown error');
    return; // Keep dialog open; user must pick a valid folder.
  }

  originalGamePath = path;
  if (dialog) {
    dialog.dataset.required = 'false';
    dialog.classList.remove('show');
  }
  const cancelBtn = document.getElementById('btn-cancel-game-dir');
  if (cancelBtn) cancelBtn.style.display = '';
  const banner = document.getElementById('game-dir-error-banner');
  if (banner) banner.style.display = 'none';

  showUpdateNotification('success', 'Game directory saved', path);

  // If the dialog was opened because the launcher couldn't start without a
  // valid folder, resume initialization now that we have one.
  if (wasRequired && window.App) {
    if (window.App.state && window.App.state.isFirstLaunch && typeof window.App.completeFirstLaunch === 'function') {
      window.App.completeFirstLaunch();
    } else if (typeof window.App.initializeAndCheckUpdates === 'function') {
      window.App.initializeAndCheckUpdates(false);
    }
  }
}
window.saveGameDirectory = saveGameDirectory;

// ========== IFRAME FUNCTIONS ==========
/**
 * Closes the iframe container with animation.
 */
function closeIframe() {
  const container = document.getElementById('iframeContainer');
  const iframe = document.getElementById('embeddedSite');
  if (container) {
    container.classList.remove('show');
    container.classList.add('hide');
    setTimeout(function() {
      container.style.display = 'none';
      if (iframe) iframe.src = '';
    }, 500);
  }
}
window.closeIframe = closeIframe;

/**
 * Shows the iframe container with the given URL.
 * @param {string} url - The URL to load in the iframe
 */
function showIframe(url) {
  const container = document.getElementById('iframeContainer');
  const iframe = document.getElementById('embeddedSite');
  if (iframe) iframe.src = url;
  if (container) {
    container.style.display = 'block';
    container.classList.remove('hide');
    container.classList.add('show');
  }
}
window.showIframe = showIframe;

/**
 * Fetches JSON data from a URL.
 * @param {string} url - The URL to fetch from
 * @returns {Promise<any>} The parsed JSON data
 */
async function fetchData(url) {
  try {
    const response = await fetch(url);
    if (!response.ok) {
      throw new Error('HTTP error! status: ' + response.status);
    }
    return await response.json();
  } catch (error) {
    console.error('There was a problem with the News fetch operation:', error);
  }
}
window.fetchData = fetchData;

// ========== HEADER AUTH STATE ==========
/**
 * Updates the header UI based on login state.
 * @param {boolean} isLoggedIn - Whether the user is logged in
 * @param {string} username - The username to display
 */
function updateIndexHeaderAuthState(isLoggedIn, username) {
  const logoutLink = document.getElementById('logout-link');
  const viewProfileLink = document.getElementById('menu-view-profile');

  if (isLoggedIn) {
    // Show logout and profile options in settings menu
    if (logoutLink) logoutLink.style.display = 'block';
    if (viewProfileLink) viewProfileLink.style.display = 'block';
  } else {
    // Hide logout and profile options when logged out
    if (logoutLink) logoutLink.style.display = 'none';
    if (viewProfileLink) viewProfileLink.style.display = 'none';
  }

  // Update account manager display (handled by AccountManager module)
  if (window.App && App.updateAccountDisplay) {
    App.updateAccountDisplay();
  }
}
window.updateIndexHeaderAuthState = updateIndexHeaderAuthState;

// ========== HEADER INITIALIZATION ==========
/**
 * Initializes header UI elements (region dropdown, login form, dialogs).
 * Called when DOM is ready.
 */
function initializeHeaderUI() {
  // ========== REGION DROPDOWN ==========
  const regionBtn = document.getElementById('region-btn-display');
  const regionDropdown = document.getElementById('region-dropdown');
  const regionCurrent = document.getElementById('region-current');
  const languageSelector = document.getElementById('language-selector');
  const regionOptions = document.querySelectorAll('.region-option');
  const regionFlag = document.getElementById('region-flag');

  if (regionBtn && regionDropdown) {
    // Toggle dropdown
    regionBtn.addEventListener('click', function(e) {
      e.stopPropagation();
      regionDropdown.classList.toggle('open');
    });

    // Close dropdown when clicking outside
    document.addEventListener('click', function() {
      regionDropdown.classList.remove('open');
    });
  }

  // Handle region option selection
  if (regionOptions.length > 0) {
    regionOptions.forEach(function(option) {
      option.addEventListener('click', function(e) {
        e.stopPropagation();
        const lang = this.dataset.lang;

        // Update visual display
        regionOptions.forEach(function(opt) { opt.classList.remove('active'); });
        this.classList.add('active');

        // Get language name (last text node, after the SVG)
        const langName = this.textContent.trim();
        if (regionCurrent) regionCurrent.textContent = langName;

        // Clone the SVG flag from the selected option
        const flagSvg = this.querySelector('.flag-icon');
        if (regionFlag && flagSvg) {
          regionFlag.innerHTML = '';
          regionFlag.appendChild(flagSvg.cloneNode(true));
        }

        // Update hidden select and trigger change
        if (languageSelector) {
          languageSelector.value = lang;
          languageSelector.dispatchEvent(new Event('change', { bubbles: true }));
        }

        // Close dropdown
        if (regionDropdown) regionDropdown.classList.remove('open');
      });
    });
  }

  // Sync with language selector changes from app.js
  if (languageSelector) {
    languageSelector.addEventListener('change', function() {
      const lang = this.value;
      regionOptions.forEach(function(opt) {
        if (opt.dataset.lang === lang) {
          opt.classList.add('active');
          const langName = opt.textContent.trim();
          if (regionCurrent) regionCurrent.textContent = langName;
          // Clone the SVG flag
          const flagSvg = opt.querySelector('.flag-icon');
          if (regionFlag && flagSvg) {
            regionFlag.innerHTML = '';
            regionFlag.appendChild(flagSvg.cloneNode(true));
          }
        } else {
          opt.classList.remove('active');
        }
      });
    });
  }

  // ========== GAME DIRECTORY DIALOG ==========
  const gameDirDialog = document.getElementById('game-directory-dialog');
  if (gameDirDialog) {
    // Close on backdrop click
    gameDirDialog.addEventListener('click', function(e) {
      if (e.target === this) {
        closeGameDirectoryDialog();
      }
    });
  }

  // Game directory dialog buttons
  const btnBrowseGameDir = document.getElementById('btn-browse-game-dir');
  if (btnBrowseGameDir) {
    btnBrowseGameDir.addEventListener('click', function(e) {
      e.preventDefault();
      browseGameDirectory();
    });
  }

  const btnCancelGameDir = document.getElementById('btn-cancel-game-dir');
  if (btnCancelGameDir) {
    btnCancelGameDir.addEventListener('click', function(e) {
      e.preventDefault();
      closeGameDirectoryDialog();
    });
  }

  const btnSaveGameDir = document.getElementById('btn-save-game-dir');
  if (btnSaveGameDir) {
    btnSaveGameDir.addEventListener('click', function(e) {
      e.preventDefault();
      saveGameDirectory();
    });
  }

  // ========== SETTINGS MENU ITEMS ==========
  const menuCheckLauncherUpdate = document.getElementById('menu-check-launcher-update');
  if (menuCheckLauncherUpdate) {
    menuCheckLauncherUpdate.addEventListener('click', function(e) {
      e.preventDefault();
      handleCheckLauncherUpdate();
    });
  }

  const menuCheckRepairFiles = document.getElementById('menu-check-repair-files');
  if (menuCheckRepairFiles) {
    menuCheckRepairFiles.addEventListener('click', function(e) {
      e.preventDefault();
      handleCheckRepairFiles();
    });
  }

  const menuGameDirectory = document.getElementById('menu-game-directory');
  if (menuGameDirectory) {
    menuGameDirectory.addEventListener('click', function(e) {
      e.preventDefault();
      openGameDirectoryDialog();
    });
  }

  const menuViewProfile = document.getElementById('menu-view-profile');
  if (menuViewProfile) {
    menuViewProfile.addEventListener('click', function(e) {
      e.preventDefault();
      handleViewProfile();
    });
  }

  const logoutLink = document.getElementById('logout-link');
  if (logoutLink) {
    logoutLink.addEventListener('click', function(e) {
      e.preventDefault();
      handleLogout();
    });
  }

  // ========== IFRAME EXIT BUTTON ==========
  const exitButton = document.getElementById('exitButton');
  if (exitButton) {
    exitButton.addEventListener('click', function(e) {
      e.preventDefault();
      closeIframe();
    });
  }
}
window.initializeHeaderUI = initializeHeaderUI;

// Initialize header UI when DOM is ready
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', initializeHeaderUI);
} else {
  // DOM already loaded, initialize immediately
  initializeHeaderUI();
}

/**
 * Escapes HTML special characters to prevent XSS attacks.
 * @param {string} str - The string to escape
 * @returns {string} The escaped string safe for innerHTML
 */
function escapeHtml(str) {
  if (str === null || str === undefined) return '';
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

// Simple semantic version comparison
function compareVersions(v1, v2) {
  const parts1 = v1.split(".").map((n) => parseInt(n, 10) || 0);
  const parts2 = v2.split(".").map((n) => parseInt(n, 10) || 0);
  const len = Math.max(parts1.length, parts2.length);
  for (let i = 0; i < len; i++) {
    const a = parts1[i] || 0;
    const b = parts2[i] || 0;
    if (a > b) return 1;
    if (a < b) return -1;
  }
  return 0;
}

const App = {
  translations: {},
  currentLanguage: "GER",
  languages: {
    GER: "GERMAN",
    EUR: "ENGLISH",
    FRA: "FRENCH",
    RUS: "RUSSIAN",
  },

  launchGameBtn: null,
  statusEl: null,
  loadingModal: null,
  loadingMessage: null,
  loadingError: null,
  refreshButton: null,
  quitTheApp: null,
  deferredUpdate: null,

  // Global application state
  state: {
    lastLogMessage: null,
    lastLogTime: 0,
    speedHistory: [],
    speedHistoryMaxLength: 10,
    isUpdateAvailable: false,
    isDownloadComplete: false,
    lastProgressUpdate: null,
    lastDownloadedBytes: 0,
    downloadStartTime: null,
    currentUpdateMode: null,
    currentProgress: 0,
    currentFileName: "",
    currentFileIndex: 0,
    totalFiles: 0,
    downloadedSize: 0,
    downloadedBytesOffset: 0,
    totalSize: 0,
    currentSpeed: 0,
    timeRemaining: 0,
    isLoggingIn: false,
    isLoggingOut: false,
    isGameRunning: false,
    gameExecutionFailed: false,
    updatesEnabled: true,
    isCheckingForUpdates: false,
    updateCheckPerformed: false,
    isGameLaunching: false,
    isAuthenticated: false,
    isFileCheckComplete: false,
    isFirstLaunch: true,
    isGeneratingHashFile: false,
    hashFileProgress: 0,
    currentProcessingFile: "",
    processedFiles: 0,
    isPauseRequested: false,
    updateError: false,
    isTogglingPauseResume: false,
    isDownloading: false,
  },

  // Store unlisteners for cleanup
  updateListeners: [],
  gameStatusListeners: [],
  gameStatusRecoveryInterval: null,
  errorListener: null,

  // Track pending download timeout so it can be cancelled
  pendingDownloadTimeout: null,

  /**
   * Updates the global application state.
   *
   * If `newState.totalSize` is provided, it will be used to initialize the
   * `totalSize` field in the state if it is currently undefined. If
   * `newState.totalDownloadedBytes` is provided, it will be used to initialize
   * the `totalDownloadedBytes` field in the state if it is currently undefined.
   *
   * Otherwise, the state is updated by shallow-merging `newState` into the
   * existing state.
   *
   * Finally, the UI is updated by calling `this.updateUI()`.
   *
   * @param {Object} newState - The new state to update the application with.
   * @param {number} [newState.totalSize] - The total size of the download.
   * @param {number} [newState.totalDownloadedBytes] - The total number of bytes
   *   downloaded so far.
   */
  setState(newState) {
    if (
      newState.totalSize !== undefined &&
      this.state.totalSize === undefined
    ) {
      this.state.totalSize = newState.totalSize;
    }
    if (
      newState.totalDownloadedBytes !== undefined &&
      this.state.totalDownloadedBytes === undefined
    ) {
      this.state.totalDownloadedBytes = 0;
    }
    Object.assign(this.state, newState);
    this.updateUI();
  },

  /**
   * Initializes the app by setting up event listeners, window controls, animations,
   * modal elements, and navigation. It also sends stored authentication information
   * to the backend, sets up a mutation observer, and checks if the user is authenticated.
   * If the user is authenticated and the current route is 'home', it checks if the app
   * is running for the first time and handles it accordingly. If the app is not running
   * for the first time, it checks for updates. If updates are disabled, it skips the
   * update check and server connection.
   */
  async init() {
    try {
      // Migrate legacy single-account storage to multi-account
      AccountManager.migrateFromLegacyStorage();
      AccountManager.getInstanceId(); // Ensure instance ID exists

      // If no active account but accounts exist, select the most recently used one
      if (!AccountManager.getActiveAccountId()) {
        const accounts = AccountManager.getAccounts();
        if (accounts.length > 0) {
          // Sort by lastUsed descending, pick first
          accounts.sort((a, b) => (b.lastUsed || 0) - (a.lastUsed || 0));
          AccountManager.setActiveAccountId(accounts[0].userNo);
          console.log('Auto-selected account:', accounts[0].userName);
        } else {
          console.log('No accounts found to auto-select');
        }
      }

      this.initAccountManager();

      // If there's an active account, do silent auth refresh to populate localStorage
      const activeAccount = AccountManager.getActiveAccount();
      if (activeAccount && activeAccount.authMethod === 'oauth') {
        // OAuth account — skip silent refresh, just update display
        console.log('OAuth account active — skipping silent auth refresh');
        this.setState({ isAuthenticated: true });
        this.updateAccountDisplay();
        this.updateLaunchButtonState();
      } else if (activeAccount && activeAccount.credentials) {
        try {
          const cred = JSON.parse(atob(activeAccount.credentials));
          const success = await this.silentAuthRefresh(cred.u, cred.p);
          if (success) {
            this.updateAccountDisplay();
            this.updateLaunchButtonState();
          }
        } catch (e) {
          console.warn("Failed to auto-refresh auth for active account:", e);
        }
      } else if (activeAccount) {
        // Have account but no credentials - still update display
        this.updateAccountDisplay();
        this.updateLaunchButtonState();
      }

      this.disableContextMenu();
      const savedTheme = localStorage.getItem("theme");
      if (savedTheme === "light") {
        document.body.classList.add("light-mode");
      }

      // Show the page now that basic styles are applied (prevents FOUC)
      const mainpage = document.querySelector('.mainpage');
      if (mainpage) mainpage.classList.add('ready');

    invoke("set_logging", { enabled: false });
      this.setupEventListeners();
      this.setupWindowControls();
      this.setupCustomAnimations();
      this.initializeLoadingModalElements();
      this.setupModalButtonEventHandlers();
      await this.updateLanguageSelector();
      this.setupHeaderLinks();
      this.displayLauncherVersion();
      this.Router.setupEventListeners();
      await this.Router.navigate();
      // Determine debug mode from backend (for logging/flags)
      try {
        const debug = await invoke("is_debug");
        window.__DEBUG__ = !!debug;
      } catch (e) {
        console.warn("Failed to query is_debug:", e);
      }
      this.sendStoredAuthInfoToBackend();
      this.setupMutationObserver();
      // Tauri's built-in updater (dialog: true) handles update checks automatically at startup
      this.checkAuthentication();
      this.resetState();
      this.updateUI();

      // Initialize status UI based on current auth state (before any async operations)
      this.initializeStatusUI();

      // Always load player count and news on home page
      if (this.Router.currentRoute === "home") {
        LoadStartPage();

        if (!UPDATE_CHECK_ENABLED) {
          this.setState({
            isUpdateAvailable: false,
            isFileCheckComplete: true,
            currentUpdateMode: "complete",
            currentProgress: 100,
          });
          // Update the status UI to show ready state since no checking is needed
          if (typeof window.hideCheckingState === 'function') {
            window.hideCheckingState();
          }
          this.updateLaunchGameButton(false);
          return;
        }

        const isConnected = await this.checkServerConnection();
        if (isConnected) {
          // Single gate for both first-launch and moved/deleted folder recovery.
          // ensureGameFolderValid blocks with a modal until the folder is valid;
          // the post-save flow re-enters this init via initializeAndCheckUpdates.
          this.checkFirstLaunch();
          const folderReady = await this.ensureGameFolderValid();
          if (folderReady) {
            await this.initializeAndCheckUpdates(false);
          }
        } else {
          console.error("Failed to connect to server on refresh");
          // Hide checking state and show appropriate status based on auth
          if (typeof window.hideCheckingState === 'function') {
            window.hideCheckingState();
          }
          this.updateLaunchGameButton(false);
        }
      }
    } catch (error) {
      console.error("Error during app initialization:", error);
      // Clear checking state on error to prevent UI from being stuck
      if (typeof window.hideCheckingState === 'function') {
        window.hideCheckingState();
      }
    }
  },

  /**
   * Registers a new user using the provided credentials.
   * @param {string} username - Desired username
   * @param {string} email - User email
   * @param {string} password - Desired password
   * @returns {Promise<void>}
   */
  async register(username, email, password) {
    const registerButton = document.getElementById("register-submit-button");
    const errorMsg = document.getElementById("register-error-msg");

    if (registerButton) {
      registerButton.disabled = true;
    }
    if (errorMsg) {
      errorMsg.style.display = "none";
      errorMsg.style.opacity = 0;
    }

    try {
      const response = await invoke("register_new_account", {
        login: username,
        email,
        password,
      });
      alert("Registration successful!");
      this.Router.navigate("home");
    } catch (err) {
      console.error(err);
      if (errorMsg) {
        errorMsg.textContent = err.message;
        errorMsg.style.display = "flex";
        errorMsg.style.opacity = 1;
      }
    } finally {
      if (registerButton) {
        registerButton.disabled = false;
      }
    }
  },

  // function to check if it's the first launch
  checkFirstLaunch() {
    const isFirstLaunch = localStorage.getItem("isFirstLaunch") !== "false";
    this.setState({ isFirstLaunch });
  },

  /**
   * Reads the canonical game-folder state from the backend and forces a
   * blocking picker if the folder is unset or invalid.
   *
   * States from `get_game_folder_state`:
   *   - { set: false, valid: false }           → never configured. Show first-launch welcome modal.
   *   - { set: true,  valid: false, error }    → path points somewhere without Binaries/TERA.exe. Force re-pick.
   *   - { set: true,  valid: true  }           → ready.
   *
   * Resolves to `true` once a valid folder is in place, `false` only if
   * the backend is unreachable (caller should degrade gracefully).
   */
  async ensureGameFolderValid() {
    let state;
    try {
      state = await invoke("get_game_folder_state");
    } catch (e) {
      console.error("get_game_folder_state failed:", e);
      return false;
    }

    if (state && state.valid) return true;

    if (!state || !state.set) {
      // Never configured → run the first-launch flow (language + folder picker).
      await this.handleFirstLaunch();
      return false;
    }

    // Set but invalid (user moved/deleted the folder, or picked the wrong one).
    // Non-dismissable dialog with the specific error; saveGameDirectory revalidates
    // via the backend and keeps the dialog open on failure.
    const errorMsg = state.error || "The configured game folder is no longer valid.";
    await openGameDirectoryDialog({
      currentPath: state.path || "",
      required: true,
      errorMessage: errorMsg,
    });
    return false;
  },

  /**
   * Sets up event listeners to handle page loading, hash changes, game status events, update events, and errors.
   */
  setupEventListeners() {
    window.addEventListener("hashchange", () => this.handleRouteChange());

    this.setupGameStatusListeners();
    this.setupUpdateListeners();
    this.setupErrorListener();
    this.setupModsToolbar();
  },

  /**
   * Wires the top-right Mods icon button (toolbar) and the first-launch
   * onboarding dialog's buttons. Both lookups are safe if the elements
   * aren't present yet (older index.html without mods UI) — we just
   * silently skip.
   */
  setupModsToolbar() {
    const modsButton = document.getElementById("mods-button");
    if (modsButton && !modsButton.dataset.bound) {
      modsButton.dataset.bound = "1";
      modsButton.addEventListener("click", (e) => {
        e.preventDefault();
        if (window.ModsView?.open) window.ModsView.open();
      });
    }

    const dismiss = document.getElementById("mods-onboarding-dismiss");
    if (dismiss && !dismiss.dataset.bound) {
      dismiss.dataset.bound = "1";
      dismiss.addEventListener("click", () => this.dismissModsOnboarding());
    }

    const openMods = document.getElementById("mods-onboarding-open");
    if (openMods && !openMods.dataset.bound) {
      openMods.dataset.bound = "1";
      openMods.addEventListener("click", () => {
        this.dismissModsOnboarding();
        if (window.ModsView?.open) window.ModsView.open();
      });
    }
  },

  /**
   * Hides the onboarding dialog and persists the acknowledgment in
   * localStorage so it never fires again.
   */
  dismissModsOnboarding() {
    const card = document.getElementById("mods-onboarding");
    if (card) card.hidden = true;
    try {
      localStorage.setItem("mods_onboarding_seen", "true");
    } catch (e) {
      // localStorage may be disabled in some Tauri configs; onboarding
      // just shows again next time — no harm.
      console.warn("mods onboarding: could not persist seen flag", e);
    }
  },

  /**
   * Shows the onboarding dialog if the user has never acknowledged it.
   * Returns true if the dialog was shown (caller should treat this as
   * "intercepted the launch click"), false if it was not shown.
   */
  maybeShowModsOnboarding() {
    try {
      if (localStorage.getItem("mods_onboarding_seen") === "true") return false;
    } catch (_) {
      return false;
    }
    const card = document.getElementById("mods-onboarding");
    if (!card) return false;
    card.hidden = false;
    return true;
  },

  /**
   * Scans installed mods against the catalog and, if any are out of date,
   * shows a persistent banner at bottom-right so the user knows to open
   * the Mods modal and pull the new versions down. Runs once per launch;
   * the × dismisses until the next launch (no persistence).
   *
   * Silently no-ops if the Tauri bridge isn't ready yet, or if the
   * catalog / installed list is empty — this is a best-effort hint, not
   * a guarantee.
   */
  async checkModUpdatesOnLaunch() {
    try {
      if (this._modUpdatesChecked) return;
      this._modUpdatesChecked = true;

      const invoke = window.__TAURI__?.tauri?.invoke || window.__TAURI__?.invoke;
      if (!invoke) return;

      let catalog, installed;
      try {
        catalog = await invoke("get_mods_catalog", { forceRefresh: false });
      } catch (e) {
        console.warn("mod-update-banner: catalog fetch failed", e);
        return;
      }
      try {
        installed = await invoke("list_installed_mods");
      } catch (e) {
        console.warn("mod-update-banner: installed list failed", e);
        return;
      }

      const mods = Array.isArray(catalog?.mods) ? catalog.mods : [];
      if (!mods.length || !Array.isArray(installed) || !installed.length) return;

      const byId = new Map(mods.map((m) => [m.id, m]));
      const outdated = [];
      for (const row of installed) {
        const cat = byId.get(row.id);
        if (!cat || !cat.version || !row.version) continue;
        if (cat.version === row.version) continue;
        outdated.push({ id: row.id, name: row.name || cat.name, oldV: row.version, newV: cat.version });
      }

      if (!outdated.length) return;

      const banner = document.getElementById("mods-update-banner");
      const title = document.getElementById("mods-update-banner-title");
      const subtitle = document.getElementById("mods-update-banner-subtitle");
      if (!banner) return;

      const count = outdated.length;
      if (title) {
        title.textContent = count === 1
          ? "1 mod update available"
          : `${count} mod updates available`;
      }
      if (subtitle) {
        const names = outdated.slice(0, 3).map((o) => o.name).join(", ");
        const extra = count > 3 ? ` +${count - 3} more` : "";
        subtitle.textContent = `Open the mod manager to apply: ${names}${extra}`;
      }

      banner.hidden = false;

      if (!banner.dataset.bound) {
        banner.dataset.bound = "1";
        banner.addEventListener("click", (e) => {
          const action = e.target?.closest?.("[data-action]")?.dataset?.action;
          if (action === "dismiss") {
            banner.hidden = true;
            return;
          }
          // "open" (banner body) or any click inside the banner that
          // isn't the × dismiss button.
          banner.hidden = true;
          if (window.ModsView?.open) window.ModsView.open();
        });
      }
    } catch (e) {
      console.warn("mod-update-banner: unexpected failure", e);
    }
  },

  /**
   * Sets up event listeners for game status events from the game server.
   *
   * Listens for the following events:
   *
   * - `game_status`: emitted when the game status is updated. The event payload is either
   *   `GAME_STATUS_RUNNING` or `GAME_STATUS_NOT_RUNNING`.
   * - `game_status_changed`: emitted when the game status changes. The event payload is a
   *   boolean indicating whether the game is running or not.
   * - `game_ended`: emitted when the game has ended. The event payload is empty.
   *
   * When any of these events are received, the UI is updated to reflect the new game status.
   */
  async setupGameStatusListeners() {
    // Clean up any existing listeners first
    this.cleanupGameStatusListeners();

    const gameStatusListener = await listen("game_status", async (event) => {
      const isRunning = event.payload === "GAME_STATUS_RUNNING";
      this.updateUIForGameStatus(isRunning);
    });
    this.gameStatusListeners.push(gameStatusListener);

    const gameStatusChangedListener = await listen("game_status_changed", (event) => {
      const isRunning = event.payload;
      this.updateUIForGameStatus(isRunning);
    });
    this.gameStatusListeners.push(gameStatusChangedListener);

    const gameEndedListener = await listen("game_ended", async (event) => {
      // Event payload contains the user_no of the account whose game ended
      const userNo = event.payload;
      console.log(`game_ended event received for user_no: ${userNo} (type: ${typeof userNo})`);

      if (userNo && typeof userNo === 'number') {
        // We know exactly which account's game ended - unregister it directly
        // Convert to string for consistent object key handling in AccountManager
        const userNoStr = String(userNo);
        const wasRegistered = AccountManager.isAccountInGame(userNoStr);
        AccountManager.unregisterRunningGame(userNoStr);
        console.log(`Unregistered game for account ${userNoStr} (was registered: ${wasRegistered})`);

        // Update all UI components
        this.updateAccountDisplay();
        this.renderAccountDropdown();
        this.updateLaunchButtonState();

        // Check backend status - should still be true if other games running
        const backendRunning = await invoke("get_game_status");
        const backendCount = await invoke("get_running_game_count");
        console.log(`After unregister: backend running=${backendRunning}, count=${backendCount}`);

        // Only update UI for game status - don't let it clear remaining games
        if (backendRunning) {
          // Other games still running - update status for current account only
          const activeAccount = AccountManager.getActiveAccount();
          const activeInGame = activeAccount && AccountManager.isAccountInGame(activeAccount.userNo);
          if (activeInGame) {
            if (this.statusEl) this.statusEl.textContent = this.t('IN_GAME') || 'In Game';
            this.updateLaunchGameButton(true);
          } else {
            if (this.statusEl) this.statusEl.textContent = this.t("READY_TO_PLAY");
            this.updateLaunchGameButton(false);
          }
        } else {
          // No games running - safe to use standard update
          await this.updateGameStatus();
        }
      } else {
        // Fallback: reconcile if payload is missing (shouldn't happen)
        console.warn('game_ended event missing user_no, falling back to reconciliation');
        await this.reconcileGameState();
      }
    });
    this.gameStatusListeners.push(gameEndedListener);

    // Add visibility change listener to poll game status when launcher regains focus
    // This recovers from stuck states when game is killed via taskbar
    const visibilityHandler = async () => {
      if (document.visibilityState === 'visible') {
        // Small delay to let any pending events process first
        setTimeout(async () => {
          await this.updateGameStatus();
        }, 100);
      }
    };
    document.addEventListener('visibilitychange', visibilityHandler);
    // Store cleanup function for visibility handler
    this.gameStatusListeners.push(() => {
      document.removeEventListener('visibilitychange', visibilityHandler);
    });

    // Add window focus listener as additional recovery mechanism
    const focusHandler = async () => {
      await this.updateGameStatus();
    };
    window.addEventListener('focus', focusHandler);
    this.gameStatusListeners.push(() => {
      window.removeEventListener('focus', focusHandler);
    });
  },

  /**
   * Sets up event listeners for update events from the game server.
   *
   * Listens for the following events:
   *
   * - `download_progress`: emitted when the download progress is updated. The event payload is a
   *   DownloadProgress object.
   * - `file_check_progress`: emitted when the file check progress is updated. The event payload is a
   *   FileCheckProgress object.
   * - `file_check_completed`: emitted when the file check is complete. The event payload is an empty
   *   object.
   * - `download_complete`: emitted when the download is complete. The event payload is an empty
   *   object.
   *
   * When any of these events are received, the UI is updated to reflect the new download status.
   */
  async setupUpdateListeners() {
    // Clear any existing listeners first
    this.cleanupUpdateListeners();

    const progressListener = await listen("download_progress", this.handleDownloadProgress.bind(this));
    this.updateListeners.push(progressListener);

    // Stabilized global progress for speed/ETA and total bar
    const globalProgressListener = await listen("global_download_progress", (event) => {
      const p = event?.payload;
      if (!p) return;
      const now = Date.now();
      const totalDownloadedBytes = p.downloaded_bytes || 0;
      const totalSize = Math.max(
        this.state.totalSize || 0,
        p.total_bytes || 0,
      );
      const elapsed = p.elapsed_time || 0;
      const baseDownloaded = p.base_downloaded || 0;
      const sessionBytes = Math.max(0, totalDownloadedBytes - baseDownloaded);
      const globalSpeed = elapsed > 0 ? sessionBytes / elapsed : 0;
      const timeRemaining = this.calculateGlobalTimeRemaining(
        totalDownloadedBytes,
        totalSize,
        globalSpeed,
      );

      // Only update progress data, don't touch currentUpdateMode
      this.setState({
        currentProgress:
          totalSize > 0
            ? Math.min(100, (totalDownloadedBytes / totalSize) * 100)
            : 0,
        currentSpeed: globalSpeed,
        downloadedSize: totalDownloadedBytes,
        totalDownloadedBytes: totalDownloadedBytes,
        timeRemaining: timeRemaining,
        currentFileName: p.file_name || "",
        lastProgressUpdate: now,
      });
    });
    this.updateListeners.push(globalProgressListener);

    const fileCheckProgressListener = await listen("file_check_progress", this.handleFileCheckProgress.bind(this));
    this.updateListeners.push(fileCheckProgressListener);

    const fileCheckCompletedListener = await listen("file_check_completed", this.handleFileCheckCompleted.bind(this));
    this.updateListeners.push(fileCheckCompletedListener);

    const downloadCompleteListener = await listen("download_complete", () => {
      console.log(">>> download_complete event received! updateError:", this.state.updateError);
      // CRITICAL: Do not complete if there was an error
      if (this.state.updateError) {
        console.log(">>> download_complete IGNORED because updateError is true");
        return;
      }
      // Finalize via unified completion path
      this.handleCompletion();
    });
    this.updateListeners.push(downloadCompleteListener);

    // Listen for verification status events (post-download hash verification)
    const downloadVerifyingListener = await listen("download_verifying", (event) => {
      const payload = event?.payload;
      if (!payload) return;

      const status = payload.status;
      const dlLabel = document.getElementById('dl-status-label');

      switch (status) {
        case "started":
          console.log("Post-download verification started");
          if (dlLabel) dlLabel.textContent = this.t('VERIFYING_FILES') || 'Verifying files...';
          break;
        case "verifying":
          // Update UI to show which file is being verified
          if (dlLabel) {
            dlLabel.textContent = `${this.t('VERIFYING_FILES') || 'Verifying'} (${payload.verified}/${payload.total_files})`;
          }
          break;
        case "hash_mismatch":
          console.warn(`Hash mismatch for ${payload.file}, size: ${payload.actual_size}/${payload.expected_size}`);
          if (dlLabel) {
            const shortFile = this.getFileName(payload.file);
            dlLabel.textContent = `File corrupted, redownloading: ${shortFile}`;
          }
          break;
        case "retrying":
          console.log(`Retrying ${payload.file} (${payload.attempt}/${payload.max_attempts})`);
          if (dlLabel) {
            const shortFile = this.getFileName(payload.file);
            dlLabel.textContent = `Redownloading (${payload.attempt}/${payload.max_attempts}): ${shortFile}`;
          }
          break;
        case "verification_error":
          console.warn(`Verification error for ${payload.file}: ${payload.error}`);
          if (dlLabel) {
            const shortFile = this.getFileName(payload.file);
            const errorDetail = payload.error || 'Unknown error';
            dlLabel.textContent = `Error verifying ${shortFile}: ${errorDetail}`;
          }
          break;
        case "verify_retry":
          console.log(`Verifying ${payload.file} (attempt ${payload.attempt}/${payload.max_attempts}): ${payload.reason}`);
          if (dlLabel) {
            const shortFile = this.getFileName(payload.file);
            const reason = payload.reason || 'file may be locked';
            dlLabel.textContent = `Retrying ${shortFile} (${payload.attempt}/${payload.max_attempts}): ${reason}`;
          }
          break;
        case "verify_skipped":
          console.warn(`Verification skipped for ${payload.file}: ${payload.reason}`);
          if (dlLabel) {
            const shortFile = this.getFileName(payload.file);
            const reason = payload.reason || 'could not access file';
            dlLabel.textContent = `Skipped verification for ${shortFile}: ${reason}`;
          }
          break;
        case "file_skipped":
          console.warn(`File skipped due to persistent issues: ${payload.file}`);
          if (dlLabel) {
            const shortFile = this.getFileName(payload.file);
            dlLabel.textContent = `Skipped ${shortFile} - may need repair`;
          }
          // Show a notification so user knows there might be an issue
          if (typeof window.showUpdateNotification === 'function') {
            window.showUpdateNotification('warning', 'File Issue', `${this.getFileName(payload.file)} could not be verified. Use 'Repair Game Files' if you have issues.`);
          }
          break;
        case "completed":
          console.log("Verification completed");
          break;
      }
    });
    this.updateListeners.push(downloadVerifyingListener);

    const downloadErrorListener = await listen("download_error", (event) => {
      const message =
        event?.payload?.message || this.t("UPDATE_ERROR_MESSAGE");
      const fileName = event?.payload?.file || "";
      console.error("Download error:", message, fileName ? `(file: ${fileName})` : "");

      this.setState({
        updateError: true,
        currentUpdateMode: "paused", // Set to paused so user can retry
        isUpdateAvailable: true,
        isFileCheckComplete: true,
        isPauseRequested: false,
        isDownloading: false,
      });

      // Show error state with persistent toast notification
      // showErrorState shows generic "Error" in status + persistent toast with details
      if (typeof window.showErrorState === 'function') {
        window.showErrorState(message);
      } else {
        // Fallback: hide downloading but don't show ready (keeps UI in limbo intentionally)
        if (typeof window.hideAllStatusStates === 'function') {
          window.hideAllStatusStates();
        }
      }

      // Keep LAUNCH disabled since files aren't ready
      this.updateLaunchGameButton(true); // true = disable
      this.toggleLanguageSelector(true);

      // Show pause/resume button so user can retry
      const pauseBtn = document.getElementById('btn-pause-resume');
      if (pauseBtn) {
        pauseBtn.classList.add('active');
        // Show play/resume icon
        const pauseIcon = document.getElementById('pause-icon');
        const resumeIcon = document.getElementById('resume-icon');
        if (pauseIcon) pauseIcon.classList.add('hidden-icon');
        if (resumeIcon) resumeIcon.classList.remove('hidden-icon');
      }
    });
    this.updateListeners.push(downloadErrorListener);

    const downloadCancelledListener = await listen("download_cancelled", () => {
      (async () => {
        let downloadedSnapshot = this.state.downloadedSize;
        try {
          downloadedSnapshot = await invoke("get_downloaded_bytes");
        } catch (e) {
          console.warn("Failed to query download snapshot:", e);
        }
        this.setState({
          currentUpdateMode: "paused",
          isUpdateAvailable: true,
          isPauseRequested: false,
          downloadedSize: downloadedSnapshot,
          downloadedBytesOffset: downloadedSnapshot,
        });
        this.updateLaunchGameButton(true);
      })();
    });
    this.updateListeners.push(downloadCancelledListener);
  },

  /**
   * Cleans up update event listeners to prevent memory leaks.
   *
   * This method should be called before setting up new listeners or when
   * the component is being destroyed (e.g., on logout or route change).
   */
  cleanupUpdateListeners() {
    if (this.updateListeners && this.updateListeners.length > 0) {
      this.updateListeners.forEach(unlisten => {
        if (typeof unlisten === 'function') {
          try {
            unlisten();
          } catch (e) {
            console.warn("Failed to unlisten:", e);
          }
        }
      });
      this.updateListeners = [];
    }
  },

  /**
   * Cleans up game status event listeners to prevent memory leaks.
   *
   * This method should be called before setting up new listeners or when
   * the component is being destroyed (e.g., on logout or route change).
   */
  cleanupGameStatusListeners() {
    // Clear the recovery interval if it exists
    if (this.gameStatusRecoveryInterval) {
      clearInterval(this.gameStatusRecoveryInterval);
      this.gameStatusRecoveryInterval = null;
    }

    for (const unlisten of this.gameStatusListeners) {
      if (typeof unlisten === 'function') {
        try {
          unlisten();
        } catch (e) {
          console.warn("Failed to unlisten game status listener:", e);
        }
      }
    }
    this.gameStatusListeners = [];
  },

  /**
   * Sets up an event listener for error events from the game server.
   *
   * Listens for the following event:
   *
   * - `error`: emitted when an error occurs. The event payload is an error message string.
   *
   * When any of these events are received, the UI is updated to reflect the new error state.
   */
  async setupErrorListener() {
    if (this.errorListener) {
      this.errorListener();
    }
    this.errorListener = await listen("error", (event) => {
      this.showErrorMessage(event.payload);
    });
  },

  /**
   * Cleans up error event listener to prevent memory leaks.
   */
  cleanupErrorListener() {
    if (this.errorListener) {
      try {
        this.errorListener();
      } catch (e) {
        console.warn("Failed to unlisten error listener:", e);
      }
      this.errorListener = null;
    }
  },

  async handleFirstLaunch() {
    this.showFirstLaunchModal();
  },

  // Function to show a custom modal for first launch
  showFirstLaunchModal() {
    // SVG flags matching the header dropdown
    const flagSvgs = {
      GER: '<svg class="flag-icon" viewBox="0 0 640 480"><path fill="#ffce00" d="M0 320h640v160H0z"/><path d="M0 0h640v160H0z"/><path fill="#d00" d="M0 160h640v160H0z"/></svg>',
      EUR: '<svg class="flag-icon" viewBox="0 0 640 480"><path fill="#012169" d="M0 0h640v480H0z"/><path fill="#FFF" d="m75 0 244 181L562 0h78v62L400 241l240 178v61h-80L320 301 81 480H0v-60l239-178L0 64V0z"/><path fill="#C8102E" d="m424 281 216 159v40L369 281zm-184 20 6 35L54 480H0zM640 0v3L391 191l2-44L590 0zM0 0l239 176h-60L0 42z"/><path fill="#FFF" d="M241 0v480h160V0zM0 160v160h640V160z"/><path fill="#C8102E" d="M0 193v96h640v-96zM273 0v480h96V0z"/></svg>',
      FRA: '<svg class="flag-icon" viewBox="0 0 640 480"><path fill="#fff" d="M0 0h640v480H0z"/><path fill="#002654" d="M0 0h213.3v480H0z"/><path fill="#ce1126" d="M426.7 0H640v480H426.7z"/></svg>',
      RUS: '<svg class="flag-icon" viewBox="0 0 640 480"><path fill="#fff" d="M0 0h640v160H0z"/><path fill="#0039a6" d="M0 160h640v160H0z"/><path fill="#d52b1e" d="M0 320h640v160H0z"/></svg>',
    };

    const modal = document.createElement("div");
    modal.id = "first-launch-modal";
    modal.innerHTML = `
      <div class="first-launch-content">
        <div class="first-launch-icon">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"></path>
            <polyline points="9 12 12 15 16 10"></polyline>
          </svg>
        </div>
        <h2 class="first-launch-title">${this.t("WELCOME_TO_LAUNCHER")}</h2>
        <p class="first-launch-subtitle">${this.t("FIRST_LAUNCH_MESSAGE")}</p>
        <div class="first-launch-form">
          <label class="first-launch-label">
            ${this.t("CHOOSE_DEFAULT_LANGUAGE")}
          </label>
          <div class="first-launch-dropdown-wrapper">
            <button class="first-launch-dropdown-btn" id="first-launch-lang-btn">
              <span class="first-launch-lang-flag" id="first-launch-flag"></span>
              <span class="first-launch-lang-name" id="first-launch-lang-name"></span>
              <svg class="first-launch-dropdown-arrow" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="6 9 12 15 18 9"></polyline>
              </svg>
            </button>
            <div class="first-launch-dropdown" id="first-launch-dropdown"></div>
          </div>
          <button id="set-game-path-btn" class="first-launch-continue-btn">
            ${this.t("SET_GAME_PATH")}
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <line x1="5" y1="12" x2="19" y2="12"></line>
              <polyline points="12 5 19 12 12 19"></polyline>
            </svg>
          </button>
        </div>
      </div>
    `;
    document.body.appendChild(modal);

    // Populate dropdown options
    const dropdown = document.getElementById("first-launch-dropdown");
    const flagDisplay = document.getElementById("first-launch-flag");
    const nameDisplay = document.getElementById("first-launch-lang-name");
    const dropdownBtn = document.getElementById("first-launch-lang-btn");
    let selectedLang = this.currentLanguage;

    for (const [code, name] of Object.entries(this.languages)) {
      const option = document.createElement("button");
      option.className = "first-launch-dropdown-option";
      option.dataset.lang = code;
      option.innerHTML = `${flagSvgs[code] || ""}<span>${name}</span>`;
      if (code === selectedLang) {
        option.classList.add("active");
      }
      dropdown.appendChild(option);
    }

    // Set initial display
    flagDisplay.innerHTML = flagSvgs[selectedLang] || "";
    nameDisplay.textContent = this.languages[selectedLang] || "";

    // Toggle dropdown
    dropdownBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      dropdown.classList.toggle("open");
    });

    // Handle option selection — flagSvgs values are hardcoded static SVG
    // strings defined in this file, never sourced from user input.
    dropdown.addEventListener("click", async (e) => {
      const option = e.target.closest(".first-launch-dropdown-option");
      if (!option) return;

      const lang = option.dataset.lang;
      selectedLang = lang;

      // Update display
      flagDisplay.innerHTML = flagSvgs[lang] || "";
      nameDisplay.textContent = this.languages[lang] || "";

      // Update active state
      dropdown.querySelectorAll(".first-launch-dropdown-option").forEach(opt => {
        opt.classList.toggle("active", opt.dataset.lang === lang);
      });

      dropdown.classList.remove("open");

      // Apply language change immediately so modal text updates
      await this.changeLanguage(lang);

      // Update modal text with new language
      const titleEl = modal.querySelector(".first-launch-title");
      const subtitleEl = modal.querySelector(".first-launch-subtitle");
      const labelEl = modal.querySelector(".first-launch-label");
      const btnEl = document.getElementById("set-game-path-btn");
      if (titleEl) titleEl.textContent = this.t("WELCOME_TO_LAUNCHER");
      if (subtitleEl) subtitleEl.textContent = this.t("FIRST_LAUNCH_MESSAGE");
      if (labelEl) labelEl.textContent = this.t("CHOOSE_DEFAULT_LANGUAGE");
      if (btnEl) btnEl.textContent = this.t("SET_GAME_PATH");
    });

    // Close dropdown when clicking outside
    modal.addEventListener("click", (e) => {
      if (!e.target.closest(".first-launch-dropdown-wrapper")) {
        dropdown.classList.remove("open");
      }
    });

    const setGamePathBtn = document.getElementById("set-game-path-btn");
    setGamePathBtn.addEventListener("click", async () => {
      await this.changeLanguage(selectedLang);
      this.closeFirstLaunchModal();
      this.openGamePathSettings();
    });

    anime({
      targets: modal,
      opacity: [0, 1],
      duration: 300,
      easing: "easeOutQuad",
    });

    anime({
      targets: ".first-launch-content",
      opacity: [0, 1],
      scale: [0.95, 1],
      duration: 400,
      delay: 50,
      easing: "easeOutQuad",
    });
  },

  // Function to close the first launch modal
  closeFirstLaunchModal() {
    const modal = document.getElementById("first-launch-modal");
    if (!modal) return;

    anime({
      targets: ".first-launch-content",
      opacity: 0,
      scale: 0.95,
      duration: 200,
      easing: "easeInQuad",
    });

    anime({
      targets: modal,
      opacity: 0,
      duration: 250,
      delay: 50,
      easing: "easeInQuad",
      complete: () => {
        modal.remove();
      },
    });
  },

  // Function to open game path settings
  openGamePathSettings() {
    if (typeof window.openGameDirectoryDialog === 'function') {
      window.openGameDirectoryDialog();
    }
  },

  // Opens an url in the user's default browser
  openExternal(url) {
    const localizedUrl = localizeForumUrl(url, this.currentLanguage);

    if (
      window.__TAURI__ &&
      window.__TAURI__.shell &&
      window.__TAURI__.shell.open
    ) {
      window.__TAURI__.shell.open(localizedUrl);
    } else {
      window.open(localizedUrl, "_blank");
    }
  },

  // Map launcher language codes to website locale codes
  getWebsiteLocale() {
    const map = { GER: 'de', EUR: 'en', FRA: 'fr', RUS: 'ru' };
    return map[this.currentLanguage] || 'en';
  },

  // Open the registration website in the user's default browser
  async openRegisterPopup() {
    // Classic+ TODO: Re-enable when registration page is available
    if (!URLS.external.register) {
      console.log("[Classic+] Registration URL not configured");
      return;
    }
    const locale = this.getWebsiteLocale();
    this.openExternal(`${URLS.external.register}?locale=${locale}`);
  },

  // Set up handlers for the header buttons and links
  setupHeaderLinks() {
    const startBtn = document.getElementById("start-button");
    if (startBtn) {
      startBtn.addEventListener("click", (e) => {
        e.preventDefault();
        // Classic+ TODO: Re-enable when forum is available
        if (!URLS.external.forum) return;
        this.openExternal(URLS.external.forum);
      });
    }

    const settingsBtn = document.getElementById("settings-button");
    const settingsDropdown = document.getElementById(
      "settings-dropdown-wrapper",
    );
    if (settingsBtn && settingsDropdown) {
      let open = false;
      gsap.set(settingsDropdown, { display: "none", opacity: 0, y: -10 });
      const tl = gsap.timeline({ paused: true });
      tl.to(settingsDropdown, {
        duration: 0.3,
        display: "block",
        opacity: 1,
        y: 0,
        ease: "power2.out",
      });

      // Close function - exposed globally for menu items
      const closeDropdown = () => {
        if (open) {
          tl.reverse().then(() =>
            gsap.set(settingsDropdown, { display: "none" }),
          );
          open = false;
        }
      };
      window.closeSettingsDropdown = closeDropdown;

      settingsBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        if (!open) {
          tl.play();
          open = true;
        } else {
          closeDropdown();
        }
      });
      document.addEventListener("click", () => {
        closeDropdown();
      });
      settingsDropdown.addEventListener("click", (event) => {
        // Only stop propagation for external links, let menu item clicks through
        if (event.target.tagName === "A" && event.target.target === "_blank") {
          event.stopPropagation();
          event.preventDefault();
          this.openExternal(event.target.href);
        }
        // Menu items (.menu-item) will bubble up to document and close the dropdown
      });
    }

    const links = [
      { id: "discord-button", url: URLS.external.discord },
      { id: "support-button", url: URLS.external.support },
      { id: "privacy-link", url: URLS.external.privacy },
    ];
    links.forEach((link) => {
      const el = document.getElementById(link.id);
      if (el) {
        // Hide buttons with empty URLs so they don't confuse users
        if (!link.url) {
          el.style.display = "none";
          return;
        }
        el.addEventListener("click", (e) => {
          e.preventDefault();
          this.openExternal(link.url);
        });
      }
    });

    // Wire: Check Launcher Update
    const checkLauncherUpdate = document.getElementById(
      "check-launcher-update",
    );
    if (checkLauncherUpdate) {
      checkLauncherUpdate.addEventListener("click", async (e) => {
        e.preventDefault();
        try {
          const updater = window.__TAURI__?.updater;
          if (!updater) {
            this.showCustomNotification("Update check not available.", "error");
            return;
          }
          // Tauri's built-in dialog (dialog: true) shows automatically if update available
          const { shouldUpdate } = await updater.checkUpdate();
          if (!shouldUpdate) {
            const version = await window.__TAURI__?.app?.getVersion?.() || "unknown";
            this.showCustomNotification(
              `You are on the latest launcher (v${version}).`,
              "success",
            );
          }
        } catch (err) {
          console.error("Update check failed:", err);
          this.showCustomNotification("Update check failed.", "error");
        }
      });
    }
  },

  // Display launcher version in the header
  async displayLauncherVersion() {
    try {
      const version = await window.__TAURI__?.app?.getVersion?.();
      const versionEl = document.getElementById("launcher-version");
      if (versionEl && version) {
        versionEl.textContent = `v${version}`;
      }
    } catch (e) {
      console.warn("Failed to get launcher version:", e);
    }
  },

  // Function to complete the first launch process
  completeFirstLaunch() {
    localStorage.setItem("isFirstLaunch", "false");
    this.setState({ isFirstLaunch: false });

    // Proceed with update check
    this.checkServerConnection().then((isConnected) => {
      if (isConnected) {
        this.initializeAndCheckUpdates(false);
      }
    });
  },

  // Function for custom notifications
  showCustomNotification(message, type) {
    const notification = document.createElement("div");
    notification.className = `custom-notification ${type}`;
    notification.textContent = message;
    document.body.appendChild(notification);

    anime({
      targets: notification,
      opacity: [0, 1],
      translateY: [-20, 0],
      duration: 300,
      easing: "easeOutQuad",
    });

    setTimeout(() => {
      anime({
        targets: notification,
        opacity: 0,
        translateY: -20,
        duration: 300,
        easing: "easeInQuad",
        complete: () => {
          notification.remove();
        },
      });
    }, 5000);
  },

  /**
   * Handles download progress events from the backend.
   * @param {Object} event The event object from the backend.
   * @param {Object} event.payload The payload of the event, containing the following properties:
   *   - file_name: The name of the file being downloaded.
   *   - progress: The percentage of the file downloaded.
   *   - speed: The download speed in bytes per second.
   *   - downloaded_bytes: The total number of bytes downloaded so far.
   *   - total_bytes: The total number of bytes to download.
   *   - total_files: The total number of files to download.
   *   - current_file_index: The index of the current file in the list of files to download.
   */
  handleDownloadProgress(event) {
    if (!event || !event.payload) {
      console.error(
        "Invalid event or payload received in handleDownloadProgress",
      );
      return;
    }

    const {
      file_name,
      progress,
      speed,
      downloaded_bytes,
      total_bytes,
      total_files,
      current_file_index,
    } = event.payload;

    // Ensure totalSize is initialized correctly (preserve larger value for resumed downloads)
    if (this.state.totalSize === undefined || this.state.totalSize === 0) {
      this.state.totalSize = total_bytes;
    }

    // Add offset for resumed downloads (bytes already downloaded before pause)
    const offset = this.state.downloadedBytesOffset || 0;
    const totalDownloadedBytes = downloaded_bytes + offset;

    // Use preserved totalSize if larger (for resumed downloads)
    const effectiveTotalSize = Math.max(this.state.totalSize, total_bytes);

    const now = Date.now();

    // Initialize download start time on first progress event
    if (this.state.downloadStartTime === null) {
      this.state.downloadStartTime = now;
    }

    // Calculate speed based on current session bytes only (not including offset)
    const elapsedSeconds = (now - this.state.downloadStartTime) / 1000;
    const globalSpeed =
      elapsedSeconds > 0 ? downloaded_bytes / elapsedSeconds : speed;

    const timeRemaining = this.calculateGlobalTimeRemaining(
      totalDownloadedBytes,
      effectiveTotalSize,
      globalSpeed,
    );
    const nextUpdateMode = getProgressUpdateMode({
      currentUpdateMode: this.state.currentUpdateMode,
      isDownloadComplete: this.state.isDownloadComplete,
      isUpdateAvailable: this.state.isUpdateAvailable,
    });

    // Smooth the displayed file name to the most active file over a short window
    if (!this._activeFileWindow) this._activeFileWindow = [];
    const nowTs = Date.now();
    this._activeFileWindow.push({ t: nowTs, name: file_name });
    this._activeFileWindow = this._activeFileWindow.filter(
      (s) => nowTs - s.t <= 1500,
    );
    // Cap at 100 entries to prevent unbounded growth in edge cases
    if (this._activeFileWindow.length > 100) {
      this._activeFileWindow = this._activeFileWindow.slice(-100);
    }
    const freq = {};
    for (const s of this._activeFileWindow)
      freq[s.name] = (freq[s.name] || 0) + 1;
    let topName = file_name;
    let topCount = 0;
    for (const k in freq) {
      if (freq[k] > topCount) {
        topCount = freq[k];
        topName = k;
      }
    }

    this.setState({
      currentFileName: topName,
      currentProgress: Math.min(
        100,
        (totalDownloadedBytes / effectiveTotalSize) * 100,
      ),
      currentSpeed: globalSpeed,
      downloadedSize: totalDownloadedBytes,
      totalFiles: total_files,
      currentFileIndex: current_file_index,
      totalDownloadedBytes: totalDownloadedBytes,
      timeRemaining: timeRemaining,
      currentUpdateMode: nextUpdateMode,
      lastProgressUpdate: now,
      lastDownloadedBytes: totalDownloadedBytes,
    });
  },

  /**
   * Handles file check progress events from the backend.
   * @param {Object} event The event object from the backend.
   * @param {Object} event.payload The payload of the event, containing the following properties:
   *   - current_file: The name of the file being checked.
   *   - progress: The percentage of the file check completed.
   *   - current_count: The number of files checked so far.
   *   - total_files: The total number of files to check.
   */
  handleFileCheckProgress(event) {
    if (!event || !event.payload) {
      console.error(
        "Invalid event or payload received in file_check_progress listener",
      );
      return;
    }

    const { current_file, progress, current_count, total_files } =
      event.payload;

    this.setState({
      currentFileName: current_file,
      currentProgress: Math.min(100, progress),
      currentFileIndex: current_count,
      totalFiles: total_files,
      currentUpdateMode: "file_check",
    });

    // Update new checking state UI
    if (typeof window.showCheckingState === 'function') {
      window.showCheckingState(current_count, total_files);
    }
  },

  /**
   * Handles file check completed events from the backend.
   * @param {Object} event The event object from the backend.
   * @param {Object} event.payload The payload of the event, containing the following properties:
   *   - total_files: The total number of files to check.
   *   - files_to_update: The number of files that require an update.
   *   - total_time_seconds: The total time taken to check all the files in seconds.
   *   - average_time_per_file_ms: The average time taken to check each file in milliseconds.
   */
  handleFileCheckCompleted(event) {
    const {
      total_files,
      files_to_update,
      total_time_seconds,
      average_time_per_file_ms,
    } = event.payload;
    // Mark file check done; only complete if there is nothing to download
    const hasUpdates = (files_to_update ?? 0) > 0;
    this.setState({
      isFileCheckComplete: true,
      isUpdateAvailable: hasUpdates,
      // Don't change mode here - let checkForUpdates handle the transition
    });

    if (!hasUpdates) {
      // Only hide checking state and show ready when there are NO updates
      if (typeof window.hideCheckingState === 'function') {
        window.hideCheckingState();
      }
      this.handleCompletion();
    }
    // If hasUpdates is true, the download will start and showDownloadingState will handle the UI transition
  },

  /**
   * Handles update completed events from the backend.
   * Sets the state to indicate that the update is complete.
   */
  handleUpdateCompleted() {
    this.setState({
      isUpdateComplete: true,
      currentUpdateMode: "complete",
    });
  },

  /**
   * Requests an update of the UI elements by scheduling a call to updateUIElements
   * using requestAnimationFrame. This ensures that the UI is updated as soon as
   * possible after the state has changed, without causing unnecessary re-renders.
   * @return {void}
   */
  updateUI() {
    if (!this.deferredUpdate) {
      this.deferredUpdate = requestAnimationFrame(() => {
        this.updateUIElements();
        this.deferredUpdate = null;
      });
    }
  },

  /**
   * Updates the UI elements with the latest state. This function is
   * called when the state of the application changes.
   *
   * @return {void}
   */
  updateUIElements() {
    const elements = {
      statusString: document.getElementById("status-string"),
      currentFile: document.getElementById("current-file"),
      filesProgress: document.getElementById("files-progress"),
      downloadedSize: document.getElementById("downloaded-size"),
      totalSize: document.getElementById("total-size"),
      progressPercentage: document.getElementById("progress-percentage"),
      progressPercentageDiv: document.getElementById("progress-percentage-div"),
      downloadSpeed: document.getElementById("download-speed"),
      timeRemaining: document.getElementById("time-remaining"),
      dlStatusString: document.getElementById("dl-status-string"),
    };

    if (!UPDATE_CHECK_ENABLED) {
      //#GAMEUPDATER
      if (elements.dlStatusString)
        elements.dlStatusString.textContent = this.t("NO_UPDATE_REQUIRED");
      if (elements.progressPercentage)
        elements.progressPercentage.textContent = "(100%)";
      if (elements.progressPercentageDiv)
        elements.progressPercentageDiv.style.width = "100%";

      // Hide unnecessary elements
      if (elements.currentFile) elements.currentFile.style.display = "none";
      if (elements.filesProgress) elements.filesProgress.style.display = "none";
      const sizeProgress = document.getElementById("size-progress");
      if (sizeProgress) sizeProgress.style.display = "none";
      if (elements.downloadSpeed) elements.downloadSpeed.style.display = "none";
      if (elements.timeRemaining) elements.timeRemaining.style.display = "none";

      return; // Exit the function because we don't need to update other elements
    }

    this.updateTextContents(elements);
    this.updateProgressBar(elements);
    this.updateDownloadInfo(elements);
    this.updateElementsVisibility(elements);
  },

  /**
   * Updates the text content of the elements in the object with the relevant text from the state.
   * @param {Object} elements - An object containing the elements to be updated. Can contain the following properties:
   *      dlStatusString: The element to display the download status string.
   *      statusString: The element to display the status string.
   *      currentFile: The element to display the current file name.
   *      filesProgress: The element to display the progress of the file check (e.g. 10/100).
   *      downloadedSize: The element to display the downloaded size.
   *      totalSize: The element to display the total size.
   */
  updateTextContents(elements) {
    if (elements.dlStatusString) {
      elements.dlStatusString.textContent = this.getDlStatusString();
    }
    // Only update status-string in ready state - not during download/check/complete
    // The new UI handles status display through separate state divs
    const mode = this.state.currentUpdateMode;
    const skipStatusUpdate = mode === "download" || mode === "paused" || mode === "file_check" || mode === "complete";
    if (elements.statusString && !skipStatusUpdate) {
      elements.statusString.textContent = this.getStatusText();
    }
    if (elements.currentFile)
      elements.currentFile.textContent = this.getFileName(
        this.state.currentFileName,
      );
    if (elements.filesProgress)
      elements.filesProgress.textContent = `(${this.state.currentFileIndex}/${this.state.totalFiles})`;
    if (elements.downloadedSize)
      elements.downloadedSize.textContent = this.formatSize(
        this.state.downloadedSize,
      );
    if (elements.totalSize)
      elements.totalSize.textContent = this.formatSize(this.state.totalSize);
  },

  /**
   * Updates the progress bar elements in the object with the relevant progress.
   * @param {Object} elements - An object containing the elements to be updated. Can contain the following properties:
   *      progressPercentage: The element to display the progress percentage.
   *      progressPercentageDiv: The element to display the progress bar itself.
   *      currentFile: The element to display the current file name.
   */
  updateProgressBar(elements) {
    const progress = Math.min(100, this.calculateProgress());
    const showProgress =
      this.state.isUpdateAvailable &&
      (this.state.currentUpdateMode === "download" ||
        this.state.currentUpdateMode === "paused");

    // Update new UI state sections - only for download/pause states
    // Don't interfere with checking state
    const isCheckingMode = this.state.currentUpdateMode === "file_check";
    const statusDownloading = document.getElementById("status-downloading");
    const btnPauseResume = document.getElementById("btn-pause-resume");

    if (!isCheckingMode && statusDownloading) {
      if (showProgress) {
        // Use the proper state function to handle transitions
        const isPaused = this.state.currentUpdateMode === "paused";
        if (isPaused && typeof window.showPausedState === 'function') {
          // Ensure downloading state is visible first, then update to paused
          if (!statusDownloading.classList.contains('active')) {
            statusDownloading.classList.add('active');
          }
          window.showPausedState();
        } else if (typeof window.showDownloadingState === 'function') {
          window.showDownloadingState();
        }
      } else {
        statusDownloading.classList.remove("active");
        // Don't auto-show ready state here - let the proper state functions handle it
      }
    }

    // Show/hide pause button
    if (btnPauseResume) {
      if (showProgress && !isCheckingMode) {
        btnPauseResume.classList.add("active");
      } else {
        btnPauseResume.classList.remove("active");
      }
    }

    if (elements.progressPercentage) {
      if (!showProgress) {
        elements.progressPercentage.style.display = "none";
      } else {
        elements.progressPercentage.style.display = "inline";
        elements.progressPercentage.textContent = `${Math.round(progress)}%`;
      }
    }
    if (elements.progressPercentageDiv && showProgress) {
      elements.progressPercentageDiv.style.width = `${progress}%`;
    }
    if (elements.currentFile) {
      elements.currentFile.style.display = showProgress ? "flex" : "none";
    }
    const sizeProgress = document.getElementById("size-progress");
    if (sizeProgress) {
      sizeProgress.style.display = showProgress ? "flex" : "none";
    }
  },

  /**
   * Updates the download info elements in the object with the relevant download information.
   * @param {Object} elements - An object containing the elements to be updated. Can contain the following properties:
   *      downloadSpeed: The element to display the download speed.
   *      timeRemaining: The element to display the time remaining.
   */
  updateDownloadInfo(elements) {
    if (elements.downloadSpeed) {
      const speedText =
        this.state.currentUpdateMode === "download"
          ? this.formatSpeed(this.state.currentSpeed)
          : "";
      elements.downloadSpeed.textContent = speedText;
    }
    if (elements.timeRemaining) {
      const timeText =
        this.state.currentUpdateMode === "download"
          ? this.formatTime(this.state.timeRemaining)
          : "";
      elements.timeRemaining.textContent = timeText;
    }
  },

  /**
   * Returns the current download status string based on the current update mode.
   * This function will return the following strings based on the current update mode:
   *      'file_check': 'VERIFYING_FILES'
   *      'download': 'DOWNLOADING_FILES'
   *      'complete': If the file check is complete and there is no update available, 'NO_UPDATE_REQUIRED'
   *                  If the file check is complete and there is an update available, 'FILE_CHECK_COMPLETE'
   *                  If the download is complete, 'DOWNLOAD_COMPLETE'
   *                  If the update is complete, 'UPDATE_COMPLETED'
   *      default: 'GAME_READY_TO_LAUNCH'
   *
   * @returns {string} The current download status string
   */
  getDlStatusString() {
    if (!UPDATE_CHECK_ENABLED) {
      return this.t("NO_UPDATE_REQUIRED");
    }
    return this.t(getDlStatusKey(this.state));
  },

  /**
   * Calculates the current progress of the update as a percentage.
   * If there is an update available and the total size of the update is greater than 0,
   * the progress is calculated as (downloadedSize / totalSize) * 100.
   * Otherwise, the current progress is returned.
   * @returns {number} The current progress as a percentage
   */
  calculateProgress() {
    if (this.state.isUpdateAvailable && this.state.totalSize > 0) {
      return (this.state.downloadedSize / this.state.totalSize) * 100;
    }
    return this.state.currentProgress;
  },

  /**
   * Returns the current download status string based on the current update mode.
   * If the download is complete, 'DOWNLOAD_COMPLETE' is returned.
   * If there is no update available, 'NO_UPDATE_REQUIRED' is returned.
   * If the file check is being performed, 'VERIFYING_FILES' is returned.
   * If the download is being performed, 'DOWNLOADING_FILES' is returned.
   * @returns {string} The current download status string
   */
  getStatusText() {
    return this.t(getStatusKey(this.state));
  },

  /**
   * Updates the visibility of the given elements based on the current state of the download.
   * If the download is available and the current update mode is 'download',
   * the elements are shown. Otherwise, they are hidden.
   * @param {Object} elements - The elements to update.
   */
  updateElementsVisibility(elements) {
    const isDownloading = this.state.currentUpdateMode === "download";
    const isPaused = this.state.currentUpdateMode === "paused";
    const showDownloadInfo =
      this.state.isUpdateAvailable &&
      (isDownloading || isPaused) &&
      !this.state.updateError;

    if (elements.currentFile)
      elements.currentFile.style.display = this.state.isUpdateAvailable
        ? "flex"
        : "none";
    if (elements.filesProgress)
      elements.filesProgress.style.display = this.state.isUpdateAvailable
        ? "inline"
        : "none";

    if (elements.progressPercentage) {
      elements.progressPercentage.style.display =
        this.state.isUpdateAvailable &&
        this.state.currentUpdateMode !== "ready" &&
        !this.state.updateError
          ? "inline"
          : "none";
    }
    if (elements.downloadSpeed) {
      elements.downloadSpeed.style.display = isDownloading ? "inline" : "none";
    }
    if (elements.timeRemaining) {
      elements.timeRemaining.style.display = isDownloading ? "inline" : "none";
    }

    const speedLabel = document.querySelector(".dl-speed-string");
    if (speedLabel) {
      speedLabel.style.display = isDownloading ? "inline" : "none";
    }
    const timeLabel = document.querySelector(".tr-string");
    if (timeLabel) {
      timeLabel.style.display = isDownloading ? "inline" : "none";
    }

    const pauseBtn = document.querySelector(".btn-pause");
    if (pauseBtn) {
      pauseBtn.style.display = showDownloadInfo ? "flex" : "none";
      pauseBtn.style.pointerEvents = this.state.isPauseRequested
        ? "none"
        : "auto";
      const icon = pauseBtn.querySelector("img");
      if (icon) {
        icon.src = isPaused
          ? "./assets/vector-3.svg"
          : "./assets/pause-icon.svg";
        icon.alt = isPaused ? "Resume" : "Pause";
      }
      pauseBtn.title = isPaused ? this.t("RESUME") : this.t("PAUSE");
    }
  },

  /**
   * Resets the state to its initial values.
   * This function is called on various events such as the download completing, the user logging out, or the user navigating away from the page.
   * It resets all the state fields to their default values, effectively resetting the state of the download.
   */
  resetState() {
    // Cancel any pending download timeout
    if (this.pendingDownloadTimeout) {
      clearTimeout(this.pendingDownloadTimeout);
      this.pendingDownloadTimeout = null;
    }
    this.setState({
      isFileCheckComplete: false,
      isUpdateAvailable: false,
      isDownloadComplete: false,
      lastProgressUpdate: null,
      lastDownloadedBytes: 0,
      downloadStartTime: null,
      currentUpdateMode: null,
      currentProgress: 0,
      currentFileName: "",
      currentFileIndex: 0,
      totalFiles: 0,
      downloadedSize: 0,
      downloadedBytesOffset: 0,
      totalSize: 0,
      currentSpeed: 0,
      timeRemaining: 0,
      isLoggingIn: false,
      isLoggingOut: false,
      isGameRunning: false,
      updateCheckPerformed: false,
      isGeneratingHashFile: false,
      hashFileProgress: 0,
      currentProcessingFile: "",
      processedFiles: 0,
      isPauseRequested: false,
      updateError: false,
      isTogglingPauseResume: false,
      isDownloading: false,
    });
    this._activeFileWindow = [];
  },

  toggleTheme() {
    const body = document.body;
    if (body.classList.contains("light-mode")) {
      body.classList.remove("light-mode");
      localStorage.setItem("theme", "dark");
    } else {
      body.classList.add("light-mode");
      localStorage.setItem("theme", "light");
    }
  },


  /**
   * Handles download completion events from the backend.
   * Sets the state to indicate that the download is complete, and after a 2 second delay, sets the state to indicate that the update is complete.
   * Also re-enables the game launch button and language selector.
   */
  handleCompletion() {
    console.log(">>> handleCompletion() called, updateError:", this.state.updateError);

    // CRITICAL: If there was an error, do NOT clear it or complete the download.
    // The error state should remain visible so users know something went wrong.
    if (this.state.updateError) {
      console.log(">>> handleCompletion() aborted: updateError is true, keeping error state visible");
      return;
    }

    // First, ensure progress bar shows 100% before hiding
    // Set downloadedSize equal to totalSize to show 100% progress
    this.setState({
      downloadedSize: this.state.totalSize,
      currentProgress: 100,
    });

    // Force update the progress bar to 100% visually
    const progressPercentageDiv = document.getElementById("progress-percentage-div");
    if (progressPercentageDiv) {
      progressPercentageDiv.style.width = "100%";
    }
    const progressPercentage = document.getElementById("progress-percentage");
    if (progressPercentage) {
      progressPercentage.textContent = "(100%)";
    }

    // Brief delay to show 100% progress before transitioning
    setTimeout(() => {
      // Double-check error state hasn't been set during the timeout
      if (this.state.updateError) {
        console.log(">>> handleCompletion() timeout aborted: updateError became true");
        return;
      }

      this.setState({
        isDownloadComplete: true,
        currentUpdateMode: "complete",
        isUpdateAvailable: false,
        isFileCheckComplete: true,
        updateError: false,
      });
      // Re-enable controls immediately, then transition to ready after delay
      this.updateLaunchGameButton(false);
      this.toggleLanguageSelector(true);
      setTimeout(() => {
        // Triple-check error state
        if (this.state.updateError) {
          return;
        }
        this.setState({
          isUpdateComplete: true,
          currentUpdateMode: "ready",
        });
        // Show the ready state UI
        if (typeof window.showReadyState === 'function') {
          window.showReadyState();
        }
      }, 1500);
    }, 500);
  },

  /**
   * Initializes the home page and checks for updates if needed.
   * If the first launch flag is set, it handles the first launch by generating the hash file.
   * If not, it checks for updates and sets the state accordingly.
   * If an error occurs during initialization and update check, it logs the error but does not display it to the user.
   * @param {boolean} [isLogin=false] Whether the update check is triggered by a login action.
   */
  async initializeAndCheckUpdates(isLogin = false) {
    if (!UPDATE_CHECK_ENABLED) {
      this.setState({
        isUpdateAvailable: false,
        isFileCheckComplete: true,
        currentUpdateMode: "complete",
        currentProgress: 100,
      });
      // Update the status UI to show ready state since no checking is needed
      if (typeof window.hideCheckingState === 'function') {
        window.hideCheckingState();
      }
      this.updateLaunchGameButton(false);
      return;
    }

    const checkNeeded = isLogin
      ? !this.state.updateCheckPerformedOnLogin
      : !this.state.updateCheckPerformedOnRefresh;

    if (!checkNeeded) {
      // Update check already performed - clear checking state and show ready
      if (typeof window.hideCheckingState === 'function') {
        window.hideCheckingState();
      }
      return;
    }

    try {
      await this.initializeHomePage();
      this.checkFirstLaunch();
      const folderReady = await this.ensureGameFolderValid();
      if (folderReady) {
        await this.checkForUpdates();
      }

      if (isLogin) {
        this.setState({ updateCheckPerformedOnLogin: true });
      } else {
        this.setState({ updateCheckPerformedOnRefresh: true });
      }
    } catch (error) {
      console.error("Error during initialization and update check:", error);
      this.showErrorMessage(this.t("UPDATE_CHECK_FAILED") || "Failed to check for updates. Please try again.");
      this.setState({
        currentUpdateMode: "error",
        updateError: true,
        isCheckingForUpdates: false
      });
      // Clear checking state on error
      if (typeof window.hideCheckingState === 'function') {
        window.hideCheckingState();
      }
    }
  },

  /**
   * Checks for updates if needed. If no update is needed, it disables the update check button and
   * sets the state to indicate that the update is complete. If an update is needed, it sets the
   * state to indicate that the update is available and starts the update process.
   * If an error occurs, it logs the error and displays an error message to the user.
   * @param {boolean} [isLogin=false] Whether the update check is triggered by a login action.
   */
  async checkForUpdates() {
    if (!UPDATE_CHECK_ENABLED) {
      this.setState({
        isUpdateAvailable: false,
        isFileCheckComplete: true,
        currentUpdateMode: "complete",
        currentProgress: 100,
      });
      return;
    }

    if (this.state.isCheckingForUpdates) return;

    // Reset state first, then set file_check mode (order matters to avoid race condition)
    this.resetState();
    this.setState({
      isCheckingForUpdates: true,
      currentUpdateMode: "file_check",
    });
    // Disable the game launch button and language selector during the check
    this.updateLaunchGameButton(true);
    this.toggleLanguageSelector(false);

    // Show checking state UI
    if (typeof window.showCheckingState === 'function') {
      window.showCheckingState(0, 0);
    }

    try {
      const filesToUpdate = await invoke("get_files_to_update");

      if (filesToUpdate.length === 0) {
        this.setState({
          isUpdateAvailable: false,
          isFileCheckComplete: true,
          currentUpdateMode: "complete",
        });
        // Hide checking state UI
        if (typeof window.hideCheckingState === 'function') {
          window.hideCheckingState();
        }
        // Re-enable elements if no update is needed
        this.updateLaunchGameButton(false);
        this.toggleLanguageSelector(true);
        setTimeout(() => {
          this.setState({ currentUpdateMode: "ready" });
        }, 1000);
      } else {
        this.setState({
          isUpdateAvailable: true,
          isFileCheckComplete: true,
          currentUpdateMode: "complete",
          totalFiles: filesToUpdate.length,
          totalSize: filesToUpdate.reduce(
            (total, file) => total + file.size,
            0,
          ),
        });
        // Start download immediately
        this.setState({ currentUpdateMode: "download" });
        await this.runPatchSystem(filesToUpdate);
      }
    } catch (error) {
      console.error("Error checking for updates:", error);
      this.resetState();
      this.showErrorMessage(this.t("UPDATE_SERVER_UNREACHABLE"));
      // Hide checking state UI on error
      if (typeof window.hideCheckingState === 'function') {
        window.hideCheckingState();
      }
      // Re-enable elements in case of error
      this.updateLaunchGameButton(false);
      this.toggleLanguageSelector(true);
    } finally {
      this.setState({ isCheckingForUpdates: false });
    }
  },

  /**
   * Runs the patch system to download and install updates.
   *
   * The method disables the game launch button and language selector at the start of the process, and
   * re-enables them at the end of the process. If no updates are needed, the method simply returns without
   * doing anything else. If an error occurs during the update process, the method shows an error message
   * and re-enables the game launch button and language selector.
   *
   * @param {Array.<FileInfo>} filesToUpdate - The list of files to update.
   *
   * @returns {Promise<void>}
   */
  async runPatchSystem(filesToUpdate) {
    if (!UPDATE_CHECK_ENABLED) return;

    // Prevent concurrent download attempts
    if (this.state.isDownloading) {
      console.log("Download already in progress, skipping duplicate call");
      return;
    }
    this.setState({ isDownloading: true });

    let pausedDuringDownload = false;
    try {
      this.updateLaunchGameButton(true);
      this.toggleLanguageSelector(false);

      if (filesToUpdate.length === 0) {
        this.updateLaunchGameButton(false);
        this.toggleLanguageSelector(true);
        return;
      }

      // Transition UI from checking to downloading state
      if (typeof window.showDownloadingState === 'function') {
        window.showDownloadingState();
      }

      const downloadedSizes = await invoke("download_all_files", {
        filesToUpdate: filesToUpdate,
        resumeDownloaded: this.state.downloadedSize || 0,
      });

      // If user paused during the backend call, stop here without completing
      if (this.state.currentUpdateMode === "paused") {
        pausedDuringDownload = true;
        return;
      }

      let totalDownloadedSize = 0;
      let lastUpdateTime = Date.now();
      let lastDownloadedSize = 0;
      for (let i = 0; i < downloadedSizes.length; i++) {
        const fileInfo = filesToUpdate[i];
        const downloadedSize = downloadedSizes[i];
        totalDownloadedSize += downloadedSize;

        this.setState({
          currentFileName: fileInfo.path,
          currentFileIndex: i + 1,
          downloadedSize: totalDownloadedSize,
        });

        const currentTime = Date.now();
        const timeDiff = (currentTime - lastUpdateTime) / 1000; // in seconds
        const sizeDiff = totalDownloadedSize - lastDownloadedSize;
        const speed = sizeDiff / timeDiff; // bytes per second

        // Emit a progress event if necessary
        this.handleDownloadProgress({
          payload: {
            file_name: fileInfo.path,
            progress: (totalDownloadedSize / this.state.totalSize) * 100,
            speed: speed,
            downloaded_bytes: totalDownloadedSize,
            total_bytes: this.state.totalSize,
            total_files: this.state.totalFiles,
            current_file_index: i + 1,
          },
        });

        lastUpdateTime = currentTime;
        lastDownloadedSize = totalDownloadedSize;
      }

      // Do not mark completion here; wait for backend "download_complete" event
      if (this.state.currentUpdateMode === "paused") {
        pausedDuringDownload = true;
      }
    } catch (error) {
      console.error("Error during update:", error);

      // Handle "Download already in progress" error by resetting state and retrying
      const errorStr = String(error);
      if (errorStr.includes("Download already in progress")) {
        console.log("Download state conflict detected, resetting and retrying...");
        try {
          await invoke("reset_download_state");
          // Reset frontend state and retry
          this.setState({ isDownloading: false });
          // Small delay to ensure state is fully reset
          await new Promise(resolve => setTimeout(resolve, 100));
          // Retry the download
          return this.runPatchSystem(filesToUpdate);
        } catch (resetError) {
          console.error("Failed to reset download state:", resetError);
        }
      }

      const message = getUpdateErrorMessage(
        error,
        this.t("UPDATE_ERROR_MESSAGE"),
      );
      this.showErrorMessage(message);
      this.setState({
        updateError: true,
        currentUpdateMode: "error",
        isUpdateAvailable: true,
        isFileCheckComplete: true,
      });
    } finally {
      // Always reset isDownloading flag
      this.setState({ isDownloading: false });

      // Re-enable controls only if not paused
      if (!pausedDuringDownload && this.state.currentUpdateMode !== "paused") {
        this.updateLaunchGameButton(false);
        this.toggleLanguageSelector(true);
      } else {
        // Keep launch disabled while paused
        this.updateLaunchGameButton(true);
      }
    }
  },

  /**
   * Logs in to the game server using the given username and password.
   *
   * If a login attempt is already in progress, this function will not do anything.
   *
   * @param {string} username - The username to use for login
   * @param {string} password - The password to use for login
   *
   * @return {Promise<void>}
   */
  async login(username, password) {
    if (this.state.isLoggingIn) return;

    this.setState({ isLoggingIn: true });

    try {
      const response = await invoke("login", { username, password });
      const jsonResponse = JSON.parse(response);

      if (
        jsonResponse &&
        jsonResponse.Return &&
        jsonResponse.Msg === "success"
      ) {
        const jsonResponseFormatted = {
          AuthKey: jsonResponse.Return.AuthKey,
          UserName: username,
          UserNo: Number(jsonResponse.Return.UserNo),
          CharacterCount: jsonResponse.Return.CharacterCount,
          Permission: Number(jsonResponse.Return.Permission),
          Privilege: 0,
        };
        await this.storeAuthInfo(jsonResponseFormatted, username, password);

        // Store credentials for automatic re-login (base64 encoded for basic obfuscation)
        // Uses localStorage so credentials persist across launcher restarts
        localStorage.setItem('_cred', btoa(JSON.stringify({u: username, p: password})));

        if (!UPDATE_CHECK_ENABLED) {
          this.setState({
            isUpdateAvailable: false,
            isFileCheckComplete: true,
            currentUpdateMode: "complete",
            currentProgress: 100,
          });
          await this.Router.navigate("home");
          LoadStartPage();
          return;
        }

        const isConnected = await this.checkServerConnection();
        if (!isConnected) {
          throw new Error(this.t("SERVER_CONNECTION_ERROR"));
        }
        await this.initializeAndCheckUpdates(true);
        await this.Router.navigate("home");
        LoadStartPage();
      } else {
        const errorMessage = jsonResponse
          ? jsonResponse.Msg || this.t("LOGIN_ERROR")
          : this.t("LOGIN_ERROR");
        throw new Error(errorMessage);
      }
    } catch (error) {
      console.error("Error during login:", error);
      const message = error && (error.message || error);
      let errorText;
      if (message === "INVALID_CREDENTIALS") {
        errorText = this.t("LOGIN_ERROR") || "Invalid username or password";
      } else if (message && typeof message === "string") {
        errorText = message;
      } else {
        errorText = this.t("SERVER_CONNECTION_ERROR") || "Connection error";
      }
      // Show error via toast notification
      window.showUpdateNotification('error', this.t('LOGIN_FAILED') || 'Login Failed', errorText);
    } finally {
      this.setState({ isLoggingIn: false });
    }
  },

  /**
   * Stores the authentication info in local storage and
   * informs the backend to set the authentication info
   * @param {Object} jsonResponse - The JSON response from the server
   * @param {string} jsonResponse.AuthKey - The authorization key
   * @param {string} jsonResponse.UserName - The username
   * @param {number} jsonResponse.UserNo - The user number
   * @param {string} jsonResponse.CharacterCount - The character count
   * @param {number} jsonResponse.Permission - The permission level
   * @param {number} jsonResponse.Privilege - The privilege level
   */
  async storeAuthInfo(jsonResponse, username, password) {
    localStorage.setItem("authKey", jsonResponse.AuthKey);
    localStorage.setItem("userName", jsonResponse.UserName);
    localStorage.setItem("userNo", jsonResponse.UserNo.toString());
    localStorage.setItem(
      "characterCount",
      jsonResponse.CharacterCount.toString(),
    );
    localStorage.setItem("permission", jsonResponse.Permission.toString());
    localStorage.setItem("privilege", jsonResponse.Privilege.toString());

    try {
      await invoke("set_auth_info", {
        authKey: jsonResponse.AuthKey,
        userName: jsonResponse.UserName,
        userNo: jsonResponse.UserNo,
        characterCount: jsonResponse.CharacterCount,
      });
    } catch (error) {
      console.error("Failed to set auth info in backend:", error);
      // Continue anyway - localStorage has the data as backup
    }

    // Also update AccountManager for multi-account support
    if (username && password) {
      const credentials = btoa(JSON.stringify({ u: username, p: password }));
      AccountManager.addAccount({
        userNo: jsonResponse.UserNo,
        userName: jsonResponse.UserName,
        credentials: credentials
      });
      AccountManager.setActiveAccountId(jsonResponse.UserNo);
    }

    this.checkAuthentication();
  },

  /**
   * Navigates to the home page and initializes it
   *
   * @returns {Promise<void>}
   */
  async initializeHomePage() {
    this.Router.navigate("home");
    await this.waitForHomePage();
    await this.initHome();
  },

  /**
   * Waits until the home page is loaded and resolves the promise
   * @param {number} maxWaitMs - Maximum time to wait in milliseconds (default: 10000)
   * @returns {Promise<void>}
   */
  waitForHomePage(maxWaitMs = 10000) {
    return new Promise((resolve) => {
      const startTime = Date.now();
      const checkDom = () => {
        if (document.getElementById("home-page")) {
          resolve();
        } else if (Date.now() - startTime > maxWaitMs) {
          console.warn("Timeout waiting for home page element");
          resolve(); // Resolve anyway to not block the app
        } else {
          setTimeout(checkDom, 100);
        }
      };
      checkDom();
    });
  },

  /**
   * Logs out the user and resets the state
   *
   * This method waits until a logout is not already in progress, then
   * sets the isLoggingOut state variable to true and calls the
   * backend's logout handler. After the logout is successful, it
   * removes all locally stored authentication information, resets
   * the state, and navigates to the login page.
   *
   * @returns {Promise<void>}
   */
  async logout() {
    if (this.state.isLoggingOut) return;

    // Close settings dropdown immediately
    const settingsWrapper = document.getElementById("settings-dropdown-wrapper");
    if (settingsWrapper) settingsWrapper.classList.remove("active");

    this.setState({ isLoggingOut: true });
    try {
      await invoke("handle_logout");
      localStorage.removeItem("authKey");
      localStorage.removeItem("userName");
      localStorage.removeItem("userNo");
      localStorage.removeItem("characterCount");
      localStorage.removeItem("permission");
      localStorage.removeItem("privilege");

      // Clear stored credentials for auto re-login
      localStorage.removeItem("_cred");

      // Clear active account in AccountManager (keep accounts list for easy re-login)
      AccountManager.clearActiveAccount();

      // Clean up event listeners to prevent memory leaks
      this.cleanupUpdateListeners();
      this.cleanupGameStatusListeners();
      this.cleanupErrorListener();
      this.cleanupMutationObserver();

      this.setState({
        updateCheckPerformed: false,
        updateCheckPerformedOnLogin: false,
        updateCheckPerformedOnRefresh: false,
        isAuthenticated: false,
      });

      // Update UI to reflect logged out state
      this.updateAccountDisplay();
      this.updateLaunchButtonState();

      // Stay on current page, just update auth state in header
      this.checkAuthentication();
    } catch (error) {
      console.error("Error during logout:", error);
    } finally {
      this.setState({ isLoggingOut: false });
    }
  },

  /**
   * Changes the language used in the launcher to the given language and
   * updates the UI to reflect the new language.
   *
   * @param {string} newLang - The new language to use. Must be one of the
   *     keys in the languages object.
   *
   * @returns {Promise<void>}
   */
  async changeLanguage(newLang) {
    if (newLang !== this.currentLanguage) {
      this.currentLanguage = newLang;
      await invoke("save_language_to_config", {
        language: this.currentLanguage,
      });
      await this.loadTranslations();
      await this.updateAllUIElements();
      const isGameRunning = await invoke("get_game_status");
      this.setState({ isGameRunning: isGameRunning });
    }
  },

  /**
   * Updates all UI elements to reflect the current state of the launcher. This
   * involves calling updateAllTranslations to update all the translations, and
   * then calling updateUI to update the actual UI elements.
   *
   * @returns {Promise<void>}
   */
  async updateAllUIElements() {
    await this.updateAllTranslations();
    this.updateUI();
  },

  /**
   * Updates the dynamic UI elements (i.e., the game status and the launch
   * game button) with the current translations.
   *
   * @returns {void}
   */
  updateDynamicTranslations() {
    if (this.statusEl) {
      this.statusEl.textContent = this.t(
        this.state.isGameRunning
          ? "GAME_STATUS_RUNNING"
          : "GAME_STATUS_NOT_RUNNING",
      );
    }
    // Update only the text span inside the launch button, not the entire button
    // (preserves SVG icons)
    const launchBtnText = document.getElementById("launch-btn-text");
    if (launchBtnText) {
      launchBtnText.textContent = this.t("LAUNCH_GAME");
    }
  },

  /**
   * Enables or disables the language selector UI element, depending on the
   * value of the `enable` parameter. If `enable` is true, the language selector
   * will be enabled and the user will be able to select a language. If `enable`
   * is false, the language selector will be disabled and the user will not be
   * able to select a language.
   *
   * @param {boolean} enable If true, the language selector will be enabled.
   *                          If false, the language selector will be disabled.
   * @returns {void}
   */
  toggleLanguageSelector(enable) {
    const selectWrapper = document.querySelector(".select-wrapper");
    const selectStyled = selectWrapper?.querySelector(".select-styled");

    if (selectWrapper && selectStyled) {
      if (enable) {
        selectWrapper.classList.remove("disabled");
        selectStyled.style.pointerEvents = "auto";
      } else {
        selectWrapper.classList.add("disabled");
        selectStyled.style.pointerEvents = "none";
      }
    }
  },

  /**
   * Handles the game launch process. If updates are available, it prevents
   * the game from launching until the updates are applied. If the game is
   * already launching, it does nothing. Otherwise, it sets the game status
   * to "launching" and initiates the game launch process by calling the
   * `handle_launch_game` command. If the game launch process fails, it sets
   * the game status to "not running" and resets the launch state.
   *
   * @returns {void}
   */
  /**
   * Silently refreshes authentication using stored credentials.
   * Does not update UI or navigate - just refreshes the backend auth state.
   *
   * @param {string} username - The username
   * @param {string} password - The password
   * @returns {Promise<boolean>} True if refresh succeeded, false otherwise
   */
  async silentAuthRefresh(username, password) {
    try {
      const response = await invoke("login", { username, password });
      const jsonResponse = JSON.parse(response);

      if (jsonResponse && jsonResponse.Return && jsonResponse.Msg === "success") {
        const authKey = jsonResponse.Return.AuthKey;
        const userNo = Number(jsonResponse.Return.UserNo);
        const characterCount = jsonResponse.Return.CharacterCount;

        // Update localStorage so isAuthenticated checks pass
        localStorage.setItem("authKey", authKey);
        localStorage.setItem("userName", username);
        localStorage.setItem("userNo", userNo.toString());
        localStorage.setItem("characterCount", characterCount.toString());

        // Store auth info in backend
        await invoke("set_auth_info", {
          authKey: authKey,
          userName: username,
          userNo: userNo,
          characterCount: characterCount,
        });

        // Update app state
        this.setState({ isAuthenticated: true });

        // Update header UI
        if (typeof window.updateIndexHeaderAuthState === "function") {
          window.updateIndexHeaderAuthState(true, username);
        }

        return true;
      }
      return false;
    } catch (error) {
      console.error("Silent auth refresh failed:", error);
      return false;
    }
  },

  async handleLaunchGame() {
    // Log all relevant state for debugging
    console.log("handleLaunchGame called with state:", {
      isAuthenticated: this.state.isAuthenticated,
      isUpdateAvailable: this.state.isUpdateAvailable,
      isDownloadComplete: this.state.isDownloadComplete,
      currentUpdateMode: this.state.currentUpdateMode,
      updateError: this.state.updateError,
      isGameLaunching: this.state.isGameLaunching,
      isFileCheckComplete: this.state.isFileCheckComplete,
      activeAccount: AccountManager.getActiveAccount()?.userName || 'none'
    });

    // Early exit if not authenticated
    if (!this.state.isAuthenticated) {
      console.log("BLOCKED: not authenticated");
      window.showUpdateNotification?.('error', this.t('LOGIN_REQUIRED') || 'Login Required', this.t('PLEASE_LOGIN_FIRST') || 'Please log in to play');
      return;
    }

    // First-launch Mods onboarding — show once before the user's first Launch
    // after upgrading. Kept here (before update/launching gates) so it always
    // fires on the first authenticated Launch click, regardless of update
    // state or a stuck isGameLaunching flag. Wrapped in try/catch so a DOM
    // hiccup can't break the launch path entirely.
    try {
      if (this.maybeShowModsOnboarding && this.maybeShowModsOnboarding()) {
        console.log("BLOCKED: mods onboarding shown (pre-gate)");
        return;
      }
    } catch (e) {
      console.warn("mods onboarding show failed (non-fatal):", e);
    }

    if (UPDATE_CHECK_ENABLED && this.state.isUpdateAvailable) {
      console.log("BLOCKED: update still available");
      window.showUpdateNotification?.('warning', this.t('UPDATE_REQUIRED') || 'Update Required', this.t('PLEASE_WAIT_FOR_UPDATE') || 'Please wait for the update to complete');
      return;
    }
    if (this.state.isGameLaunching) {
      console.log("BLOCKED: already launching");
      return;
    }

    // Set launching flag immediately to prevent double-clicks/race conditions
    // This must happen BEFORE any async operations
    this.setState({ isGameLaunching: true });

    // Check if active account already has a running game
    const activeAccount = AccountManager.getActiveAccount();
    if (activeAccount && AccountManager.isAccountInGame(activeAccount.userNo)) {
      window.showUpdateNotification('warning', this.t('ALREADY_RUNNING') || 'Already Running', this.t('ACCOUNT_ALREADY_RUNNING') || 'This account already has a game running');
      this.setState({ isGameLaunching: false });
      return;
    }

    // Check leaderboard consent (skip if we're proceeding after consent modal)
    if (!this._proceedWithLaunch) {
      const needsConsent = await this.checkLeaderboardConsent();
      if (needsConsent) {
        console.log("Showing leaderboard consent modal");
        this.setState({ isGameLaunching: false }); // Reset before showing modal
        this.openLeaderboardConsentModal();
        return; // Wait for user to respond to consent modal
      }
    }
    // Reset the flag for next launch
    this._proceedWithLaunch = false;

    try {
      this.updateUIForGameStatus(true);
      if (this.statusEl) this.statusEl.textContent = this.t("LAUNCHING_GAME");

      // Silently refresh auth before launching to ensure valid session
      const activeAccountForAuth = AccountManager.getActiveAccount();

      if (activeAccountForAuth && activeAccountForAuth.authMethod === 'oauth') {
        // OAuth account — check if we have stored auth info from last OAuth login
        const storedAuthKey = localStorage.getItem('authKey');
        if (storedAuthKey) {
          console.log("OAuth account — using stored auth key for launch");
        } else {
          // No stored auth — need to re-authenticate via OAuth
          console.log("OAuth account — no stored auth, triggering OAuth re-auth for launch");
          window.showUpdateNotification('info', 'Re-authentication Required', 'Opening browser to re-authenticate...');
          const provider = activeAccountForAuth.provider || 'google';
          startOAuth(provider, 'launch');
          this.setState({ isGameLaunching: false });
          this.updateUIForGameStatus(false);
          return;
        }
      } else if (activeAccountForAuth && activeAccountForAuth.credentials) {
        const cred = JSON.parse(atob(activeAccountForAuth.credentials));
        console.log("Refreshing auth before game launch...");
        const refreshed = await this.silentAuthRefresh(cred.u, cred.p);
        if (!refreshed) {
          console.error("Auth refresh failed");
          window.showUpdateNotification('error', this.t('LOGIN_FAILED') || 'Login Failed', this.t('PLEASE_REENTER_PASSWORD') || 'Please re-enter your password');
          this.openAddAccountModal(activeAccountForAuth.userName);
          this.setState({ isGameLaunching: false });
          this.updateUIForGameStatus(false);
          return;
        }
        console.log("Auth refreshed, launching game...");
      } else {
        // No active account with credentials - show add account modal
        console.log("No active account with credentials");
        window.showUpdateNotification('error', this.t('LOGIN_FAILED') || 'Login Failed', this.t('PLEASE_REENTER_PASSWORD') || 'Please re-enter your password');
        this.openAddAccountModal();
        this.setState({ isGameLaunching: false });
        this.updateUIForGameStatus(false);
        return;
      }

      await invoke("handle_launch_game");

      // Register this game as running for this account
      const launchedAccount = AccountManager.getActiveAccount();
      if (launchedAccount) {
        // Use timestamp as pseudo-ID since we don't get actual process ID from backend
        AccountManager.registerRunningGame(launchedAccount.userNo, Date.now());
        this.updateAccountDisplay();
        this.updateLaunchButtonState();
      }

      // Start a recovery interval that periodically syncs game status with backend
      // This recovers from stuck states when game is killed via taskbar or crashes
      this.startGameStatusRecoveryInterval();
    } catch (error) {
      console.error("Error initiating game launch:", error);

      const game_launch_error = this.t("GAME_LAUNCH_ERROR") + error.toString();

      await message(game_launch_error, {
        title: this.t("ERROR"),
        type: "error",
      });
      if (this.statusEl)
        this.statusEl.textContent = this.t(
          "GAME_LAUNCH_ERROR",
          error.toString(),
        );
      await invoke("reset_launch_state");
      this.updateUIForGameStatus(false);
      this.setState({ gameExecutionFailed: true });
    } finally {
      this.setState({ isGameLaunching: false });
    }
  },

  /**
   * Starts a recovery interval that periodically checks game status with the backend.
   * Stops automatically when the game is no longer running.
   * This recovers from stuck states when the game is killed via taskbar or crashes.
   */
  startGameStatusRecoveryInterval() {
    // Clear any existing interval
    if (this.gameStatusRecoveryInterval) {
      clearInterval(this.gameStatusRecoveryInterval);
      this.gameStatusRecoveryInterval = null;
    }

    // Check game status every 5 seconds while game is supposed to be running
    this.gameStatusRecoveryInterval = setInterval(async () => {
      try {
        // Use reconciliation to sync frontend with backend count
        await this.reconcileGameState();

        const isRunning = await invoke("get_game_status");

        // If no game is running, clear interval and ensure clean state
        if (!isRunning) {
          console.log("Game status recovery: no games running, clearing interval");
          clearInterval(this.gameStatusRecoveryInterval);
          this.gameStatusRecoveryInterval = null;

          // Ensure backend state is clean (only when NO games running)
          try {
            await invoke("reset_launch_state");
          } catch (e) {
            console.warn("Failed to reset launch state:", e);
          }
        }
      } catch (error) {
        console.warn("Game status recovery check failed:", error);
      }
    }, 5000); // Check every 5 seconds
  },

  /**
   * Updates the game status UI based on the current game status.
   *
   * The game status is retrieved by invoking the "get_game_status" command.
   * If the command fails, an error is logged and the game status is set to
   * "GAME_STATUS_ERROR".
   *
   * @memberof App
   */
  async updateGameStatus() {
    try {
      const isRunning = await invoke("get_game_status");
      this.updateUIForGameStatus(isRunning);
    } catch (error) {
      console.error("Error checking game status:", error);
      if (this.statusEl)
        this.statusEl.textContent = this.t("GAME_STATUS_ERROR");
    }
  },

  /**
   * Reconciles frontend AccountManager state with backend game count.
   * Called when a game_ended event fires to sync the two tracking systems.
   *
   * Strategy: Compare backend game count with frontend tracked count.
   * If backend has fewer games, remove excess accounts from frontend tracking.
   * We don't know WHICH account's game ended, so we remove the oldest ones first.
   */
  async reconcileGameState() {
    try {
      const backendCount = await invoke("get_running_game_count");
      const frontendGames = AccountManager.getRunningGames();
      const frontendAccounts = Object.keys(frontendGames);
      const frontendCount = frontendAccounts.length;

      console.log(`Game state reconciliation: backend=${backendCount}, frontend=${frontendCount}`);

      if (backendCount === 0) {
        // No games running - clear all frontend tracking
        AccountManager.clearAllRunningGames();
      } else if (backendCount < frontendCount) {
        // Some games ended but not all - remove oldest entries to match count
        // Sort by launchedAt timestamp (oldest first)
        const sorted = frontendAccounts.sort((a, b) => {
          return (frontendGames[a].launchedAt || 0) - (frontendGames[b].launchedAt || 0);
        });
        // Remove the oldest (frontendCount - backendCount) entries
        const toRemove = frontendCount - backendCount;
        for (let i = 0; i < toRemove && i < sorted.length; i++) {
          AccountManager.unregisterRunningGame(sorted[i]);
          console.log(`Unregistered game for account ${sorted[i]} (reconciliation)`);
        }
      }

      // Update UI
      this.updateAccountDisplay();
      this.updateLaunchButtonState();
      await this.updateGameStatus();
    } catch (error) {
      console.error("Error reconciling game state:", error);
      // Fall back to simple status update
      await this.updateGameStatus();
    }
  },

  /**
   * Updates the game status UI based on the current game status.
   *
   * For multi-account support, we check per-account game status:
   * - Only disable launch if the ACTIVE account has a running game
   * - Allow launch for other accounts even if a game is globally running
   *
   * @param {boolean} isRunning - whether ANY game is running (backend global status)
   * @memberof App
   */
  updateUIForGameStatus(isRunning) {
    // Note: We no longer clear all games here - that's handled by reconcileGameState()
    // when it confirms backend has 0 running games. This prevents race conditions
    // where this function is called before reconciliation completes.

    const activeAccount = AccountManager.getActiveAccount();
    // Check per-account status, not just global isRunning
    const activeAccountInGame = activeAccount && AccountManager.isAccountInGame(activeAccount.userNo);

    // For the active account, check per-account status
    if (activeAccountInGame) {
      // This account has a running game
      if (this.statusEl) {
        this.statusEl.textContent = this.t('IN_GAME') || 'In Game';
      }
      this.updateLaunchGameButton(true); // Disable launch
      this.toggleLanguageSelector(false);
    } else {
      // This account does NOT have a running game - allow launch
      if (this.statusEl) {
        this.statusEl.textContent = this.t("READY_TO_PLAY");
      }
      this.updateLaunchGameButton(false); // Enable launch
      this.toggleLanguageSelector(true);
    }
  },

  /**
   * Updates the launch game button UI based on the current game status.
   *
   * The launch game button is disabled or enabled based on the game status.
   * The "disabled" class is also toggled on the button based on the game status.
   *
   * @param {boolean} disabled - whether the game is running or not
   * @memberof App
   */
  updateLaunchGameButton(disabled) {
    if (!this.launchGameBtn) return;
    // Always disable if not authenticated
    if (!this.state.isAuthenticated) {
      this.launchGameBtn.disabled = true;
      this.launchGameBtn.classList.add("disabled");
      return;
    }
    const shouldDisable = shouldDisableLaunch({
      disabled,
      currentUpdateMode: this.state.currentUpdateMode,
      updateError: this.state.updateError,
    });
    this.launchGameBtn.disabled = shouldDisable;
    this.launchGameBtn.classList.toggle("disabled", shouldDisable);
  },

  /**
   * Updates the hash file generation progress UI based on the current game status.
   *
   * The hash file generation progress bar, current file being processed, and progress text are updated
   * based on the current game status. The modal title is also updated if necessary.
   *
   * @memberof App
   */
  updateHashFileProgressUI() {
    const modal = document.getElementById("hash-file-progress-modal");
    if (!modal || modal.style.display === "none") {
      return; // Ne pas mettre à jour si le modal n'est pas visible
    }

    const progressBar = modal.querySelector(".hash-progress-bar");
    const currentFileEl = modal.querySelector("#hash-file-current-file");
    const progressTextEl = modal.querySelector("#hash-file-progress-text");

    if (progressBar) {
      progressBar.style.width = `${this.state.hashFileProgress}%`;
      progressBar.textContent = `${Math.round(this.state.hashFileProgress)}%`;
    }

    if (currentFileEl) {
      const processingFileText = this.t("PROCESSING_FILE");
      currentFileEl.textContent = `${processingFileText}: ${this.state.currentProcessingFile}`;
    }

    if (progressTextEl) {
      const progressText = this.t("PROGRESS_TEXT");
      progressTextEl.textContent = `${progressText} ${this.state.processedFiles}/${this.state.totalFiles} (${this.state.hashFileProgress.toFixed(2)}%)`;
    }

    // Mettre à jour le titre du modal si nécessaire
    const modalTitle = modal.querySelector("h2");
    if (modalTitle) {
      modalTitle.textContent = this.t("GENERATING_HASH_FILE");
    }
  },

  /**
   * Checks if the game is currently running.
   *
   * @returns {Promise<boolean>} whether the game is running or not
   * @memberof App
   */
  async isGameRunning() {
    try {
      const isRunning = await invoke("get_game_status");
      return isRunning;
    } catch (error) {
      console.error("Error checking game status:", error);
      return false;
    }
  },

  /**
   * Checks if the server is currently reachable.
   *
   * @returns {Promise<boolean>} whether the server is reachable or not
   * @memberof App
   */
  async checkServerConnection() {
    this.showLoadingModal(this.t("CHECKING_SERVER_CONNECTION"));
    try {
      const isConnected = await invoke("check_server_connection");
      this.hideLoadingModal();
      return isConnected;
    } catch (error) {
      console.error("Server connection error:", error);
      this.showLoadingError(this.t("SERVER_CONNECTION_ERROR"));
      return false;
    }
  },

  async loadServerStatus() {
    if (!URLS.content.serverStatus) {
      console.log("[Classic+] Server status endpoint not configured, skipping");
      return;
    }

    try {
      // The v100 ServerList endpoint returns XML, not JSON.
      // Parse with DOMParser to extract server availability.
      const response = await fetch(URLS.content.serverStatus);
      if (!response.ok) return;

      const xmlText = await response.text();
      const doc = new DOMParser().parseFromString(xmlText, "application/xml");

      // Check for XML parse errors
      const parseError = doc.querySelector("parsererror");
      if (parseError) {
        console.error("[Classic+] Server list XML parse error", parseError.textContent);
        return;
      }

      // Accept <server> elements anywhere in the document
      const servers = doc.querySelectorAll("server");
      if (servers.length === 0) return;

      const first = servers[0];

      // The v100 API uses child elements, not attributes.
      // <server_stat> is a hex bitmask where 0 = offline, non-zero = some access status.
      // <open> contains colored HTML text (green = low population = server is accessible).
      const serverStatEl = first.querySelector("server_stat");
      const serverStatVal = serverStatEl
        ? parseInt(serverStatEl.textContent.trim(), 16)
        : 0;

      // Server is considered online if server_stat is non-zero (server has an access mode set).
      const isOnline = !isNaN(serverStatVal) && serverStatVal > 0;

      const statusEl =
        document.getElementById("game-status") ||
        document.querySelector(".game-status");
      if (statusEl) {
        statusEl.textContent = isOnline ? "Online" : "Offline";
      }
    } catch (e) {
      console.error("Failed to load server status", e);
    }
  },

  async loadPatchNotes() {
    // Classic+ TODO: Re-enable when patch notes endpoint is available
    if (!URLS.content.patchNotes) {
      console.log("[Classic+] Patch notes endpoint not configured, skipping");
      return;
    }

    try {
      const notes = await fetchData(URLS.content.patchNotes);
      if (notes && notes.notes && Array.isArray(notes.notes)) {
        const container = document.getElementById("patch-notes");
        if (container) {
          container.innerHTML = notes.notes.map((n) => `<p>${escapeHtml(n)}</p>`).join("");
        }
      }
    } catch (e) {
      console.error("Failed to load patch notes", e);
    }
  },

  async checkLauncherUpdate() {
    // Classic+ TODO: Re-enable when launcher update endpoint is available
    if (!URLS.launcher.versionCheck) {
      return;
    }

    try {
      const response = await fetch(URLS.launcher.versionCheck);
      if (!response.ok) {
        return; // Do nothing when the version URL is unreachable
      }
      const data = await response.json();

      if (
        window.__TAURI__ &&
        window.__TAURI__.app &&
        window.__TAURI__.app.getVersion
      ) {
        const current = await window.__TAURI__.app.getVersion();

        const isNewerVersion =
          data.version && compareVersions(data.version, current) > 0;
        const isNewerDate =
          data.release_date &&
          new Date(data.release_date) > new Date(CURRENT_RELEASE_DATE);

        if (isNewerVersion && isNewerDate) {
          let userConfirm = false;
          if (typeof ask === "function") {
            userConfirm = await ask(
              "A new launcher version is available. Do you want to update now?",
              { title: "Launcher Update" },
            );
          } else {
            userConfirm = window.confirm(
              "A new launcher version is available. Do you want to update now?",
            );
          }
          if (userConfirm) {
            await invoke("update_launcher", {
              downloadUrl: URLS.launcher.download,
            });
          }
        }
      }
    } catch (e) {
      console.error("Launcher update check failed", e);
    }
  },

  /**
   * Formats a given number of bytes into a human-readable size string.
   *
   * @param {number} bytes the number of bytes to format
   * @returns {string} the formatted size string
   * @memberof App
   */
  formatSize(bytes) {
    if (bytes === undefined || bytes === null || isNaN(bytes)) return "0 B";
    const units = ["B", "KB", "MB", "GB", "TB"];
    let size = parseFloat(bytes);
    let unitIndex = 0;
    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024;
      unitIndex++;
    }
    return `${size.toFixed(2)} ${units[unitIndex]}`;
  },

  /**
   * Formats a given number of bytes per second into a human-readable speed string.
   *
   * @param {number} bytesPerSecond the number of bytes per second to format
   * @returns {string} the formatted speed string
   * @memberof App
   */
  formatSpeed(bytesPerSecond) {
    if (!isFinite(bytesPerSecond) || bytesPerSecond < 0) return "0 B/s";
    const units = ["B/s", "KB/s", "MB/s", "GB/s"];
    let speed = bytesPerSecond;
    let unitIndex = 0;
    while (speed >= 1024 && unitIndex < units.length - 1) {
      speed /= 1024;
      unitIndex++;
    }
    return `${speed.toFixed(2)} ${units[unitIndex]}`;
  },

  /**
   * Calculates the estimated time remaining for a download based on the total number of bytes downloaded so far, the total size of the download, and the current download speed.
   *
   * @param {number} totalDownloadedBytes the total number of bytes already downloaded
   * @param {number} totalSize the total size of the download in bytes
   * @param {number} speed the current download speed in bytes per second
   * @returns {number} the estimated time remaining in seconds, or 0 if the input is invalid. The result is capped at 30 days maximum.
   * @memberof App
   */
  calculateGlobalTimeRemaining(totalDownloadedBytes, totalSize, speed) {
    if (
      !isFinite(speed) ||
      speed <= 0 ||
      !isFinite(totalDownloadedBytes) ||
      !isFinite(totalSize) ||
      totalDownloadedBytes >= totalSize
    ) {
      return 0;
    }
    const bytesRemaining = totalSize - totalDownloadedBytes;
    const averageSpeed = this.calculateAverageSpeed(speed);
    if (averageSpeed <= 0) return 0;
    const secondsRemaining = bytesRemaining / averageSpeed;
    return Math.min(secondsRemaining, 30 * 24 * 60 * 60);
  },

  // Updated calculateAverageSpeed method
  calculateAverageSpeed(currentSpeed) {
    this.state.speedHistory.push(currentSpeed);

    if (this.state.speedHistory.length > this.state.speedHistoryMaxLength) {
      this.state.speedHistory.shift();
    }

    const sum = this.state.speedHistory.reduce((acc, speed) => acc + speed, 0);
    return sum / this.state.speedHistory.length;
  },

  /**
   * Format a time in seconds to a human-readable string.
   * If the input is invalid, returns 'Calculating...'
   * @param {number} seconds the time in seconds
   * @returns {string} a human-readable string representation of the time
   * @memberof App
   */
  formatTime(seconds) {
    if (!isFinite(seconds) || seconds < 0) return "Calculating...";

    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    const remainingSeconds = Math.floor(seconds % 60);

    if (hours > 0) {
      return `${hours}h ${minutes}m ${remainingSeconds}s`;
    } else if (minutes > 0) {
      return `${minutes}m ${remainingSeconds}s`;
    } else {
      return `${remainingSeconds}s`;
    }
  },

  /**
   * Returns the file name from a given path, or an empty string if the path is invalid.
   * @param {string} path the path to get the file name from
   * @returns {string} the file name
   * @memberof App
   */
  getFileName(path) {
    return path ? path.split("\\").pop().split("/").pop() : "";
  },

  /**
   * Shows an error message in the #error-container element for 5 seconds.
   * If the element does not exist, does nothing.
   * @param {string} message the error message to display
   * @memberof App
   */
  showErrorMessage(message) {
    const errorContainer = document.getElementById("error-container");
    if (errorContainer) {
      errorContainer.textContent = message;
      errorContainer.style.display = "block";
      setTimeout(() => {
        errorContainer.style.display = "none";
      }, 5000);
    }
  },

  /**
   * Shows a persistent error message that doesn't auto-hide.
   * Used for critical errors like download failures where user action is needed.
   * @param {string} message the error message to display
   * @memberof App
   */
  showPersistentError(message) {
    const errorContainer = document.getElementById("error-container");
    if (errorContainer) {
      errorContainer.textContent = message;
      errorContainer.style.display = "block";
      // Don't auto-hide - user needs to see this
    }
  },

  /**
   * Hides the error container.
   * @memberof App
   */
  hideError() {
    const errorContainer = document.getElementById("error-container");
    if (errorContainer) {
      errorContainer.style.display = "none";
    }
  },

  // Updated methods for loading modal
  showLoadingModal(message) {
    this.toggleModal("loading-modal", true, message);

    // Specific handling for loading modal elements
    if (this.loadingError) {
      this.loadingError.textContent = "";
      this.loadingError.style.display = "none";
    }
    if (this.refreshButton) {
      this.refreshButton.style.display = "none";
    }
    if (this.quitTheApp) {
      this.quitTheApp.style.display = "none";
    }
  },

  /**
   * Hides the loading modal.
   * @memberof App
   */
  hideLoadingModal() {
    this.toggleModal("loading-modal", false);
  },

  /**
   * Toggles the display of a modal.
   * @param {string} modalId The id of the modal to toggle.
   * @param {boolean} show Whether to show or hide the modal.
   * @param {string} [message] An optional message to display in the modal.
   * @memberof App
   */
  toggleModal(modalId, show, message = "") {
    const modal = document.getElementById(modalId);
    if (!modal) return;

    modal.classList.toggle("show", show);
    modal.style.display = show ? "block" : "none";

    if (modalId === "loading-modal" && message) {
      const messageElement = modal.querySelector(".loading-message");
      if (messageElement) messageElement.textContent = message;
    }
  },

  /**
   * Toggles the display of the hash file progress modal.
   * @param {boolean} show Whether to show or hide the modal.
   * @param {string} [message] An optional message to display in the modal.
   * @param {boolean} [isComplete=false] Whether the hash file generation is complete.
   * If true, shows a success message and closes the modal after 5 seconds.
   * @memberof App
   */
  toggleHashProgressModal(show, message = "", isComplete = false) {
    const modal = document.getElementById("hash-file-progress-modal");
    if (!modal) return;

    if (show) {
      modal.classList.add("show", "hash-modal-fade-in");
      modal.style.display = "block";

      // Handle message for hash file progress modal
      const messageElement = modal.querySelector("#hash-file-progress-text");
      if (messageElement && message) {
        messageElement.textContent = message;
      }

      if (isComplete) {
        // Show success message
        const successMessage = this.t("HASH_FILE_GENERATION_COMPLETE");
        const successElement = document.createElement("div");
        successElement.id = "hash-success-message";
        successElement.textContent = successMessage;

        const modalContent =
          modal.querySelector(".hash-progress-modal") || modal;
        modalContent.appendChild(successElement);

        // Wait 5 seconds, then close the modal
        setTimeout(() => {
          this.toggleHashProgressModal(false);
        }, 5000);
      }
    } else {
      modal.classList.remove("show", "hash-modal-fade-in");

      // Use a fade-out animation
      anime({
        targets: modal,
        opacity: 0,
        duration: 500,
        easing: "easeOutQuad",
        complete: () => {
          modal.style.display = "none";
          modal.style.opacity = 1; // Reset opacity for next time

          // Remove success message if it exists
          const successElement = modal.querySelector("#hash-success-message");
          if (successElement) {
            successElement.remove();
          }
        },
      });
    }
  },

  //method to display the loading indicator
  showLoadingIndicator() {
    let loadingIndicator = document.getElementById("loading-indicator");
    if (!loadingIndicator) {
      loadingIndicator = document.createElement("div");
      loadingIndicator.id = "loading-indicator";
      loadingIndicator.innerHTML = '<div class="spinner"></div>';
      document.body.appendChild(loadingIndicator);
    }
    loadingIndicator.style.display = "flex";
  },

  //method to hide the loading indicator
  hideLoadingIndicator() {
    const loadingIndicator = document.getElementById("loading-indicator");
    if (loadingIndicator) {
      loadingIndicator.style.display = "none";
    }
  },

  /**
   * Shows the loading error message on the loading modal.
   * @param {string} errorMessage The error message to be displayed.
   */
  showLoadingError(errorMessage) {
    const loadingModal = document.getElementById("loading-modal");
    if (loadingModal) {
      const errorElement = loadingModal.querySelector(".loading-error");
      if (errorElement) {
        errorElement.textContent = errorMessage;
        errorElement.style.display = "block";
      }

      const refreshButton = loadingModal.querySelector("#refresh-button");
      if (refreshButton) {
        refreshButton.style.display = "inline-block";
      }

      const quitButton = loadingModal.querySelector("#quit-button");
      if (quitButton) {
        quitButton.style.display = "inline-block";
      }
    }
  },

  /**
   * Shows a notification at the top of the page.
   * @param {string} message The message to be displayed in the notification.
   * @param {string} type The type of the notification, which will be used to determine the
   * colour of the notification. Possible values are 'success' and 'error'.
   */
  showNotification(message, type) {
    const notification = document.getElementById("notification");
    if (notification) {
      notification.textContent = message;
      notification.className = `notification ${type}`;

      // Show the notification
      gsap.fromTo(
        notification,
        { opacity: 0, y: -20 },
        {
          duration: 0.5,
          opacity: 1,
          y: 0,
          display: "block",
          ease: "power2.out",
        },
      );

      // Hide the notification after 5 seconds
      gsap.to(notification, {
        delay: 5,
        duration: 0.5,
        opacity: 0,
        y: -20,
        display: "none",
        ease: "power2.in",
      });
    }
  },

  /**
   * Loads the translations from a JSON file named `translations.json` at the root of the
   * project. If any error occurs, it logs the error to the console and sets the
   * `translations` property to an empty object.
   *
   * @returns {Promise<void>}
   */
  async loadTranslations() {
    try {
      const response = await fetch("translations.json");
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }
      this.translations = await response.json();
    } catch (error) {
      console.error("Error loading translations:", error);
      this.translations = { [this.currentLanguage]: {} };
    }
  },

  /**
   * Returns a translated string from the current language's translations.
   *
   * @param {string} key The key to translate.
   * @param {...*} args The arguments to replace in the translated string.
   * @returns {string} The translated string.
   */
  t(key, ...args) {
    const translations = this.translations[this.currentLanguage] || {};
    let str = translations[key] || key;
    return str.replace(/\{(\d+)\}/g, (_, index) => args[index] || "");
  },

  /**
   * Updates the language selector with the current language from the config file.
   * If any error occurs, it logs the error to the console and sets the
   * `currentLanguage` property to `'EUR'`.
   *
   * @returns {Promise<void>}
   */
  async updateLanguageSelector() {
    try {
      this.currentLanguage = await invoke("get_language_from_config");

      // Update legacy select wrapper if present
      const selectWrapper = document.querySelector(".select-wrapper");
      const selectStyled = selectWrapper?.querySelector(".select-styled");
      const selectOptions = selectWrapper?.querySelector(".select-options");
      const originalSelect = selectWrapper?.querySelector("select");

      if (selectWrapper && selectStyled && selectOptions && originalSelect) {
        this.setupLanguageOptions(selectOptions, originalSelect);
        this.setupLanguageEventListeners(selectStyled, selectOptions);

        const currentLanguageName =
          this.languages[this.currentLanguage] || this.currentLanguage;
        selectStyled.textContent = currentLanguageName;
        originalSelect.value = this.currentLanguage;
      }

      // Update the new shadcn dropdown in index.html
      this.syncShadcnLanguageDropdown();

      await this.loadTranslations();
      await this.updateAllTranslations();
    } catch (error) {
      console.error("Error updating language selector:", error);
      this.currentLanguage = "EUR";
      await this.loadTranslations();
      await this.updateAllTranslations();
    }
  },

  /**
   * Syncs the shadcn-style language dropdown in index.html with the current language.
   */
  syncShadcnLanguageDropdown() {
    const languageSelector = document.getElementById("language-selector");
    const regionCurrent = document.getElementById("region-current");
    const regionFlag = document.getElementById("region-flag");
    const regionOptions = document.querySelectorAll(".region-option");

    if (languageSelector) {
      languageSelector.value = this.currentLanguage;
    }

    // Update the visual dropdown state
    regionOptions.forEach((opt) => {
      if (opt.dataset.lang === this.currentLanguage) {
        opt.classList.add("active");
        if (regionCurrent) {
          regionCurrent.textContent = opt.textContent.trim();
        }
        // Clone the SVG flag
        const flagSvg = opt.querySelector(".flag-icon");
        if (regionFlag && flagSvg) {
          regionFlag.innerHTML = "";
          regionFlag.appendChild(flagSvg.cloneNode(true));
        }
      } else {
        opt.classList.remove("active");
      }
    });
  },

  /**
   * Sets up the language selector options based on the `this.languages` object.
   *
   * @param {HTMLElement} selectOptions - The `<ul>` element containing the language options.
   * @param {HTMLSelectElement} originalSelect - The `<select>` element containing the language options.
   * @returns {void}
   */
  setupLanguageOptions(selectOptions, originalSelect) {
    selectOptions.innerHTML = "";
    originalSelect.innerHTML = "";

    for (const [code, name] of Object.entries(this.languages)) {
      const option = document.createElement("option");
      option.value = code;
      option.textContent = name;
      originalSelect.appendChild(option);

      const li = document.createElement("li");
      li.setAttribute("rel", code);
      li.textContent = name;
      selectOptions.appendChild(li);
    }
  },

  /**
   * Sets up event listeners on the language selector options to change the language
   * when an option is clicked.
   *
   * @param {HTMLElement} selectStyled - The styled `<div>` element containing the selected language.
   * @param {HTMLElement} selectOptions - The `<ul>` element containing the language options.
   * @returns {void}
   */
  setupLanguageEventListeners(selectStyled, selectOptions) {
    selectOptions.querySelectorAll("li").forEach((li) => {
      li.addEventListener("click", async (e) => {
        const newLang = e.target.getAttribute("rel");
        if (newLang !== this.currentLanguage) {
          await this.changeLanguage(newLang);
          selectStyled.textContent = e.target.textContent;
        }
      });
    });
  },

  /**
   * Updates all elements with a `data-translate` attribute by setting their text
   * content to the translated value of the attribute's value. Also updates all
   * elements with a `data-translate-placeholder` attribute by setting their
   * `placeholder` attribute to the translated value of the attribute's value.
   *
   * This should be called after the language has been changed.
   *
   * @returns {Promise<void>}
   */
  async updateAllTranslations() {
    document.querySelectorAll("[data-translate]").forEach((el) => {
      const key = el.getAttribute("data-translate");
      el.textContent = this.t(key);
    });

    document.querySelectorAll("[data-translate-placeholder]").forEach((el) => {
      const key = el.getAttribute("data-translate-placeholder");
      el.placeholder = this.t(key);
    });

    // Tooltip (title attribute) localization — used by the Mods toolbar
    // button so screen readers and hover tooltips follow the selected
    // language.
    document.querySelectorAll("[data-translate-title]").forEach((el) => {
      const key = el.getAttribute("data-translate-title");
      el.title = this.t(key);
    });

    this.updateDynamicTranslations();
  },

  /**
   * Initializes the login page by adding an event listener to the login button.
   * When the button is clicked, the `login` function is called with the values
   * of the `username` and `password` input fields.
   *
   * @returns {void}
   */
  initLogin() {
    const loginButton = document.getElementById("login-button");
    const registerButton = document.getElementById("register-button");

    if (loginButton) {
      loginButton.addEventListener("click", async () => {
        const username = document.getElementById("username").value;
        const password = document.getElementById("password").value;
        await this.login(username, password);
      });
    }

    if (registerButton) {
      registerButton.addEventListener("click", () => {
        this.openRegisterPopup();
      });
    }
  },

  /**
   * Initializes the register page by wiring up button handlers.
   */
  initRegister() {
    const submitBtn = document.getElementById("register-submit-button");
    const backBtn = document.getElementById("register-back-button");

    if (submitBtn) {
      submitBtn.addEventListener("click", async () => {
        const username = document.getElementById("reg-username").value;
        const email = document.getElementById("reg-email").value;
        const password = document.getElementById("reg-password").value;
        await this.register(username, email, password);
      });
    }

    if (backBtn) {
      backBtn.addEventListener("click", () => {
        this.Router.navigate("home");
      });
    }
  },

  /**
   * Initializes the home page by creating a swiper slider and setting up the
   * home page elements and event listeners.
   *
   * @returns {Promise<void>}
   */
  async initHome() {
    // Initialize status UI immediately based on auth state
    // This overrides the HTML defaults before any async operations
    this.initializeStatusUI();

    // Initialize background carousel for home page
    initBackgroundCarousel();

    const sliderContainer = document.querySelector(".slider-container");

    const swiper = new Swiper(".news-slider", {
      effect: "fade",
      fadeEffect: {
        crossFade: true,
      },
      speed: 1500,
      loop: true,
      autoplay: {
        delay: 5000,
        disableOnInteraction: false,
      },
      pagination: {
        el: ".swiper-pagination",
        clickable: true,
      },
      navigation: {
        nextEl: ".swiper-button-next",
        prevEl: ".swiper-button-prev",
      },
      on: {
        slideChangeTransitionStart: function () {
          sliderContainer.classList.add("pulse");
        },
        slideChangeTransitionEnd: function () {
          sliderContainer.classList.remove("pulse");
        },
      },
    });

    this.setupHomePageElements();
    this.setupHomePageEventListeners();
    await this.initializeHomePageComponents();

    // Re-check authentication to update UI state after home page is loaded
    this.checkAuthentication();

    // One-shot scan: if any installed mods have a newer catalog version,
    // drop a dismissible banner at bottom-right pointing users at the
    // mod manager. Runs after a short delay so the home page paint
    // settles first.
    setTimeout(() => {
      if (this.checkModUpdatesOnLaunch) {
        this.checkModUpdatesOnLaunch().catch((e) =>
          console.warn("checkModUpdatesOnLaunch failed", e)
        );
      }
    }, 1500);
  },

  /**
   * Sets up the elements for the home page
   *
   * This is a one-time setup that should only be called once. It sets up the
   * elements that are used by the home page, such as the launch game button
   * and the game status element.
   *
   * @returns {void}
   */
  setupHomePageElements() {
    this.launchGameBtn = document.querySelector("#launch-game-btn");
    this.statusEl = document.querySelector("#status-string");
  },

  /**
   * Sets up the event listeners for the home page
   *
   * This method sets up the event listeners for the home page, such as the
   * launch game button, the logout button, the generate hash file button, and
   * the quit button.
   *
   * @returns {void}
   */
  setupHomePageEventListeners() {
    if (this.launchGameBtn) {
      this.launchGameBtn.addEventListener("click", () =>
        this.handleLaunchGame(),
      );
    }

    // Safety-net trigger: if the user has never seen the Mods onboarding,
    // show it once the authenticated home page is in view. The primary
    // trigger is still the Launch click (handleLaunchGame), but rendering
    // the dialog here as well guarantees it never gets missed due to timing
    // or a stuck launch gate. The localStorage flag prevents double-showing.
    if (this.state?.isAuthenticated && this.maybeShowModsOnboarding) {
      try { this.maybeShowModsOnboarding(); } catch (_) { /* non-fatal */ }
    }

    const repairButton = document.getElementById("check-game-files");
    if (repairButton) {
      repairButton.addEventListener("click", async (e) => {
        e.preventDefault();
        await invoke("clear_cache");
        await this.checkForUpdates();
      });
    }

    const logoutButton = document.getElementById("logout-link");
    if (logoutButton) {
      logoutButton.addEventListener("click", async (e) => {
        e.preventDefault();
        // Close settings dropdown
        const settingsWrapper = document.getElementById("settings-dropdown-wrapper");
        if (settingsWrapper) settingsWrapper.classList.remove("active");
        await this.logout();
      });
    }

    const generateHashFileBtn = document.getElementById("generate-hash-file");
    if (generateHashFileBtn && this.checkPrivilegeLevel()) {
      generateHashFileBtn.style.display = "block";
      generateHashFileBtn.addEventListener("click", () =>
        this.generateHashFile(),
      );
    }

    const appQuitButton = document.getElementById("app-quit");
    if (appQuitButton) {
      appQuitButton.addEventListener("click", () => this.appQuit());
    }

    const pauseButton = document.getElementById("btn-pause-resume");
    if (pauseButton) {
      pauseButton.addEventListener("click", () => {
        this.togglePauseResume();
      });
    }

    const themeBtn = document.getElementById("toggle-theme");
    if (themeBtn) {
      themeBtn.addEventListener("click", (e) => {
        e.preventDefault();
        this.toggleTheme();
      });
    }

  },

  /**
   * Initializes the home page components
   *
   * This method initializes the components on the home page, such as the game
   * path, the user panel, the modal settings, and the game status. It also
   * updates the UI based on the user's privileges and the game status.
   *
   * @returns {Promise<void>}
   */
  async initializeHomePageComponents() {
    this.checkFirstLaunch();
    if (!this.state.isFirstLaunch) {
      await this.loadGamePath();
    }
    this.initUserPanel();
    this.initModalSettings();
    await this.updateGameStatus();
    await this.loadServerStatus();
    await this.loadPatchNotes();
    this.updateUIBasedOnPrivileges();
    this.updateUI();
    const isGameRunning = await this.isGameRunning();
    this.updateUIForGameStatus(isGameRunning);
  },

  initUserPanel() {
    const btnUserAvatar = document.querySelector(".btn-user-avatar");
    const dropdownPanelWrapper = document.querySelector(
      ".dropdown-panel-wrapper",
    );
    if (!btnUserAvatar || !dropdownPanelWrapper) return;

    // Initialize panel state
    let isPanelOpen = false;

    // Set up initial animation
    gsap.set(dropdownPanelWrapper, {
      display: "none",
      opacity: 0,
      y: -10,
    });

    // Create a reusable GSAP timeline
    const tl = gsap.timeline({ paused: true });
    tl.to(dropdownPanelWrapper, {
      duration: 0.3,
      display: "block",
      opacity: 1,
      y: 0,
      ease: "power2.out",
    });

    // Event handler for the button
    btnUserAvatar.addEventListener("click", (event) => {
      event.stopPropagation();
      if (!isPanelOpen) {
        tl.play();
      } else {
        tl.reverse().then(() => {
          gsap.set(dropdownPanelWrapper, { display: "none" });
        });
      }
      isPanelOpen = !isPanelOpen;
    });

    // Close panel when clicking outside
    document.addEventListener("click", () => {
      if (isPanelOpen) {
        tl.reverse().then(() => {
          gsap.set(dropdownPanelWrapper, { display: "none" });
        });
        isPanelOpen = false;
      }
    });

    dropdownPanelWrapper.addEventListener("click", (event) => {
      event.stopPropagation();
      if (event.target.tagName === "A" && event.target.target === "_blank") {
        event.preventDefault();
        window.__TAURI__.shell.open(event.target.href);
      }
    });
  },

  /**
   * Initializes the modal settings by finding the required elements in the DOM and
   * setting up event listeners for the button, close span, and input field.
   * @returns {void}
   */
  initModalSettings() {
    const modal = document.getElementById("modal");
    const btn = document.getElementById("openModal");
    const span = document.getElementsByClassName("close")[0];
    const input = document.getElementById("gameFolder");
    const versionInfo = document.getElementById("version-info");
    const tabButtons = modal ? modal.querySelectorAll(".menu-tab") : [];

    if (!modal || !btn || !span || !input) return;

    this.setupModalEventListeners(
      modal,
      btn,
      span,
      input,
      versionInfo,
      tabButtons,
    );
  },

  /**
   * Sets up event listeners for the modal settings.
   * @param {HTMLElement} modal The modal element.
   * @param {HTMLElement} btn The button element that opens the modal.
   * @param {HTMLElement} span The close span element that closes the modal.
   * @param {HTMLElement} input The input field element for the game folder.
   * @returns {void}
   */
  setupModalEventListeners(modal, btn, span, input, versionInfo, tabButtons) {
    /**
     * Handles the click event for the game folder input field.
     *
     * Opens the file dialog to select a game folder, and if a folder is selected,
     * saves the path to the configuration file and shows a success notification.
     * If an error occurs, shows an error notification.
     * @returns {Promise<void>}
     */
    input.onclick = async () => {
      try {
        const selectedPath = await invoke("select_game_folder");
        if (selectedPath) {
          input.value = selectedPath;
          await this.saveGamePath(selectedPath);
          if (typeof window.showUpdateNotification === 'function') {
            window.showUpdateNotification('success', this.t("FOLDER_SAVED_SUCCESS"), selectedPath);
          }
          this.closeModal(modal);
        }
      } catch (error) {
        console.error("Error selecting game folder:", error);
        if (typeof window.showUpdateNotification === 'function') {
          window.showUpdateNotification('error', this.t("FOLDER_SELECTION_ERROR"), error.message || '');
        }
      }
    };

    /**
     * Handles the click event for the button that opens the modal.
     *
     * Animates the modal to open with a fade-in effect.
     * @returns {void}
     */
    btn.onclick = () => {
      if (tabButtons && tabButtons.length) {
        tabButtons[0].click();
      }
      gsap.to(modal, {
        duration: 0.5,
        display: "flex",
        opacity: 1,
        ease: "power2.inOut",
      });
    };

    span.onclick = () => this.closeModal(modal);

    /**
     * Handles the change event for the game folder input field.
     *
     * Checks if the new value contains the string "tera" (case-insensitive),
     * and shows a success notification if it does, or an error notification if it does not.
     * @returns {void}
     */
    input.onchange = () => {
      if (input.value.toLowerCase().includes("tera")) {
        this.showNotification(this.t("FOLDER_FOUND_SUCCESS"), "success");
      } else {
        this.showNotification(this.t("FOLDER_NOT_FOUND"), "error");
      }
    };

    window.addEventListener("click", (event) => {
      if (event.target === modal) {
        this.closeModal(modal);
      }
    });

    if (versionInfo && URLS.launcher.versionInfo) {
      fetch(URLS.launcher.versionInfo)
        .then((r) => r.json())
        .then((data) => {
          versionInfo.innerHTML = `
                        <strong>${escapeHtml(data.launcher_name)}</strong><br>
                        Version: ${escapeHtml(data.version)}<br>
                        Release: ${escapeHtml(data.release_date)}<br>
                        <a href="${escapeHtml(data.website)}" target="_blank">Website</a>
                    `;
        })
        .catch(() => {
          if (
            window.__TAURI__ &&
            window.__TAURI__.app &&
            window.__TAURI__.app.getVersion
          ) {
            window.__TAURI__.app.getVersion().then((v) => {
              versionInfo.textContent = v;
            });
          }
        });
    }

    if (tabButtons) {
      tabButtons.forEach((tab) => {
        tab.onclick = () => {
          const section = tab.dataset.section;
          modal.querySelectorAll(".settings-section").forEach((sec) => {
            sec.classList.remove("active");
            sec.style.display = "none";
          });
          modal
            .querySelectorAll(".menu-tab")
            .forEach((btn) => btn.classList.remove("active"));
          const target = document.getElementById(`settings-${section}`);
          if (target) {
            target.classList.add("active");
            target.style.display = "block";
          }
          tab.classList.add("active");
        };
      });
    }
  },

  /**
   * Closes the given modal element with a fade-out effect.
   *
   * Animates the modal to fade out with a duration of 0.5 seconds,
   * and once the animation is complete, sets the display property of the modal to "none".
   * @param {HTMLElement} modal The modal element to close.
   * @returns {void}
   */
  closeModal(modal) {
    gsap.to(modal, {
      duration: 0.5,
      opacity: 0,
      ease: "power2.inOut",
      /**
       * Sets the display property of the modal to "none" once the animation is complete.
       * This is necessary because the opacity animation does not affect the display property.
       * @this {GSAP}
       */
      onComplete: () => {
        modal.style.display = "none";
      },
    });
  },

  /**
   * Initializes the loading modal elements.
   *
   * Gets the loading modal, loading message, loading error, refresh button, and quit button elements
   * from the DOM. If any of these elements are not found, logs an error.
   * @memberof App
   * @returns {void}
   */
  initializeLoadingModalElements() {
    this.loadingModal = document.getElementById("loading-modal");
    if (this.loadingModal) {
      this.loadingMessage = this.loadingModal.querySelector(".loading-message");
      this.loadingError = this.loadingModal.querySelector(".loading-error");
      this.refreshButton = this.loadingModal.querySelector("#refresh-button");
      this.quitTheApp = this.loadingModal.querySelector("#quit-button");
    }
  },

  /**
   * Sets up event listeners for the refresh and quit buttons in the loading modal.
   *
   * If the refresh button is found, adds a click event listener that checks if the user
   * is connected to the internet and authenticated. If both conditions are true, calls
   * initializeAndCheckUpdates. If the quit button is found, adds a click event listener
   * that calls appQuit.
   * @memberof App
   * @returns {void}
   */
  setupModalButtonEventHandlers() {
    if (this.refreshButton) {
      this.refreshButton.addEventListener("click", async () => {
        const isConnected = await this.checkServerConnection();
        if (isConnected && this.state.isAuthenticated) {
          await this.initializeAndCheckUpdates();
        }
      });
    }
    if (this.quitTheApp) {
      this.quitTheApp.addEventListener("click", () => this.appQuit());
    }
  },

  /**
   * Saves the game path to the config file and handles the result based on first launch state.
   * @param {string} path - The path to the game executable.
   * @returns {Promise<void>}
   */
  async saveGamePath(path) {
    try {
      await invoke("save_game_path_to_config", { path });
      if (this.state.isFirstLaunch) {
        this.completeFirstLaunch();
        if (typeof window.showUpdateNotification === 'function') {
          window.showUpdateNotification('success', this.t("GAME_PATH_SET_FIRST_LAUNCH"), path);
        }
      } else {
        if (typeof window.showUpdateNotification === 'function') {
          window.showUpdateNotification('success', this.t("GAME_PATH_UPDATED"), path);
        }

        // Cancel any ongoing downloads/checks before starting new check
        try {
          await invoke("cancel_downloads");
        } catch (e) {
          console.warn("Failed to cancel ongoing downloads:", e);
        }

        // Clear any pending download timeout
        if (this.pendingDownloadTimeout) {
          clearTimeout(this.pendingDownloadTimeout);
          this.pendingDownloadTimeout = null;
        }

        this.setState(getPathChangeResetState());
        this.updateLaunchGameButton(true);
        this.toggleLanguageSelector(false);

        // Show checking state and start file check for new folder
        if (typeof window.showCheckingState === 'function') {
          window.showCheckingState(0, 0);
        }

        const isConnected = await this.checkServerConnection();
        if (isConnected) {
          this.checkForUpdates(); // Don't await - let it run in background
        } else {
          // Server not connected - clear checking state
          if (typeof window.hideCheckingState === 'function') {
            window.hideCheckingState();
          }
          this.updateLaunchGameButton(false);
        }
      }
    } catch (error) {
      console.error("Error saving game path:", error);
      if (typeof window.showUpdateNotification === 'function') {
        window.showUpdateNotification('error', this.t("GAME_PATH_SAVE_ERROR"), error.message || '');
      }
      // Clear checking state on error
      if (typeof window.hideCheckingState === 'function') {
        window.hideCheckingState();
      }
      throw error;
    }
  },

  /**
   * Loads the game path from the config file and sets the input field value.
   * If an error occurs, it displays the error in a Windows system message and
   * offers the user the option to quit the app.
   */
  async loadGamePath() {
    try {
      const path = await invoke("get_game_path_from_config");
      const input = document.getElementById("gameFolder");
      if (input) {
        input.value = path;
      }
    } catch (error) {
      console.error("Error loading game path:", error);
      // Display the error in a Windows system message
      let errorMessage;
      if (
        error &&
        error.message &&
        typeof error.message === "string" &&
        error.message.toLowerCase().includes("src/tera_config.ini")
      ) {
        errorMessage = this.t("CONFIG_INI_MISSING");
      } else {
        errorMessage = `${this.t("GAME_PATH_LOAD_ERROR")} ${error || ""}`;
      }

      const userResponse = await message(errorMessage, {
        title: this.t("ERROR"),
        type: "error",
      });

      if (userResponse) {
        this.appQuit();
      }
    }
  },

  /**
   * Generic config loader for the new UI.
   * @param {string} key - The config key to load ('gamePath' supported)
   * @returns {Promise<string|null>}
   */
  async loadConfig(key) {
    try {
      if (key === 'gamePath') {
        return await invoke("get_game_path_from_config");
      }
      return null;
    } catch (error) {
      console.error(`Error loading config ${key}:`, error);
      return null;
    }
  },

  /**
   * Generic config saver for the new UI.
   * @param {string} key - The config key to save ('gamePath' supported)
   * @param {string} value - The value to save
   * @returns {Promise<void>}
   */
  async saveConfig(key, value) {
    try {
      if (key === 'gamePath') {
        await this.saveGamePath(value);
      }
    } catch (error) {
      console.error(`Error saving config ${key}:`, error);
      throw error;
    }
  },

  /**
   * Force revalidates all game files and updates if necessary.
   * Triggered from the Settings menu.
   * Status is shown in the bottom status area, no popup notification needed.
   */
  async revalidateAndUpdateGame() {
    if (this.state.isCheckingForUpdates || this.state.isDownloading) {
      console.log("Already checking or downloading, skipping");
      return;
    }

    // Just run the file check - status area handles the UI
    await this.checkForUpdates();
  },

  /**
   * Checks for launcher updates and shows notification.
   * Uses Tauri's built-in updater.
   */
  async checkForLauncherUpdates() {
    if (typeof window.showUpdateNotification === 'function') {
      window.showUpdateNotification('Checking for launcher updates...', false);
    }

    try {
      // Tauri updater handles this via dialog:true in tauri.conf.json
      // This is just for manual trigger from menu
      const { checkUpdate } = window.__TAURI__.updater;
      if (checkUpdate) {
        const { shouldUpdate, manifest } = await checkUpdate();
        if (shouldUpdate) {
          if (typeof window.showUpdateNotification === 'function') {
            window.showUpdateNotification(`Update available: ${manifest?.version}`, true);
          }
        } else {
          if (typeof window.showUpdateNotification === 'function') {
            window.showUpdateNotification('Launcher is up to date', true);
          }
        }
      } else {
        if (typeof window.showUpdateNotification === 'function') {
          window.showUpdateNotification('Launcher is up to date', true);
        }
      }
    } catch (error) {
      console.error("Error checking for launcher updates:", error);
      if (typeof window.showUpdateNotification === 'function') {
        window.showUpdateNotification('Update check failed', true);
      }
    }
  },

  /**
   * Sets up the event listeners for the window controls (minimize and close buttons)
   * to allow the user to interact with the window.
   */
  setupWindowControls() {
    const appMinimizeBtn = document.getElementById("app-minimize");
    if (appMinimizeBtn) {
      appMinimizeBtn.addEventListener("click", () => appWindow.minimize());
    }

    const appCloseBtn = document.getElementById("app-close");
    if (appCloseBtn) {
      appCloseBtn.addEventListener("click", () => this.appQuit());
    }

    // Set up window dragging for areas with data-tauri-drag-region or -webkit-app-region: drag
    this.setupWindowDragging();

    // Set up language selector listener for the new shadcn dropdown
    this.setupLanguageSelectorListener();
  },

  /**
   * Sets up window dragging functionality. In Tauri with decorations disabled,
   * we need to call appWindow.startDragging() on mousedown for drag regions.
   */
  setupWindowDragging() {
    // Handle mousedown on elements with data-tauri-drag-region attribute
    document.addEventListener("mousedown", async (e) => {
      // Check if the clicked element or any parent has the drag region attribute
      const target = e.target;

      // Skip if clicking on interactive elements
      if (this.isInteractiveElement(target)) {
        return;
      }

      // Check for data-tauri-drag-region attribute
      const hasDragRegion = target.closest("[data-tauri-drag-region]");
      if (hasDragRegion) {
        await appWindow.startDragging();
        return;
      }

      // Check for CSS -webkit-app-region: drag (computed style check)
      const computedStyle = window.getComputedStyle(target);
      // Note: -webkit-app-region is not exposed via getComputedStyle in all browsers
      // So we check if the element matches our known draggable selectors
      const draggableSelectors = [
        "#home-page",
        "#home-bg-container",
        "#home-main",
        "#home-info-section",
        ".info-content",
        "#home-news-bar",
        "#home-footer",
        ".header"
      ];

      for (const selector of draggableSelectors) {
        if (target.matches(selector) || target.closest(selector)) {
          // Make sure we're not clicking on an interactive child
          const interactiveChild = target.closest("button, a, input, select, textarea, #home-launch-section, .promo-card, #home-player-badge, .news-item, .news-label, .nav-btn, .control-btn, .region-btn, .login-input, .login-btn, .register-btn, .menu-item, .region-option, .account-manager, .account-dropdown-item, .account-btn, .account-add-btn, .account-register-btn");
          if (!interactiveChild) {
            await appWindow.startDragging();
            return;
          }
        }
      }
    });
  },

  /**
   * Checks if an element is interactive (button, link, input, etc.)
   * Also checks if the element is INSIDE an interactive element (e.g., SVG inside button)
   */
  isInteractiveElement(element) {
    const interactiveTags = ["BUTTON", "A", "INPUT", "SELECT", "TEXTAREA"];
    if (interactiveTags.includes(element.tagName)) {
      return true;
    }

    // Check if element is inside an interactive element (e.g., SVG inside button)
    if (element.closest("button, a, input, select, textarea")) {
      return true;
    }

    // Check for interactive class names on element itself
    const interactiveClasses = [
      "nav-btn", "control-btn", "region-btn", "login-btn", "register-btn",
      "menu-item", "region-option", "promo-card", "news-item", "news-label",
      "account-manager", "account-dropdown-item", "account-btn", "account-add-btn",
      "account-register-btn", "account-delete-btn"
    ];
    for (const cls of interactiveClasses) {
      if (element.classList.contains(cls)) {
        return true;
      }
    }

    // Check if element is inside an element with interactive class
    for (const cls of interactiveClasses) {
      if (element.closest(`.${cls}`)) {
        return true;
      }
    }

    return false;
  },

  /**
   * Sets up the language selector listener for the new shadcn-style dropdown.
   * The dropdown in index.html dispatches change events on #language-selector.
   */
  setupLanguageSelectorListener() {
    const languageSelector = document.getElementById("language-selector");
    if (languageSelector) {
      languageSelector.addEventListener("change", async (e) => {
        const newLang = e.target.value;
        if (newLang && newLang !== this.currentLanguage) {
          await this.changeLanguage(newLang);
        }
      });
    }
  },

  /**
   * Sets up the custom animations for the select element (dropdown menu) to give
   * it a nicer appearance. If the select element is not found, it does nothing.
   */
  setupCustomAnimations() {
    const selectWrapper = document.querySelector(".select-wrapper");
    if (selectWrapper) {
      const selectStyled = selectWrapper.querySelector(".select-styled");
      const selectOptions = selectWrapper.querySelector(".select-options");
      const originalSelect = selectWrapper.querySelector("select");

      if (selectStyled && selectOptions && originalSelect) {
        this.setupSelectAnimation(selectStyled, selectOptions, originalSelect);
      }
    }
  },

  /**
   * Sets up the custom animations for the select element (dropdown menu) to give
   * it a nicer appearance. If the select element is not found, it does nothing.
   * @param {HTMLElement} selectStyled The styled select element.
   * @param {HTMLElement} selectOptions The select options element.
   * @param {HTMLElement} originalSelect The original select element.
   */
  setupSelectAnimation(selectStyled, selectOptions, originalSelect) {
    selectStyled.addEventListener("click", (e) => {
      e.stopPropagation();
      selectStyled.classList.toggle("active");
      this.animateSelectOptions(selectOptions);
    });

    selectOptions.querySelectorAll("li").forEach((option) => {
      option.addEventListener("click", (e) => {
        e.stopPropagation();
        this.handleSelectOptionClick(
          e.target,
          selectStyled,
          selectOptions,
          originalSelect,
        );
      });
    });

    document.addEventListener("click", () => {
      selectStyled.classList.remove("active");
      this.animateSelectOptions(selectOptions, true);
    });
  },

  /**
   * Animates the display of the select options element to give it a nicer
   * appearance. If the second argument is true, the element is hidden.
   * @param {HTMLElement} selectOptions The select options element.
   * @param {boolean} [hide=false] Whether to hide or show the element.
   */
  animateSelectOptions(selectOptions, hide = false) {
    anime({
      targets: selectOptions,
      opacity: hide ? [1, 0] : [0, 1],
      translateY: hide ? [0, -10] : [-10, 0],
      duration: 300,
      easing: "easeOutQuad",
      begin: (anim) => {
        if (!hide) selectOptions.style.display = "block";
      },
      complete: (anim) => {
        if (hide) selectOptions.style.display = "none";
      },
    });
  },

  /**
   * Handles a click on a select option by updating the displayed text on the
   * styled select element and hiding the options. Also animates the select
   * element to give it a nicer appearance.
   * @param {HTMLElement} target The option that was clicked.
   * @param {HTMLElement} selectStyled The styled select element.
   * @param {HTMLElement} selectOptions The select options element.
   * @param {HTMLSelectElement} originalSelect The original select element.
   */
  handleSelectOptionClick(target, selectStyled, selectOptions, originalSelect) {
    selectStyled.textContent = target.textContent;
    originalSelect.value = target.getAttribute("rel");
    selectStyled.classList.remove("active");
    this.animateSelectOptions(selectOptions, true);
    anime({
      targets: selectStyled,
      scale: [1, 1.05, 1],
      duration: 300,
      easing: "easeInOutQuad",
    });
  },

  /**
   * Sets up a mutation observer to detect changes to the 'dl-status-string'
   * element, which is used to display the download status of the game. When a
   * mutation is detected, the UI is updated to ensure that the displayed
   * information is correct.
   */
  setupMutationObserver() {
    // Cleanup existing observer first
    this.cleanupMutationObserver();

    const targetNode = document.getElementById("dl-status-string");
    if (targetNode) {
      const config = { childList: true, subtree: true };
      const callback = (mutationsList) => {
        for (let mutation of mutationsList) {
          if (mutation.type === "childList") this.updateUI();
        }
      };
      this.observer = new MutationObserver(callback);
      this.observer.observe(targetNode, config);
    }
  },

  /**
   * Disconnects the MutationObserver to prevent memory leaks.
   */
  cleanupMutationObserver() {
    if (this.observer) {
      this.observer.disconnect();
      this.observer = null;
    }
  },

  /**
   * Updates the visibility of the "Generate Hash File" button based on the current
   * privilege level. If the user has the required privilege level, the button is
   * displayed; otherwise, it is hidden.
   */
  updateUIBasedOnPrivileges() {
    const generateHashFileBtn = document.getElementById("generate-hash-file");
    if (generateHashFileBtn) {
      generateHashFileBtn.style.display = this.checkPrivilegeLevel()
        ? "block"
        : "none";
    }
  },

  /**
   * Checks if the user is authenticated by checking for the presence of a stored
   * authentication key in local storage. If the key is present, the user is
   * considered authenticated, otherwise they are not.
   */
  checkAuthentication() {
    const isAuthenticated = localStorage.getItem("authKey") !== null;
    const userName = localStorage.getItem("userName");
    this.setState({ isAuthenticated });

    // Update index.html header (login form, user display) - always available
    if (typeof window.updateIndexHeaderAuthState === "function") {
      window.updateIndexHeaderAuthState(isAuthenticated, userName);
    }

    // Update home.html UI (launch button, status) - only when home page is loaded
    if (typeof window.updateHeaderAuthState === "function") {
      window.updateHeaderAuthState(isAuthenticated, userName);
    }
  },

  /**
   * Initializes the status UI based on authentication state.
   * Called at app startup to set the correct initial state before any async operations.
   */
  initializeStatusUI() {
    if (typeof window.initializeStatusUI === "function") {
      window.initializeStatusUI();
    }
  },

  /**
   * Checks if the user has the required privilege level by checking if the
   * 'privilege' key in local storage is a valid integer and greater than or
   * equal to the value of REQUIRED_PRIVILEGE_LEVEL.
   * @returns {boolean} True if the user has the required privilege level, false
   * otherwise.
   */
  checkPrivilegeLevel() {
    const userPrivilege = parseInt(localStorage.getItem("privilege"), 10);
    return !isNaN(userPrivilege) && userPrivilege >= REQUIRED_PRIVILEGE_LEVEL;
  },

  /**
   * Sends the stored authentication key, user name, user number, and character count
   * to the backend to set the auth info.
   * @returns {Promise<void>}
   */
  async sendStoredAuthInfoToBackend() {
    const authKey = localStorage.getItem("authKey");
    const userName = localStorage.getItem("userName");
    const userNo = parseInt(localStorage.getItem("userNo"), 10);
    const characterCount = localStorage.getItem("characterCount");

    if (authKey && userName && userNo && characterCount) {
      await invoke("set_auth_info", {
        authKey,
        userName,
        userNo,
        characterCount,
      });
    }
  },

  /**
   * Generates a hash file for the game files. If the operation is already in
   * progress, it will not start a new operation. It will disable the 'Generate
   * Hash File' button until the operation is complete. It will also show a
   * modal with a progress bar and show a notification when the operation is
   * complete or has failed.
   * @returns {Promise<void>}
   */
  async generateHashFile() {
    if (this.state.isGeneratingHashFile) return;

    let unlistenProgress = null;
    try {
      this.setState({
        isGeneratingHashFile: true,
        hashFileProgress: 0,
        currentProcessingFile: "",
        processedFiles: 0,
        totalFiles: 0,
      });

      const generateHashBtn = document.getElementById("generate-hash-file");
      if (generateHashBtn) {
        generateHashBtn.disabled = true;
      }

      this.toggleHashProgressModal(
        true,
        this.t("INITIALIZING_HASH_GENERATION"),
      );

      unlistenProgress = await listen("hash_file_progress", (event) => {
        const { current_file, progress, processed_files, total_files } =
          event.payload;

        this.setState({
          hashFileProgress: progress,
          currentProcessingFile: current_file,
          processedFiles: processed_files,
          totalFiles: total_files,
        });

        this.updateHashFileProgressUI();
      });

      await invoke("generate_hash_file");
      this.toggleHashProgressModal(true, "", true);
      this.showNotification(this.t("HASH_FILE_GENERATED"), "success");
    } catch (error) {
      console.error("Error generating hash file:", error);
      this.showNotification(this.t("HASH_FILE_GENERATION_ERROR"), "error");
    } finally {
      this.setState({
        isGeneratingHashFile: false,
        hashFileProgress: 0,
        currentProcessingFile: "",
        processedFiles: 0,
        totalFiles: 0,
      });

      const generateHashBtn = document.getElementById("generate-hash-file");
      if (generateHashBtn) {
        generateHashBtn.disabled = false;
      }

      if (unlistenProgress) {
        unlistenProgress();
      }
    }
  },

  /**
   * Disable the context menu and text selection in the app window.
   *
   * This is needed to prevent users from selecting and copying text from the app window.
   * It's also needed to prevent users from accessing the context menu and doing things like
   * saving the page as a file, etc.
   */
  disableContextMenu() {
    document.addEventListener("contextmenu", (e) => {
      e.preventDefault();
    });

    document.addEventListener("selectstart", (e) => {
      e.preventDefault();
    });
  },

  /**
   * Close the app window.
   *
   * This function is called when the app needs to be closed, such as when the user
   * clicks the "Exit" button in the app menu.
   */
  appQuit() {
    appWindow.close();
  },

  async stopDownloads() {
    try {
      await invoke("cancel_downloads");
    } catch (error) {
      console.error("Failed to stop downloads:", error);
    }
  },

  async togglePauseResume() {
    // Prevent double execution
    if (this.state.isTogglingPauseResume) {
      return;
    }

    try {
      this.setState({ isTogglingPauseResume: true });

      // Cancel any pending download timeout when pausing
      if (this.pendingDownloadTimeout) {
        clearTimeout(this.pendingDownloadTimeout);
        this.pendingDownloadTimeout = null;
      }

      if (this.state.currentUpdateMode === "download" || this.state.currentUpdateMode === "complete") {
        try {
          this.setState({ isPauseRequested: true, currentUpdateMode: "paused" });
          await invoke("cancel_downloads");
        } catch (e) {
          console.error("pause failed", e);
          this.setState({ isPauseRequested: false });
        }
        return;
      }
      if (this.state.currentUpdateMode === "paused") {
        if (this.state.updateError) {
          this.showErrorMessage(this.t("UPDATE_ERROR_MESSAGE"));
          return;
        }
        try {
          const previousTotal = this.state.totalSize || 0;
          const previousDownloaded = this.state.downloadedSize || 0;
          this.setState({
            currentUpdateMode: "file_check",
            isCheckingForUpdates: true,
          });
          const filesToUpdate = await invoke("get_files_to_update");
          this.setState({ isCheckingForUpdates: false });

          if (filesToUpdate && filesToUpdate.length > 0) {
            const { newTotalSize, clampedDownloaded } = calculateResumeSnapshot(
              previousTotal,
              previousDownloaded,
              filesToUpdate,
            );

            this.setState({
              currentUpdateMode: "download",
              isUpdateAvailable: true,
              isFileCheckComplete: true,
              downloadStartTime: Date.now(),
              lastProgressUpdate: null,
              speedHistory: [],
              totalFiles: filesToUpdate.length,
              totalSize: newTotalSize,
              downloadedBytesOffset: clampedDownloaded,
              downloadedSize: clampedDownloaded,
              isPauseRequested: false,
            });
            // Reset toggle guard BEFORE starting the long-running download
            // so user can pause again during download
            this.setState({ isTogglingPauseResume: false });
            await this.runPatchSystem(filesToUpdate);
          } else {
            this.handleCompletion();
          }
        } catch (e) {
          console.error("resume failed", e);
          this.setState({
            currentUpdateMode: "paused",
            isCheckingForUpdates: false,
          });
        }
      }
    } finally {
      // Ensure toggle guard is always reset (handles early returns and errors)
      if (this.state.isTogglingPauseResume) {
        this.setState({ isTogglingPauseResume: false });
      }
    }
  },

  /**
   * Handles route changes.
   *
   * This function is called when a route change is detected. It simply calls
   * the Router's navigate method to handle the route change.
   */
  handleRouteChange() {
    this.Router.navigate();
  },

  /**
   * Loads the content of the specified file asynchronously.
   *
   * @param {string} file - The file to load the content of.
   *
   * @returns {Promise<string>} The loaded content as a string.
   */
  async loadAsyncContent(file) {
    const response = await fetch(file);
    if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);
    return await response.text();
  },

  /**
   * Smoothly transitions between two pages.
   *
   * This function handles the process of smoothly transitioning between two
   * pages. It does this by animating the opacity and translateX properties of
   * the two pages. The new page is first appended to the app element, and then
   * the current page is removed once the animation is finished.
   *
   * @param {HTMLElement} app - The app element.
   * @param {HTMLElement} newPage - The new page element.
   */
  async smoothPageTransition(app, newPage) {
    const currentPage = app.querySelector(".page");

    newPage.style.position = "absolute";
    newPage.style.top = "0";
    newPage.style.left = "0";
    newPage.style.width = "100%";
    newPage.style.opacity = "0";
    newPage.style.transform = "translateX(20px)";

    app.appendChild(newPage);

    if (currentPage) {
      await anime({
        targets: currentPage,
        opacity: [1, 0],
        translateX: [0, -20],
        easing: "easeInOutQuad",
        duration: 300,
      }).finished;

      currentPage.remove();
    }

    await anime({
      targets: newPage,
      opacity: [0, 1],
      translateX: [20, 0],
      easing: "easeOutQuad",
      duration: 300,
    }).finished;

    newPage.style.position = "";
    newPage.style.top = "";
    newPage.style.left = "";
    newPage.style.width = "";
    newPage.style.transform = "";
  },

  // ========== ACCOUNT MANAGER UI ==========

  /**
   * Initialize account manager UI and event listeners.
   */
  initAccountManager() {
    const accountBtn = document.getElementById('account-btn');
    const accountManager = document.getElementById('account-manager');
    const accountAddBtn = document.getElementById('account-add-btn');
    const accountDropdownList = document.getElementById('account-dropdown-list');

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
    if (accountAddBtn) {
      accountAddBtn.addEventListener('click', () => {
        accountManager.classList.remove('open');
        this.openAddAccountModal();
      });
    }

    // Event delegation for dropdown list - handles both account switch and delete
    if (accountDropdownList) {
      accountDropdownList.addEventListener('click', async (e) => {
        // Check if delete button was clicked
        const deleteBtn = e.target.closest('.account-delete-btn');
        if (deleteBtn) {
          e.stopPropagation();
          const userNo = deleteBtn.dataset.userNo;
          const account = AccountManager.getAccount(userNo);
          if (account) {
            App.openDeleteAccountModal(account);
          }
          return;
        }

        // Check if account item was clicked (for switching)
        const accountItem = e.target.closest('.account-dropdown-item');
        if (accountItem) {
          e.stopPropagation();
          const userNo = accountItem.dataset.userNo;
          console.log('Switching to account:', userNo);
          try {
            await App.switchAccount(userNo);
          } catch (err) {
            console.error('Error switching account:', err);
          }
          accountManager.classList.remove('open');
        }
      });
    }

    // Setup modals
    this.setupAddAccountModal();
    this.setupDeleteAccountModal();
    this.setupConsentModals();

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
      // Empty state - just leave blank, the "Add Account" button below is self-explanatory
      list.innerHTML = '';
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
    // Event handlers are managed via event delegation in initAccountManager
  },

  /**
   * Update the account button display with current account info.
   */
  updateAccountDisplay() {
    const btnName = document.getElementById('account-btn-name');
    const statusDot = document.getElementById('account-status-dot');
    const accountOptionsMenu = document.getElementById('menu-account-options');

    if (!btnName) return;

    const activeAccount = AccountManager.getActiveAccount();

    if (activeAccount) {
      btnName.textContent = activeAccount.userName;
      const isPlaying = AccountManager.isAccountInGame(activeAccount.userNo);
      statusDot.classList.toggle('playing', isPlaying);
      // Show Account Options menu item when an account is selected
      if (accountOptionsMenu) {
        accountOptionsMenu.style.display = 'flex';
      }
    } else {
      btnName.textContent = 'Not logged in';
      statusDot.classList.remove('playing');
      // Hide Account Options menu item when no account is selected
      if (accountOptionsMenu) {
        accountOptionsMenu.style.display = 'none';
      }
    }
  },

  /**
   * Switch to a different account.
   */
  async switchAccount(userNo) {
    console.log('switchAccount called with userNo:', userNo);
    const account = AccountManager.getAccount(userNo);
    if (!account) {
      console.error('Account not found for userNo:', userNo);
      return;
    }
    console.log('Found account:', account.userName);

    AccountManager.setActiveAccountId(userNo);

    // OAuth accounts — use stored auth info (no credential-based refresh)
    if (account.authMethod === 'oauth') {
      console.log('OAuth account — using stored auth info for:', account.userName);
      this.setState({ isAuthenticated: true });
      this.updateAccountDisplay();
      this.updateLaunchButtonState();
      return;
    }

    // Password accounts — do silent auth refresh
    try {
      const cred = JSON.parse(atob(account.credentials));
      console.log('Attempting silent auth refresh for:', cred.u);
      const success = await this.silentAuthRefresh(cred.u, cred.p);
      console.log('Silent auth refresh result:', success);
      if (!success) {
        this.openAddAccountModal(account.userName);
        window.showUpdateNotification('error', this.t('LOGIN_FAILED') || 'Login Failed', this.t('PLEASE_REENTER_PASSWORD') || 'Please re-enter your password');
        return;
      }
    } catch (e) {
      console.error('Failed to switch account:', e);
      window.showUpdateNotification('error', this.t('ERROR') || 'Error', 'Failed to switch account');
      return;
    }

    console.log('Updating account display and launch button state');
    this.updateAccountDisplay();
    this.updateLaunchButtonState();
  },

  /**
   * Update launch button and status based on whether active account has running game.
   */
  updateLaunchButtonState() {
    const launchBtn = document.getElementById('launch-game-btn');
    if (!launchBtn) return;

    const activeAccount = AccountManager.getActiveAccount();
    if (!activeAccount) {
      launchBtn.disabled = true;
      launchBtn.classList.add('disabled');
      return;
    }

    const isPlaying = AccountManager.isAccountInGame(activeAccount.userNo);
    if (isPlaying) {
      // Show "In Game" in the status text, keep button disabled
      if (this.statusEl) {
        this.statusEl.textContent = this.t('IN_GAME') || 'In Game';
      }
      launchBtn.disabled = true;
      launchBtn.classList.add('disabled');
      // Update only the text span, NOT the entire button (preserve SVG icons)
      const btnText = document.getElementById('launch-btn-text');
      if (btnText) {
        btnText.textContent = this.t('LAUNCH_GAME') || 'LAUNCH';
      }
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
    const passwordInput = document.getElementById('add-account-password');

    if (!modal) return;

    cancelBtn.addEventListener('click', () => this.closeAddAccountModal());

    modal.addEventListener('click', (e) => {
      if (e.target === modal) this.closeAddAccountModal();
    });

    submitBtn.addEventListener('click', () => this.handleAddAccount());

    // Enter key to submit
    if (passwordInput) {
      passwordInput.addEventListener('keypress', (e) => {
        if (e.key === 'Enter') this.handleAddAccount();
      });
    }
  },

  openAddAccountModal(prefillUsername = '') {
    const modal = document.getElementById('add-account-modal');
    const usernameInput = document.getElementById('add-account-username');
    const passwordInput = document.getElementById('add-account-password');
    const errorEl = document.getElementById('add-account-error');

    if (!modal) return;

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
    if (modal) modal.classList.remove('show');
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
      const jsonResponse = JSON.parse(response);

      if (jsonResponse && jsonResponse.Return && jsonResponse.Msg === 'success') {
        const userNo = Number(jsonResponse.Return.UserNo).toString();
        const authKey = jsonResponse.Return.AuthKey;
        const characterCount = jsonResponse.Return.CharacterCount;

        // Check if account already exists
        const existingAccounts = AccountManager.getAccounts();
        const exists = existingAccounts.some(a => a.userNo === userNo);

        const credentials = btoa(JSON.stringify({ u: username, p: password }));

        if (exists) {
          // Update credentials for existing account
          AccountManager.updateAccountCredentials(userNo, credentials);
        } else {
          // Add new account
          AccountManager.addAccount({
            userNo: userNo,
            userName: username,
            credentials: credentials
          });
        }

        // Set as active and update backend state
        AccountManager.setActiveAccountId(userNo);
        await invoke('set_auth_info', {
          authKey: authKey,
          userName: username,
          userNo: Number(jsonResponse.Return.UserNo),
          characterCount: characterCount
        });

        this.setState({ isAuthenticated: true });
        this.updateAccountDisplay();
        this.updateLaunchButtonState();
        this.closeAddAccountModal();

        window.showUpdateNotification('success', this.t('ACCOUNT_ADDED') || 'Account Added', username);
      } else {
        const errorMessage = jsonResponse ? jsonResponse.Msg || 'Login failed' : 'Login failed';
        errorEl.textContent = errorMessage;
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

    if (!modal || !account) return;

    message.textContent = `Remove "${account.userName}" from the launcher?`;
    modal.classList.add('show');
  },

  closeDeleteAccountModal() {
    const modal = document.getElementById('delete-account-modal');
    if (modal) modal.classList.remove('show');
    this._accountToDelete = null;
  },

  confirmDeleteAccount() {
    if (!this._accountToDelete) return;

    const deletedName = this._accountToDelete.userName;
    AccountManager.removeAccount(this._accountToDelete.userNo);
    this.closeDeleteAccountModal();

    // If we deleted the active account, switch to another or clear auth
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
    window.showUpdateNotification('info', this.t('ACCOUNT_REMOVED') || 'Account Removed', deletedName);
  },

  // ========== LEADERBOARD CONSENT MODAL ==========

  setupConsentModals() {
    this.setupLeaderboardConsentModal();
    this.setupAccountOptionsModal();
  },

  setupLeaderboardConsentModal() {
    const modal = document.getElementById('leaderboard-consent-modal');
    const checkbox = document.getElementById('consent-read-checkbox');
    const agreeBtn = document.getElementById('consent-agree-btn');
    const disagreeBtn = document.getElementById('consent-disagree-btn');

    if (!modal) return;

    // Enable/disable buttons based on checkbox
    checkbox.addEventListener('change', () => {
      const checked = checkbox.checked;
      agreeBtn.disabled = !checked;
      disagreeBtn.disabled = !checked;
    });

    // Agree button
    agreeBtn.addEventListener('click', async () => {
      // Disable buttons immediately to prevent double-clicks
      agreeBtn.disabled = true;
      disagreeBtn.disabled = true;
      await this.setLeaderboardConsent(true);
      this.closeLeaderboardConsentModal();
      // Continue launching the game
      this._proceedWithLaunch = true;
      this.handleLaunchGame();
    });

    // Disagree button
    disagreeBtn.addEventListener('click', async () => {
      // Disable buttons immediately to prevent double-clicks
      agreeBtn.disabled = true;
      disagreeBtn.disabled = true;
      await this.setLeaderboardConsent(false);
      this.closeLeaderboardConsentModal();
      // Continue launching the game
      this._proceedWithLaunch = true;
      this.handleLaunchGame();
    });

    // Close when clicking outside
    modal.addEventListener('click', (e) => {
      if (e.target === modal) {
        this.closeLeaderboardConsentModal();
      }
    });
  },

  openLeaderboardConsentModal() {
    const modal = document.getElementById('leaderboard-consent-modal');
    const checkbox = document.getElementById('consent-read-checkbox');
    const agreeBtn = document.getElementById('consent-agree-btn');
    const disagreeBtn = document.getElementById('consent-disagree-btn');

    if (!modal) return;

    // Reset state
    checkbox.checked = false;
    agreeBtn.disabled = true;
    disagreeBtn.disabled = true;

    // Disable launch button while consent modal is open to prevent race conditions
    if (this.launchGameBtn) {
      this.launchGameBtn.disabled = true;
      this.launchGameBtn.classList.add('disabled');
    }

    modal.classList.add('show');
  },

  closeLeaderboardConsentModal() {
    const modal = document.getElementById('leaderboard-consent-modal');
    if (modal) modal.classList.remove('show');

    // Re-enable launch button if not proceeding with launch
    // (if proceeding, handleLaunchGame will manage the button state)
    if (!this._proceedWithLaunch && this.launchGameBtn) {
      this.launchGameBtn.disabled = false;
      this.launchGameBtn.classList.remove('disabled');
    }
  },

  setupAccountOptionsModal() {
    const modal = document.getElementById('account-options-modal');
    const closeBtn = document.getElementById('account-options-close');
    const leaderboardToggle = document.getElementById('leaderboard-toggle');
    const accountOptionsMenu = document.getElementById('menu-account-options');

    if (!modal) return;

    // Open modal when clicking Account Options menu item
    if (accountOptionsMenu) {
      accountOptionsMenu.addEventListener('click', async (e) => {
        e.preventDefault();
        // Close settings dropdown
        const settingsWrapper = document.getElementById("settings-dropdown-wrapper");
        if (settingsWrapper) settingsWrapper.classList.remove("active");
        await this.openAccountOptionsModal();
      });
    }

    // Close modal
    closeBtn.addEventListener('click', () => {
      this.closeAccountOptionsModal();
    });

    // Toggle change handler
    leaderboardToggle.addEventListener('change', async (e) => {
      const agreed = e.target.checked;
      leaderboardToggle.disabled = true;
      const success = await this.setLeaderboardConsent(agreed);
      leaderboardToggle.disabled = false;
      if (!success) {
        // Revert on failure
        e.target.checked = !agreed;
        window.showUpdateNotification('error', this.t('ERROR') || 'Error', 'Failed to update preference');
      }
    });

    // Close when clicking outside
    modal.addEventListener('click', (e) => {
      if (e.target === modal) {
        this.closeAccountOptionsModal();
      }
    });
  },

  async openAccountOptionsModal() {
    const modal = document.getElementById('account-options-modal');
    const leaderboardToggle = document.getElementById('leaderboard-toggle');

    if (!modal) return;

    modal.classList.add('show');

    // Fetch current consent status (will silently re-auth if needed)
    leaderboardToggle.disabled = true;
    const result = await this.getLeaderboardConsent();

    if (result.success) {
      // Successfully fetched - enable toggle (consent can be true/1 or false/0)
      leaderboardToggle.checked = result.consent === true || result.consent === 1;
      leaderboardToggle.disabled = false;
    } else {
      // Could not fetch consent - keep toggle disabled
      leaderboardToggle.checked = false;
      leaderboardToggle.disabled = true;
      console.warn('[Consent] Could not fetch consent status, toggle disabled');
    }
  },

  closeAccountOptionsModal() {
    const modal = document.getElementById('account-options-modal');
    if (modal) modal.classList.remove('show');
  },

  /**
   * Ensures an authenticated session exists for API calls.
   * If no session, silently re-authenticates using stored credentials.
   * If re-auth fails, prompts user to re-login.
   * @param {boolean} promptOnFailure - If true, open login modal on failure
   * @returns {Promise<boolean>} true if session is ready, false if failed
   */
  /**
   * Classic+ TODO: Re-enable when auth session API (has_auth_session) is available.
   * Returns true as a no-op so callers proceed without blocking.
   */
  async ensureAuthSession(promptOnFailure = false) {
    return true;
  },

  /**
   * Get leaderboard consent status from backend.
   * Classic+ TODO: Re-enable when leaderboard API is available
   * @returns {Promise<{success: boolean, consent: null}>}
   */
  async getLeaderboardConsent() {
    // Classic+ TODO: Re-enable when leaderboard consent endpoint is available
    return { success: false, consent: null };
  },

  /**
   * Set leaderboard consent on backend.
   * Classic+ TODO: Re-enable when leaderboard API is available
   * @param {boolean} agreed
   * @returns {Promise<boolean>}
   */
  async setLeaderboardConsent(agreed) {
    // Classic+ TODO: Re-enable when leaderboard consent endpoint is available
    return false;
  },

  /**
   * Check if we need to show the leaderboard consent modal.
   * Classic+ TODO: Re-enable when leaderboard API is available
   * @returns {Promise<boolean>} Always false on Classic+ (no consent modal)
   */
  async checkLeaderboardConsent() {
    // Classic+ TODO: Re-enable when leaderboard consent endpoint is available
    return false;

    // Original logic preserved below for reference:
    const result = await this.getLeaderboardConsent();
    // If fetch failed, don't block game launch
    if (!result.success) {
      console.warn('[Consent] Could not check consent, allowing game launch');
      return false;
    }
    // Show modal only if consent hasn't been set yet (null)
    return result.consent === null;
  },
};
function LoadStartPage() {
  // Load player count from API (initial load)
  loadPlayerCount();

  // Refresh player count every 60 seconds
  setInterval(loadPlayerCount, 60000);

  // Load news from RSS feed
  loadNewsFeed();

  // Classic+ TODO: Re-enable when news endpoint is available
  if (!URLS.content.news) {
    console.log("[Classic+] News endpoint not configured, skipping news data fetch");
    return;
  }

  // Load original news data for ads/links
  fetchData(URLS.content.news).then((jsonNews) => {
    if (!jsonNews) {
      console.warn("Failed to load news data");
      return;
    }
    //MAINTENANCE INFO
    if (jsonNews.WARTUNG_enabled) {
      const maintenanceContainer = document.getElementById("maintenance-container");
      if (maintenanceContainer) {
        maintenanceContainer.classList.add("show");
      }
      const wartungText = document.getElementById("NewsWartungTextId");
      if (wartungText) {
        wartungText.textContent = jsonNews.WARTUNG_info_text;
      }
    } else {
      const maintenanceContainer = document.getElementById("maintenance-container");
      if (maintenanceContainer) {
        maintenanceContainer.classList.remove("show");
      }
      const wartungText = document.getElementById("NewsWartungTextId");
      if (wartungText) {
        wartungText.textContent = "";
      }
    }

    //ADVERTISEMENT LEFT
    const adBg1 = document.getElementById("AdImgBg1");
    if (jsonNews.Advertisement_left_img_url && adBg1) {
      adBg1.style.backgroundImage = `url('${jsonNews.Advertisement_left_img_url}')`;
      document.getElementById("AdTextId1").textContent = jsonNews.Advertisement_left_text || "";
    }
    if (jsonNews.Advertisement_left_img_link_url) {
      document.getElementById("AdImgId1Href").href = localizeForumUrl(
        jsonNews.Advertisement_left_img_link_url,
        App.currentLanguage,
      );
    }

    //ADVERTISEMENT MIDDLE
    const adBg2 = document.getElementById("AdImgBg2");
    if (jsonNews.Advertisement_mid_img_url && adBg2) {
      adBg2.style.backgroundImage = `url('${jsonNews.Advertisement_mid_img_url}')`;
      document.getElementById("AdTextId2").textContent = jsonNews.Advertisement_mid_text || "";
    }
    if (jsonNews.Advertisement_mid_img_link_url) {
      document.getElementById("AdImgId2Href").href = localizeForumUrl(
        jsonNews.Advertisement_mid_img_link_url,
        App.currentLanguage,
      );
    }

    //ADVERTISEMENT RIGHT
    const adBg3 = document.getElementById("AdImgBg3");
    if (jsonNews.Advertisement_right_img_url && adBg3) {
      adBg3.style.backgroundImage = `url('${jsonNews.Advertisement_right_img_url}')`;
      document.getElementById("AdTextId3").textContent = jsonNews.Advertisement_right_text || "";
    }
    if (jsonNews.Advertisement_right_img_link_url) {
      document.getElementById("AdImgId3Href").href = localizeForumUrl(
        jsonNews.Advertisement_right_img_link_url,
        App.currentLanguage,
      );
    }
  }).catch((error) => {
    console.error("Error loading start page:", error);
  });
}

/**
 * Fetches and displays the current player count from the server API.
 * Updates the compact player badge with count and status.
 */
async function loadPlayerCount() {
  const playerCountEl = document.getElementById("player-count");
  const serverPulseEl = document.getElementById("server-pulse");

  if (!playerCountEl) return;

  try {
    // Use Tauri backend to avoid CORS issues
    const jsonString = await invoke("fetch_player_count");
    const data = JSON.parse(jsonString);

    if (data.latest) {
      // Animate the player count
      animateNumber(playerCountEl, data.latest.players);

      // Update server status indicator
      if (data.latest.maintenance) {
        if (serverPulseEl) serverPulseEl.classList.add("maintenance");
      } else {
        if (serverPulseEl) serverPulseEl.classList.remove("maintenance", "offline");
      }
    }
  } catch (error) {
    console.error("Error loading player count:", error);
    if (playerCountEl) playerCountEl.textContent = "--";
    if (serverPulseEl) serverPulseEl.classList.add("offline");
  }
}

/**
 * Animates a number from 0 to the target value.
 */
function animateNumber(element, target, duration = 1000) {
  const start = 0;
  const startTime = performance.now();

  function update(currentTime) {
    const elapsed = currentTime - startTime;
    const progress = Math.min(elapsed / duration, 1);

    // Ease out cubic
    const easeOut = 1 - Math.pow(1 - progress, 3);
    const current = Math.round(start + (target - start) * easeOut);

    element.textContent = current;

    if (progress < 1) {
      requestAnimationFrame(update);
    }
  }

  requestAnimationFrame(update);
}

/**
 * Fetches and displays news in the horizontal ticker.
 */
async function loadNewsFeed() {
  const newsFeedList = document.getElementById("news-feed-list");
  if (!newsFeedList) return;

  try {
    // Use Tauri backend to avoid CORS issues (returns pre-parsed JSON)
    const jsonString = await invoke("fetch_news_feed");
    const items = JSON.parse(jsonString);

    // Clear loading state
    newsFeedList.innerHTML = "";

    // Display news items as vertical list in sidebar
    const maxItems = Math.min(items.length, 5);
    for (let i = 0; i < maxItems; i++) {
      const item = items[i];
      const title = item.title || "Untitled";
      const link = item.link || "#";

      const newsItem = document.createElement("a");
      newsItem.href = localizeForumUrl(link, App.currentLanguage);
      newsItem.target = "_blank";
      // Inline styles for vertical news list
      // Horizontal news bar style
      newsItem.className = 'news-item';
      newsItem.textContent = escapeHtml(title);
      newsFeedList.appendChild(newsItem);
    }

    // If no items, show placeholder
    if (maxItems === 0) {
      newsFeedList.innerHTML = `<span class="news-item news-muted">No news available</span>`;
    }
  } catch (error) {
    console.error("Error loading news feed:", error);
    newsFeedList.innerHTML = `<span class="news-item news-muted">Unable to load news</span>`;
  }
}

// Create the Router and attach it to App
App.Router = createRouter(App);

// Expose App globally if necessary
window.App = App;

// Initialize the app when the DOM is fully loaded
window.addEventListener("DOMContentLoaded", () => App.init());
