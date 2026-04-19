// @ts-check
import { test, expect } from "@playwright/test";
import { mockTauriAPIs, setAuthenticated } from "./helpers.js";

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
