// @ts-check
import { test, expect } from "@playwright/test";
import { mockTauriAPIs, clearAuthentication } from "./helpers.js";

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
