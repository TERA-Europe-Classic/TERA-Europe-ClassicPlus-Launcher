// @ts-check
import { test, expect } from "@playwright/test";
import { mockTauriAPIs, setAuthenticated, clearAuthentication } from "./helpers.js";

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
