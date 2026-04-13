import { defineConfig, devices } from "@playwright/test";

const BASE_URL = process.env.BASE_URL ?? "http://127.0.0.1:5173";

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
      ? {
          "x-vercel-protection-bypass": process.env.VERCEL_BYPASS_SECRET,
          "x-vercel-set-bypass-cookie": "true",
        }
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
  ],
  // Start dev server automatically for local runs (not used in Vercel e2e mode)
  webServer: process.env.BASE_URL
    ? undefined
    : {
        command: "bun run dev -- --host 127.0.0.1",
        url: "http://127.0.0.1:5173",
        reuseExistingServer: false,
        timeout: 30_000,
      },
  outputDir: "playwright-results",
});
