// @ts-check
import { test, expect } from "@playwright/test";
import { mockTauriAPIs, setAuthenticated } from "./helpers.js";

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
