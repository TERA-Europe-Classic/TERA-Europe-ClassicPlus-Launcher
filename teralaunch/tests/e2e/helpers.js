// @ts-check

/**
 * Mock Tauri API calls for browser-based testing.
 * This allows E2E tests to run without the full Tauri runtime.
 */
export async function mockTauriAPIs(page) {
  await page.addInitScript(() => {
    // Mock window.__TAURI__ object that Tauri injects
    window.__TAURI__ = {
      invoke: async (cmd, args) => {
        // Mock responses for common Tauri commands
        const mockResponses = {
          get_game_path: "C:\\Games\\TERA",
          set_game_path: true,
          check_authentication: { authenticated: false },
          login: { success: true, username: "TestUser" },
          logout: true,
          get_download_progress: { progress: 0, total: 100, speed: 0 },
          launch_game: true,
          check_updates: { hasUpdate: false },
          get_version: "1.9.2",
          get_config: { language: "EUR", gamePath: "C:\\Games\\TERA" },
        };
        return mockResponses[cmd] || null;
      },
      event: {
        listen: async (event, handler) => {
          // Return unsubscribe function
          return () => {};
        },
        emit: async (event, payload) => {},
      },
      dialog: {
        open: async (options) => "C:\\Games\\TERA",
      },
      window: {
        appWindow: {
          minimize: async () => {},
          close: async () => {},
        },
      },
    };

    // Mock localStorage for authentication state
    if (!localStorage.getItem("isAuthenticated")) {
      localStorage.setItem("isAuthenticated", "false");
    }
  });
}

/**
 * Helper to simulate authenticated state
 */
export async function setAuthenticated(page, username = "TestUser") {
  await page.evaluate((user) => {
    localStorage.setItem("isAuthenticated", "true");
    localStorage.setItem("userName", user);
    localStorage.setItem("authToken", "mock-token-12345");
  }, username);
}

/**
 * Helper to clear authentication state
 */
export async function clearAuthentication(page) {
  await page.evaluate(() => {
    localStorage.removeItem("isAuthenticated");
    localStorage.removeItem("userName");
    localStorage.removeItem("authToken");
  });
}
