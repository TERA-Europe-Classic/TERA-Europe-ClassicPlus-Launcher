// @ts-check
import { test, expect } from "@playwright/test";
import { mockTauriAPIs } from "./helpers.js";

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
