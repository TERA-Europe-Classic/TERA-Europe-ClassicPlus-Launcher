import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright configuration for TERA Launcher E2E tests.
 *
 * Note: For Tauri apps, tests run against the webview content served by the dev server.
 * The Tauri backend APIs need to be mocked for browser-based testing.
 */
export default defineConfig({
    testDir: './tests/e2e',

    /* Run tests in files in parallel */
    fullyParallel: true,

    /* Fail the build on CI if you accidentally left test.only in the source code */
    forbidOnly: !!process.env.CI,

    /* Retry on CI only */
    retries: process.env.CI ? 2 : 0,

    /* Opt out of parallel tests on CI for stability */
    workers: process.env.CI ? 1 : undefined,

    /* Reporter to use */
    reporter: [
        ['html', { outputFolder: 'playwright-report' }],
        ['list']
    ],

    /* Shared settings for all the projects below */
    use: {
        /* Base URL for the Tauri dev server */
        baseURL: 'http://localhost:1420',

        /* Collect trace when retrying the failed test */
        trace: 'on-first-retry',

        /* Screenshot on failure */
        screenshot: 'only-on-failure',

        /* Video on failure */
        video: 'retain-on-failure',
    },

    /* Configure projects for different browsers */
    projects: [
        {
            name: 'chromium',
            use: { ...devices['Desktop Chrome'] },
        },
        /* Uncomment for additional browser testing
        {
            name: 'firefox',
            use: { ...devices['Desktop Firefox'] },
        },
        {
            name: 'webkit',
            use: { ...devices['Desktop Safari'] },
        },
        */
    ],

    /* Global timeout for each test */
    timeout: 30000,

    /* Expect timeout */
    expect: {
        timeout: 5000,
    },

    /* Run your local dev server before starting the tests.
     *
     * Timeout raised to 10 minutes because `npm run tauri dev` on a cold
     * cache compiles the Rust backend, which can easily exceed the stock
     * 120 s budget on this machine. Pre-warming tip: run
     * `cd teralaunch/src-tauri && cargo build` once before the first e2e
     * run of the day — subsequent boots hit the warm cache in seconds. */
    webServer: {
        command: 'npm run tauri dev',
        url: 'http://localhost:1420',
        reuseExistingServer: !process.env.CI,
        timeout: 600000,
        stdout: 'pipe',
        stderr: 'pipe',
    },
});
