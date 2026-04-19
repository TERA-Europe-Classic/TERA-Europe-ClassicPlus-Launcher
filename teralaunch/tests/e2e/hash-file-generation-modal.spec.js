// @ts-check
import { test, expect } from "@playwright/test";
import { mockTauriAPIs } from "./helpers.js";

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
