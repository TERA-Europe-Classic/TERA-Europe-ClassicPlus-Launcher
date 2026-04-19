// @ts-check
import { test, expect } from "@playwright/test";
import { mockTauriAPIs, setAuthenticated } from "./helpers.js";

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
