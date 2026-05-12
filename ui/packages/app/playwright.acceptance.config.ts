import { defineConfig, devices } from "@playwright/test";
import { loadWorktreeEnv } from "./tests/e2e/acceptance/fixtures/env-loader";

// Load <worktree-root>/.env so CLERK_SECRET_KEY / CLERK_WEBHOOK_SECRET land
// in process.env before globalSetup runs. Bun auto-loads only this package's
// .env*, which gives us NEXT_PUBLIC_API_URL but not the Clerk creds.
loadWorktreeEnv();

const E2E_PORT = process.env.E2E_PORT ?? "3101";
const BASE_URL = process.env.BASE_URL ?? `http://localhost:${E2E_PORT}`;

export default defineConfig({
  testDir: "./tests/e2e/acceptance",
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: process.env.CI
    ? [["line"], ["html", { open: "never", outputFolder: "playwright-acceptance-report" }]]
    : "line",
  globalSetup: "./tests/e2e/acceptance/global-setup.ts",
  globalTeardown: "./tests/e2e/acceptance/global-teardown.ts",
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
      name: "acceptance-chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: process.env.BASE_URL
    ? undefined
    : {
        command: `bun run dev -- --port ${E2E_PORT}`,
        url: `http://localhost:${E2E_PORT}/sign-in`,
        reuseExistingServer: !process.env.CI,
        timeout: 120_000,
      },
  outputDir: "playwright-acceptance-results",
});
