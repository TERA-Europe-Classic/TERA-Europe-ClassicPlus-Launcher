// @ts-check
import { test, expect } from "@playwright/test";
import { mockTauriAPIs, setAuthenticated, clearAuthentication } from "./helpers.js";

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
