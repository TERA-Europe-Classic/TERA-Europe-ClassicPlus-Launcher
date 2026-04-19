// @ts-check
import { test, expect } from "@playwright/test";
import { mockTauriAPIs, setAuthenticated } from "./helpers.js";

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
