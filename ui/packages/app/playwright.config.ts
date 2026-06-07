import { defineConfig, devices } from "@playwright/test";

// E2E dev server port — unique per package to avoid collisions with locally-running
// Next.js apps on :3000. Override by exporting BASE_URL.
const E2E_PORT = process.env.E2E_PORT ?? "3100";
const BASE_URL = process.env.BASE_URL ?? `http://localhost:${E2E_PORT}`;

// Playwright reporter + trace/video capture-mode literals, reused below.
const LINE_REPORTER = "line";
const ON_FIRST_RETRY = "on-first-retry";

export default defineConfig({
  testDir: "./tests/e2e",
  // Acceptance-suite specs require Clerk DEV credentials + globalSetup
  // (see playwright.acceptance.config.ts). Exclude them from the default
  // suite so the `qa-app` lane (no Clerk env) does not collect them.
  testIgnore: ["**/acceptance/**"],
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI
    ? [[LINE_REPORTER], ["html", { open: "never", outputFolder: "playwright-report" }]]
    : LINE_REPORTER,
  use: {
    baseURL: BASE_URL,
    extraHTTPHeaders: process.env.VERCEL_BYPASS_SECRET
      ? {
          "x-vercel-protection-bypass": process.env.VERCEL_BYPASS_SECRET,
          "x-vercel-set-bypass-cookie": "true",
        }
      : {},
    trace: ON_FIRST_RETRY,
    screenshot: "only-on-failure",
    video: ON_FIRST_RETRY,
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
        // @clerk/nextjs >=7.4 hard-throws without a publishable + secret key
        // at render (no more keyless fallback), so the dry server needs both.
        // Source them from the environment — the 1Password vault in CI (see
        // .github/workflows/dry.yml) or .env.local locally — and pass them
        // through to the spawned Next server. Omit when unset so Next can
        // still load them from .env.local on disk. The publishable key is
        // public (it ships in the client bundle); the secret key never is.
        env: {
          ...(process.env.NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY
            ? { NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY: process.env.NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY }
            : {}),
          ...(process.env.CLERK_SECRET_KEY
            ? { CLERK_SECRET_KEY: process.env.CLERK_SECRET_KEY }
            : {}),
          NEXT_PUBLIC_CLERK_SIGN_IN_URL: "/sign-in",
          NEXT_PUBLIC_CLERK_SIGN_UP_URL: "/sign-up",
        },
      },
  outputDir: "playwright-results",
});
