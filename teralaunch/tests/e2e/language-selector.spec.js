// @ts-check
import { test, expect } from "@playwright/test";
import { mockTauriAPIs } from "./helpers.js";

test.describe("Language Selector", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await page.goto("/");
  });

  test("language selector is visible in header", async ({ page }) => {
    const languageSelector = page.locator("#language-selector");
    await expect(languageSelector).toBeAttached();
  });

  test("language selector has options", async ({ page }) => {
    const options = page.locator("#language-selector option");
    const count = await options.count();
    expect(count).toBeGreaterThanOrEqual(2);
  });

  test("language selector contains expected languages", async ({ page }) => {
    const selector = page.locator("#language-selector");

    // Check for German option
    const germanOption = selector.locator('option[value="GER"]');
    await expect(germanOption).toBeAttached();

    // Check for English option
    const englishOption = selector.locator('option[value="EUR"]');
    await expect(englishOption).toBeAttached();
  });

  test("custom styled language selector is functional", async ({ page }) => {
    // The custom styled dropdown
    const styledSelector = page.locator(".select-styled");
    await expect(styledSelector).toBeVisible();

    // Click to open dropdown
    await styledSelector.click();

    // Options should be visible
    const optionsList = page.locator(".select-options");
    await expect(optionsList).toBeVisible();
  });

  test("selecting a language updates the display", async ({ page }) => {
    // Open custom dropdown
    const styledSelector = page.locator(".select-styled");
    await styledSelector.click();

    // Select German
    const germanOption = page.locator('.select-options li[rel="GER"]');
    await germanOption.click();

    // Styled selector should update
    await expect(styledSelector).toContainText("GERMAN");
  });
});
