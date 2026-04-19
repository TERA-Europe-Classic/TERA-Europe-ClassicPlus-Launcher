// @ts-check
import { test, expect } from "@playwright/test";
import { mockTauriAPIs } from "./helpers.js";

test.describe("Header and Window Controls", () => {
  test.beforeEach(async ({ page }) => {
    await mockTauriAPIs(page);
    await page.goto("/");
  });

  test("header is visible with drag region", async ({ page }) => {
    const header = page.locator(".header1");
    await expect(header).toBeVisible();
    await expect(header).toHaveAttribute("data-tauri-drag-region");
  });

  test("minimize button is visible", async ({ page }) => {
    const minimizeBtn = page.locator("#app-minimize");
    await expect(minimizeBtn).toBeVisible();
  });

  test("close button is visible", async ({ page }) => {
    const closeBtn = page.locator("#app-close");
    await expect(closeBtn).toBeVisible();
  });

  test("app logo is displayed in header", async ({ page }) => {
    const logo = page.locator(".logo-top-left");
    await expect(logo).toBeVisible();
  });

  test("game status indicator is present", async ({ page }) => {
    const gameStatus = page.locator("#game-status");
    await expect(gameStatus).toBeAttached();
  });

  test("discord button is visible", async ({ page }) => {
    const discordBtn = page.locator("#discord-button");
    await expect(discordBtn).toBeVisible();
  });

  test("forum button is visible", async ({ page }) => {
    const forumBtn = page.locator("#start-button");
    await expect(forumBtn).toBeVisible();
  });

  test("support button is visible", async ({ page }) => {
    const supportBtn = page.locator("#support-button");
    await expect(supportBtn).toBeVisible();
  });

  test("settings button is visible", async ({ page }) => {
    const settingsBtn = page.locator("#settings-button");
    await expect(settingsBtn).toBeVisible();
  });

  test("privacy policy link is present", async ({ page }) => {
    const privacyLink = page.locator("#privacy-link");
    await expect(privacyLink).toBeAttached();
  });
});
