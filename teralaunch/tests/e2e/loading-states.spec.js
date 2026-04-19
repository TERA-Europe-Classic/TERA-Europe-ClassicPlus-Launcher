// @ts-check
import { test, expect } from "@playwright/test";
import { mockTauriAPIs } from "./helpers.js";

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
