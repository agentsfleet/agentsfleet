import { defineConfig, devices } from "@playwright/test";

// E2E dev server port — unique per package to avoid collisions with locally-running
// Next.js apps on :3000. Override by exporting BASE_URL.
const E2E_PORT = process.env.E2E_PORT ?? "3100";
const BASE_URL = process.env.BASE_URL ?? `http://localhost:${E2E_PORT}`;

export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI
    ? [["line"], ["html", { open: "never", outputFolder: "playwright-report" }]]
    : "line",
  use: {
    baseURL: BASE_URL,
    extraHTTPHeaders: process.env.VERCEL_BYPASS_SECRET
      ? { "x-vercel-protection-bypass": process.env.VERCEL_BYPASS_SECRET }
      : {},
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    video: "on-first-retry",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
    {
      name: "mobile-chromium",
      use: { ...devices["Pixel 7"] },
    },
  ],
  webServer: process.env.BASE_URL
    ? undefined
    : {
        command: `bun run dev -- --port ${E2E_PORT}`,
        url: `http://localhost:${E2E_PORT}/sign-in`,
        reuseExistingServer: !process.env.CI,
        // Next.js Turbopack cold start can exceed 45s on first run after install.
        timeout: 120_000,
      },
  outputDir: "playwright-results",
});
