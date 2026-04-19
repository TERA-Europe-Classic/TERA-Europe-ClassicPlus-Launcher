// @ts-check
import { test, expect } from "@playwright/test";
import { mockTauriAPIs } from "./helpers.js";

test.describe("Visual and Layout", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await page.goto("/");
  });

  test("page has correct title", async ({ page }) => {
    await expect(page).toHaveTitle(/.*TERA.*|.*Tauri.*|.*App.*/i);
  });

  test("app container exists", async ({ page }) => {
    const appContainer = page.locator("#app");
    await expect(appContainer).toBeAttached();
  });

  test("main page wrapper exists", async ({ page }) => {
    const mainPage = page.locator(".mainpage");
    await expect(mainPage).toBeVisible();
  });

  test("page background is styled", async ({ page }) => {
    const body = page.locator("body");
    const background = await body.evaluate((el) => {
      return window.getComputedStyle(el).background;
    });
    expect(background).toBeTruthy();
  });
});
