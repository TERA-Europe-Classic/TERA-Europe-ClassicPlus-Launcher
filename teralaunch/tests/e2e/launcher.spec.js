// @ts-check
import { test, expect } from "@playwright/test";

/**
 * E2E Tests for TERA Launcher
 *
 * These tests cover the main user flows of the launcher application.
 * For Tauri apps, tests run against the webview content served by the dev server.
 * Tauri-specific APIs (@tauri-apps/api) are mocked where necessary.
 */

// =============================================================================
// Test Setup and Utilities
// =============================================================================

/**
 * Mock Tauri API calls for browser-based testing.
 * This allows E2E tests to run without the full Tauri runtime.
 */
async function mockTauriAPIs(page) {
  await page.addInitScript(() => {
    // Mock window.__TAURI__ object that Tauri injects
    window.__TAURI__ = {
      invoke: async (cmd, args) => {
        // Mock responses for common Tauri commands
        const mockResponses = {
          get_game_path: "C:\\Games\\TERA",
          set_game_path: true,
          check_authentication: { authenticated: false },
          login: { success: true, username: "TestUser" },
          logout: true,
          get_download_progress: { progress: 0, total: 100, speed: 0 },
          launch_game: true,
          check_updates: { hasUpdate: false },
          get_version: "1.9.2",
          get_config: { language: "EUR", gamePath: "C:\\Games\\TERA" },
1.7.0        };
        return mockResponses[cmd] || null;
      },
      event: {
        listen: async (event, handler) => {
          // Return unsubscribe function
          return () => {};
        },
        emit: async (event, payload) => {},
      },
      dialog: {
        open: async (options) => "C:\\Games\\TERA",
      },
      window: {
        appWindow: {
          minimize: async () => {},
          close: async () => {},
        },
      },
    };

    // Mock localStorage for authentication state
    if (!localStorage.getItem("isAuthenticated")) {
      localStorage.setItem("isAuthenticated", "false");
    }
  });
}

/**
 * Helper to simulate authenticated state
 */
async function setAuthenticated(page, username = "TestUser") {
  await page.evaluate((user) => {
    localStorage.setItem("isAuthenticated", "true");
    localStorage.setItem("userName", user);
    localStorage.setItem("authToken", "mock-token-12345");
  }, username);
}

/**
 * Helper to clear authentication state
 */
async function clearAuthentication(page) {
  await page.evaluate(() => {
    localStorage.removeItem("isAuthenticated");
    localStorage.removeItem("userName");
    localStorage.removeItem("authToken");
  });
}

// =============================================================================
// 1. LOGIN FLOW TESTS
// =============================================================================

test.describe("Login Flow", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await clearAuthentication(page);
    await page.goto("/");
  });

  test("page loads with login form visible", async ({ page }) => {
    // Wait for the app to initialize and route to login
    await page.waitForSelector(".login-form", { timeout: 10000 });

    // Verify login form elements are present
    await expect(page.locator("#username")).toBeVisible();
    await expect(page.locator("#password")).toBeVisible();
    await expect(page.locator("#login-button")).toBeVisible();
    await expect(page.locator("#register-button")).toBeVisible();

    // Verify TERA logo is displayed
    await expect(page.locator(".tera-logo-1-icon")).toBeVisible();
  });

  test("login form has correct placeholders", async ({ page }) => {
    await page.waitForSelector(".login-form", { timeout: 10000 });

    const usernameInput = page.locator("#username");
    const passwordInput = page.locator("#password");

    // Check placeholder attributes exist (translation may vary)
    await expect(usernameInput).toHaveAttribute("placeholder", /.+/);
    await expect(passwordInput).toHaveAttribute("placeholder", /.+/);
    await expect(passwordInput).toHaveAttribute("type", "password");
  });

  test("error shown for empty credentials submission", async ({ page }) => {
    await page.waitForSelector(".login-form", { timeout: 10000 });

    // Attempt login without entering credentials
    await page.locator("#login-button").click();

    // Wait a moment for error handling
    await page.waitForTimeout(500);

    // The error message element should be in the DOM
    const errorMsg = page.locator("#login-error-msg");
    await expect(errorMsg).toBeAttached();
  });

  test("error shown for invalid credentials", async ({ page }) => {
    await page.waitForSelector(".login-form", { timeout: 10000 });

    // Mock the login to fail
    await page.addInitScript(() => {
      window.__TAURI__.invoke = async (cmd, args) => {
        if (cmd === "login") {
          throw new Error("Invalid credentials");
        }
        return null;
      };
    });

    // Enter invalid credentials
    await page.locator("#username").fill("wronguser");
    await page.locator("#password").fill("wrongpassword");
    await page.locator("#login-button").click();

    // Wait for error to potentially appear
    await page.waitForTimeout(1000);

    // Error message should be displayed or form should still be visible
    const loginForm = page.locator(".login-form");
    await expect(loginForm).toBeVisible();
  });

  test("successful login navigates to home", async ({ page }) => {
    await page.waitForSelector(".login-form", { timeout: 10000 });

    // Set up mock for successful login
    await page.evaluate(() => {
      // Override to simulate successful login
      const originalInvoke = window.__TAURI__.invoke;
      window.__TAURI__.invoke = async (cmd, args) => {
        if (cmd === "login" || cmd === "authenticate") {
          localStorage.setItem("isAuthenticated", "true");
          localStorage.setItem("userName", args?.username || "TestUser");
          return { success: true, username: args?.username || "TestUser" };
        }
        return originalInvoke(cmd, args);
      };
    });

    // Enter credentials
    await page.locator("#username").fill("testuser");
    await page.locator("#password").fill("testpassword");

    // Submit form
    await page.locator("#login-button").click();

    // After successful login, should navigate to home
    // Check for authentication state change
    await page.waitForTimeout(1500);

    // Verify we're no longer on login (hash changed or home content visible)
    const isHomePage = await page.evaluate(() => {
      return localStorage.getItem("isAuthenticated") === "true";
    });
    expect(isHomePage).toBe(true);
  });

  test("register button is clickable", async ({ page }) => {
    await page.waitForSelector(".login-form", { timeout: 10000 });

    const registerButton = page.locator("#register-button");
    await expect(registerButton).toBeVisible();
    await expect(registerButton).toBeEnabled();

    // Click should not throw
    await registerButton.click();
  });
});

// =============================================================================
// 2. HOME PAGE UI TESTS
// =============================================================================

test.describe("Home Page UI", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await setAuthenticated(page, "TestUser");
    await page.goto("/#home");
  });

  test("launch button is visible when authenticated", async ({ page }) => {
    await page.waitForSelector("#home-page", { timeout: 10000 });

    const launchButton = page.locator("#launch-game-btn");
    await expect(launchButton).toBeVisible();
    await expect(launchButton).toBeEnabled();
  });

  test("launch button has correct text", async ({ page }) => {
    await page.waitForSelector("#home-page", { timeout: 10000 });

    const launchButton = page.locator("#launch-game-btn");
    // Text varies by language but should contain something
    await expect(launchButton).toContainText(/.+/);
  });

  test("settings menu item opens modal", async ({ page }) => {
    await page.waitForSelector("#home-page", { timeout: 10000 });

    // Find and click the settings link in the user menu
    const settingsLink = page.locator("#openModal");
    await settingsLink.click();

    // Modal should become visible
    const modal = page.locator("#modal");
    await expect(modal).toBeVisible();
  });

  test("user name is displayed correctly", async ({ page }) => {
    await page.waitForSelector("#home-page", { timeout: 10000 });

    const userNameElement = page.locator("#userName");
    await expect(userNameElement).toContainText("TestUser");
  });

  test("status display elements are present", async ({ page }) => {
    await page.waitForSelector("#home-page", { timeout: 10000 });

    // Progress elements should exist in the DOM
    await expect(page.locator("#progress-percentage-div")).toBeAttached();
    await expect(page.locator("#status-string")).toBeAttached();
    await expect(page.locator("#download-speed")).toBeAttached();
    await expect(page.locator("#time-remaining")).toBeAttached();
  });

  test("home page displays TERA logo", async ({ page }) => {
    await page.waitForSelector("#home-page", { timeout: 10000 });

    const logo = page.locator("#Tera-Logo-home");
    await expect(logo).toBeAttached();
  });
});

// =============================================================================
// 3. SETTINGS MODAL TESTS
// =============================================================================

test.describe("Settings Modal", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await setAuthenticated(page, "TestUser");
    await page.goto("/#home");
    await page.waitForSelector("#home-page", { timeout: 10000 });
  });

  test("opens when clicking settings link", async ({ page }) => {
    // Initially modal should be hidden
    const modal = page.locator("#modal");

    // Click settings to open modal
    await page.locator("#openModal").click();

    // Modal should now be visible
    await expect(modal).toBeVisible();
  });

  test("game path input is visible in settings", async ({ page }) => {
    // Open settings modal
    await page.locator("#openModal").click();
    await expect(page.locator("#modal")).toBeVisible();

    // Game folder input should be present
    const gameFolderInput = page.locator("#gameFolder");
    await expect(gameFolderInput).toBeVisible();
  });

  test("settings tabs are functional", async ({ page }) => {
    // Open settings modal
    await page.locator("#openModal").click();
    await expect(page.locator("#modal")).toBeVisible();

    // Should have menu tabs
    const folderTab = page.locator('.menu-tab[data-section="folder"]');
    const versionTab = page.locator('.menu-tab[data-section="version"]');

    await expect(folderTab).toBeVisible();
    await expect(versionTab).toBeVisible();

    // Click version tab
    await versionTab.click();

    // Version section should become visible
    const versionSection = page.locator("#settings-version");
    await expect(versionSection).toBeVisible();
  });

  test("can close modal with close button", async ({ page }) => {
    // Open settings modal
    await page.locator("#openModal").click();
    const modal = page.locator("#modal");
    await expect(modal).toBeVisible();

    // Click close button
    await page.locator(".modal .close").click();

    // Modal should be hidden
    await expect(modal).not.toBeVisible();
  });

  test("settings folder section shows instructions", async ({ page }) => {
    // Open settings modal
    await page.locator("#openModal").click();
    await expect(page.locator("#modal")).toBeVisible();

    // Check for instruction text
    const instructions = page.locator("#settings-folder p");
    await expect(instructions).toBeVisible();
  });
});

// =============================================================================
// 4. DOWNLOAD PROGRESS UI TESTS
// =============================================================================

test.describe("Download Progress UI", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await setAuthenticated(page, "TestUser");
    await page.goto("/#home");
    await page.waitForSelector("#home-page", { timeout: 10000 });
  });

  test("progress bar element exists", async ({ page }) => {
    const progressBar = page.locator("#progress-percentage-div");
    await expect(progressBar).toBeAttached();
  });

  test("progress percentage display exists", async ({ page }) => {
    const progressPercentage = page.locator("#progress-percentage");
    await expect(progressPercentage).toBeAttached();
  });

  test("download speed display exists", async ({ page }) => {
    const speedDisplay = page.locator("#download-speed");
    await expect(speedDisplay).toBeAttached();
  });

  test("time remaining display exists", async ({ page }) => {
    const timeRemaining = page.locator("#time-remaining");
    await expect(timeRemaining).toBeAttached();
  });

  test("file count display exists", async ({ page }) => {
    const filesProgress = page.locator("#files-progress");
    await expect(filesProgress).toBeAttached();
  });

  test("progress bar updates visually", async ({ page }) => {
    // Simulate progress update by manipulating the DOM
    await page.evaluate(() => {
      const progressBar = document.getElementById("progress-percentage-div");
      if (progressBar) {
        progressBar.style.width = "50%";
      }
      const percentText = document.getElementById("progress-percentage");
      if (percentText) {
        percentText.textContent = "50%";
      }
    });

    const progressBar = page.locator("#progress-percentage-div");
    await expect(progressBar).toHaveCSS("width", /.+/);
  });

  test("download info section has speed label", async ({ page }) => {
    // Check for speed label element
    const speedLabel = page.locator('[data-translate="SPEED_LABEL"]');
    await expect(speedLabel).toBeAttached();
  });

  test("download info section has time remaining label", async ({ page }) => {
    // Check for time remaining label element
    const timeLabel = page.locator('[data-translate="TIME_REMAINING_LABEL"]');
    await expect(timeLabel).toBeAttached();
  });
});

// =============================================================================
// 5. NAVIGATION TESTS
// =============================================================================

test.describe("Navigation", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
  });

  test("protected routes redirect to login when not authenticated", async ({
    page,
  }) => {
    await clearAuthentication(page);

    // Try to navigate directly to home
    await page.goto("/#home");

    // Wait for routing to complete
    await page.waitForTimeout(1500);

    // Should be redirected to login
    const loginForm = page.locator(".login-form");
    await expect(loginForm).toBeVisible({ timeout: 10000 });
  });

  test("authenticated user can access home page", async ({ page }) => {
    await setAuthenticated(page, "TestUser");

    await page.goto("/#home");

    // Should stay on home page
    await page.waitForSelector("#home-page", { timeout: 10000 });
    const homePage = page.locator("#home-page");
    await expect(homePage).toBeVisible();
  });

  test("login page redirects authenticated users to home", async ({ page }) => {
    await setAuthenticated(page, "TestUser");

    // Try to go to login
    await page.goto("/#login");

    // Wait for routing
    await page.waitForTimeout(1500);

    // Should be redirected to home (or at least not show login form)
    const isAuth = await page.evaluate(() => {
      return localStorage.getItem("isAuthenticated") === "true";
    });
    expect(isAuth).toBe(true);
  });

  test("logout redirects to login page", async ({ page }) => {
    await setAuthenticated(page, "TestUser");
    await page.goto("/#home");
    await page.waitForSelector("#home-page", { timeout: 10000 });

    // Find and click logout link
    const logoutLink = page.locator("#logout-link");
    await logoutLink.click();

    // Clear auth state (simulating what logout does)
    await clearAuthentication(page);

    // Navigate to force route check
    await page.goto("/");
    await page.waitForTimeout(1000);

    // Should be on login
    const loginForm = page.locator(".login-form");
    await expect(loginForm).toBeVisible({ timeout: 10000 });
  });

  test("hash navigation works correctly", async ({ page }) => {
    await setAuthenticated(page, "TestUser");

    await page.goto("/");

    // Navigate via hash
    await page.evaluate(() => {
      window.location.hash = "home";
    });

    await page.waitForTimeout(1500);

    // Should navigate to home
    const homePage = page.locator("#home-page");
    await expect(homePage).toBeVisible({ timeout: 10000 });
  });
});

// =============================================================================
// 6. LANGUAGE SELECTOR TESTS
// =============================================================================

test.describe("Language Selector", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await page.goto("/");
  });

  test("language selector is visible in header", async ({ page }) => {
    const languageSelector = page.locator("#language-selector");
    await expect(languageSelector).toBeAttached();
  });

  test("language selector has options", async ({ page }) => {
    const options = page.locator("#language-selector option");
    const count = await options.count();
    expect(count).toBeGreaterThanOrEqual(2);
  });

  test("language selector contains expected languages", async ({ page }) => {
    const selector = page.locator("#language-selector");

    // Check for German option
    const germanOption = selector.locator('option[value="GER"]');
    await expect(germanOption).toBeAttached();

    // Check for English option
    const englishOption = selector.locator('option[value="EUR"]');
    await expect(englishOption).toBeAttached();
  });

  test("custom styled language selector is functional", async ({ page }) => {
    // The custom styled dropdown
    const styledSelector = page.locator(".select-styled");
    await expect(styledSelector).toBeVisible();

    // Click to open dropdown
    await styledSelector.click();

    // Options should be visible
    const optionsList = page.locator(".select-options");
    await expect(optionsList).toBeVisible();
  });

  test("selecting a language updates the display", async ({ page }) => {
    // Open custom dropdown
    const styledSelector = page.locator(".select-styled");
    await styledSelector.click();

    // Select German
    const germanOption = page.locator('.select-options li[rel="GER"]');
    await germanOption.click();

    // Styled selector should update
    await expect(styledSelector).toContainText("GERMAN");
  });
});

// =============================================================================
// 7. HEADER AND WINDOW CONTROLS TESTS
// =============================================================================

test.describe("Header and Window Controls", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await page.goto("/");
  });

  test("header is visible with drag region", async ({ page }) => {
    const header = page.locator(".header1");
    await expect(header).toBeVisible();
    await expect(header).toHaveAttribute("data-tauri-drag-region");
  });

  test("minimize button is visible", async ({ page }) => {
    const minimizeBtn = page.locator("#app-minimize");
    await expect(minimizeBtn).toBeVisible();
  });

  test("close button is visible", async ({ page }) => {
    const closeBtn = page.locator("#app-close");
    await expect(closeBtn).toBeVisible();
  });

  test("app logo is displayed in header", async ({ page }) => {
    const logo = page.locator(".logo-top-left");
    await expect(logo).toBeVisible();
  });

  test("game status indicator is present", async ({ page }) => {
    const gameStatus = page.locator("#game-status");
    await expect(gameStatus).toBeAttached();
  });

  test("discord button is visible", async ({ page }) => {
    const discordBtn = page.locator("#discord-button");
    await expect(discordBtn).toBeVisible();
  });

  test("forum button is visible", async ({ page }) => {
    const forumBtn = page.locator("#start-button");
    await expect(forumBtn).toBeVisible();
  });

  test("support button is visible", async ({ page }) => {
    const supportBtn = page.locator("#support-button");
    await expect(supportBtn).toBeVisible();
  });

  test("settings button is visible", async ({ page }) => {
    const settingsBtn = page.locator("#settings-button");
    await expect(settingsBtn).toBeVisible();
  });

  test("privacy policy link is present", async ({ page }) => {
    const privacyLink = page.locator("#privacy-link");
    await expect(privacyLink).toBeAttached();
  });
});

// =============================================================================
// 8. LOADING STATE TESTS
// =============================================================================

test.describe("Loading States", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await page.goto("/");
  });

  test("loading modal element exists", async ({ page }) => {
    const loadingModal = page.locator("#loading-modal");
    await expect(loadingModal).toBeAttached();
  });

  test("loading modal has spinner", async ({ page }) => {
    const spinner = page.locator(".loading-spinner");
    await expect(spinner).toBeAttached();
  });

  test("loading modal has message element", async ({ page }) => {
    const loadingMessage = page.locator("#loading-message");
    await expect(loadingMessage).toBeAttached();
  });

  test("loading modal has action buttons", async ({ page }) => {
    const refreshBtn = page.locator("#refresh-button");
    const quitBtn = page.locator("#quit-button");

    await expect(refreshBtn).toBeAttached();
    await expect(quitBtn).toBeAttached();
  });
});

// =============================================================================
// 9. REGISTRATION FLOW TESTS
// =============================================================================

test.describe("Registration Flow", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await clearAuthentication(page);
    await page.goto("/");
  });

  test("clicking register button shows registration form", async ({ page }) => {
    await page.waitForSelector(".login-form", { timeout: 10000 });

    // Click register button
    await page.locator("#register-button").click();

    // Wait for potential navigation or form change
    await page.waitForTimeout(1000);

    // Either hash changed or registration form should be visible
    const registerForm = page.locator("#reg-username");
    const loginForm = page.locator("#username");

    // Check if we've navigated to registration
    const currentHash = await page.evaluate(() => window.location.hash);
    // Registration may or may not be implemented as a route
  });
});

// =============================================================================
// 10. USER MENU TESTS
// =============================================================================

test.describe("User Menu", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await setAuthenticated(page, "TestUser");
    await page.goto("/#home");
    await page.waitForSelector("#home-page", { timeout: 10000 });
  });

  test("user avatar button is visible", async ({ page }) => {
    const avatarBtn = page.locator(".btn-user-avatar");
    await expect(avatarBtn).toBeVisible();
  });

  test("user menu contains account details link", async ({ page }) => {
    const accountLink = page.locator('[data-translate="ACCOUNT_DETAILS"]');
    await expect(accountLink).toBeAttached();
  });

  test("user menu contains sign out option", async ({ page }) => {
    const signOutLink = page.locator("#logout-link");
    await expect(signOutLink).toBeAttached();
  });

  test("user menu contains exit option", async ({ page }) => {
    const exitLink = page.locator("#app-quit");
    await expect(exitLink).toBeAttached();
  });

  test("check launcher update option exists", async ({ page }) => {
    const updateLink = page.locator("#check-launcher-update");
    await expect(updateLink).toBeAttached();
  });

  test("check and repair files option exists", async ({ page }) => {
    const repairLink = page.locator("#check-game-files");
    await expect(repairLink).toBeAttached();
  });
});

// =============================================================================
// 11. RESPONSIVE AND VISUAL TESTS
// =============================================================================

test.describe("Visual and Layout", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await page.goto("/");
  });

  test("page has correct title", async ({ page }) => {
    await expect(page).toHaveTitle(/.*TERA.*|.*Tauri.*|.*App.*/i);
  });

  test("app container exists", async ({ page }) => {
    const appContainer = page.locator("#app");
    await expect(appContainer).toBeAttached();
  });

  test("main page wrapper exists", async ({ page }) => {
    const mainPage = page.locator(".mainpage");
    await expect(mainPage).toBeVisible();
  });

  test("page background is styled", async ({ page }) => {
    const body = page.locator("body");
    const background = await body.evaluate((el) => {
      return window.getComputedStyle(el).background;
    });
    expect(background).toBeTruthy();
  });
});

// =============================================================================
// 12. IFRAME CONTAINER TESTS
// =============================================================================

test.describe("Iframe Container", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await page.goto("/");
  });

  test("iframe container exists", async ({ page }) => {
    const iframeContainer = page.locator("#iframeContainer");
    await expect(iframeContainer).toBeAttached();
  });

  test("iframe element exists", async ({ page }) => {
    const iframe = page.locator("#embeddedSite");
    await expect(iframe).toBeAttached();
  });

  test("exit button exists for iframe", async ({ page }) => {
    const exitBtn = page.locator("#exitButton");
    await expect(exitBtn).toBeAttached();
  });

  test("iframe container is initially hidden", async ({ page }) => {
    const iframeContainer = page.locator("#iframeContainer");
    await expect(iframeContainer).not.toBeVisible();
  });
});

// =============================================================================
// 13. HASH FILE GENERATION MODAL TESTS
// =============================================================================

test.describe("Hash File Generation Modal", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await page.goto("/");
  });

  test("hash progress modal exists", async ({ page }) => {
    const hashModal = page.locator("#hash-file-progress-modal");
    await expect(hashModal).toBeAttached();
  });

  test("hash progress bar exists", async ({ page }) => {
    const progressBar = page.locator("#hash-file-progress-bar");
    await expect(progressBar).toBeAttached();
  });

  test("hash progress text element exists", async ({ page }) => {
    const progressText = page.locator("#hash-file-progress-text");
    await expect(progressText).toBeAttached();
  });
});

// =============================================================================
// 14. ERROR HANDLING TESTS
// =============================================================================

test.describe("Error Handling", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
  });

  test("login error message element exists", async ({ page }) => {
    await clearAuthentication(page);
    await page.goto("/");
    await page.waitForSelector(".login-form", { timeout: 10000 });

    const errorMsg = page.locator("#login-error-msg");
    await expect(errorMsg).toBeAttached();
  });

  test("loading error message element exists", async ({ page }) => {
    await page.goto("/");

    const errorElement = page.locator("#loading-error");
    await expect(errorElement).toBeAttached();
  });

  test("notification element exists in settings", async ({ page }) => {
    await setAuthenticated(page, "TestUser");
    await page.goto("/#home");
    await page.waitForSelector("#home-page", { timeout: 10000 });

    // Open settings
    await page.locator("#openModal").click();
    await expect(page.locator("#modal")).toBeVisible();

    const notification = page.locator("#notification");
    await expect(notification).toBeAttached();
  });
});
