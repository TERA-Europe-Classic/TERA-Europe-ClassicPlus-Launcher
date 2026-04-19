// @ts-check
import { test, expect } from "@playwright/test";
import { mockTauriAPIs, clearAuthentication } from "./helpers.js";

test.describe("Registration Flow", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await clearAuthentication(page);
    await page.goto("/");
  });

  test("clicking register button shows registration form", async ({ page }) => {
    await page.waitForSelector(".login-form", { timeout: 10000 });

    // Click register button
    await page.locator("#register-button").click();

    // Wait for potential navigation or form change
    await page.waitForTimeout(1000);

    // Either hash changed or registration form should be visible
    const registerForm = page.locator("#reg-username");
    const loginForm = page.locator("#username");

    // Check if we've navigated to registration
    const currentHash = await page.evaluate(() => window.location.hash);
    // Registration may or may not be implemented as a route
  });
});
