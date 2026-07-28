import { defineConfig, devices } from "@playwright/test";
import { createHash } from "node:crypto";
import * as path from "node:path";
import { VERCEL_BYPASS_STATE_FILENAME } from "./tests/e2e/acceptance/fixtures/constants";
import { loadWorktreeEnv } from "./tests/e2e/acceptance/fixtures/env-loader";

// Load <worktree-root>/.env so CLERK_SECRET_KEY / CLERK_WEBHOOK_SECRET land
// in process.env before globalSetup runs. Bun auto-loads only this package's
// .env*, which gives us NEXT_PUBLIC_API_URL but not the Clerk creds.
loadWorktreeEnv();

const DEFAULT_PORT_BASE = 30_000;
const DEFAULT_PORT_SPAN = 20_000;

function worktreePort(): string {
  const digest = createHash("sha256").update(process.cwd()).digest();
  return String(DEFAULT_PORT_BASE + (digest.readUInt16BE(0) % DEFAULT_PORT_SPAN));
}

const E2E_PORT = process.env.E2E_PORT ?? worktreePort();
const BASE_URL = process.env.BASE_URL ?? `http://localhost:${E2E_PORT}`;
const REPORTER_LINE = "line" as const;
// Retries are zero, so on-first-retry capture would never fire; failed tests
// keep their first (only) trace and video instead.
const RETAIN_ON_FAILURE = "retain-on-failure" as const;
const ACCEPTANCE_AUDIT_ENABLED = "1";
const ACCEPTANCE_AUDIT_TOKEN =
  process.env.AGENTSFLEET_E2E_AUDIT_TOKEN ?? "local-acceptance-audit-token";

// Raw evidence (traces, screenshots, per-test artifacts, JSON summary) and the
// rendered report live apart so CI can upload both even when the HTML render
// never happened (canceled or hard-failed runs still leave raw artifacts).
const RAW_RESULTS_DIR = "playwright-acceptance-results";
const HTML_REPORT_DIR = "playwright-acceptance-report";
const RAW_RESULTS_JSON = `${RAW_RESULTS_DIR}/results.json`;

// Concurrency is earned by isolation, not configured optimistically: the
// journey group holds only specs whose seeds, users, and assertions are
// prefix- or tenant-scoped, so worker count is a throughput knob, not a
// correctness knob.
const SUITE_WORKERS = 2;

// Latency budgets for a live remote environment at two-way parallelism —
// Playwright's defaults (30s test / 5s expect) assume a local dev server.
// Two workers still share one small dev deployment; without headroom,
// healthy journeys burn their whole budget and fail as blanket timeouts.
// Retries stay at zero: a
// deterministic failure still fails exactly once, it just gets the time a
// remote round-trip actually needs.
const REMOTE_ENV_TEST_TIMEOUT_MS = 60_000;
const REMOTE_ENV_EXPECT_TIMEOUT_MS = 10_000;

// ── Suite groups ─────────────────────────────────────────────────────────────
// preflight  — environment prerequisites + auth wire (_smoke). Everything else
//              depends on it: a dead prerequisite stops the run in seconds.
// journeys   — fixture-isolated user journeys; files run across all workers.
// operator catalog → operator journey — platform-operator mutations (catalog
//              publish/unpublish, operator installs) are serialized by
//              project-dependency chaining, never interleaved.
// live-counter → pulse-wall — whole-wall count assertions in the shared
//              regular workspace; they run after the journey group is done
//              seeding and never concurrently with each other.
// fetch-audit — resets a global app-side fetch counter; runs strictly last.
const PREFLIGHT_SPEC = "**/_smoke.spec.ts";
const OPERATOR_CATALOG_SPEC = "**/platform-library-onboarding.spec.ts";
const OPERATOR_JOURNEY_SPEC = "**/operator-journey.spec.ts";
const LIVE_COUNTER_SPEC = "**/fleet-count.spec.ts";
const PULSE_WALL_SPEC = "**/multi-fleet.spec.ts";
const FETCH_AUDIT_SPEC = "**/workspace-fetch-dedupe.spec.ts";

const PROJECT_PREFLIGHT = "preflight";
const PROJECT_JOURNEYS = "journeys";
const PROJECT_OPERATOR_CATALOG = "operator-catalog";
const PROJECT_OPERATOR_JOURNEY = "operator-journey";
const PROJECT_LIVE_COUNTER = "live-counter";
const PROJECT_PULSE_WALL = "pulse-wall";
const PROJECT_FETCH_AUDIT = "fetch-audit";

const CHROMIUM = { ...devices["Desktop Chrome"] };

export default defineConfig({
  testDir: "./tests/e2e/acceptance",
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  // Release-critical runs never retry: a deterministic failure must surface
  // once with its first useful trace. A genuinely transient boundary earns an
  // explicit, named retry at the call site — not a blanket second attempt.
  retries: 0,
  workers: SUITE_WORKERS,
  timeout: REMOTE_ENV_TEST_TIMEOUT_MS,
  expect: { timeout: REMOTE_ENV_EXPECT_TIMEOUT_MS },
  reporter: process.env.CI
    ? [
        [REPORTER_LINE],
        ["html", { open: "never", outputFolder: HTML_REPORT_DIR }],
        ["json", { outputFile: RAW_RESULTS_JSON }],
      ]
    : REPORTER_LINE,
  globalSetup: "./tests/e2e/acceptance/global-setup.ts",
  globalTeardown: "./tests/e2e/acceptance/global-teardown.ts",
  use: {
    baseURL: BASE_URL,
    // The raw Vercel bypass secret never rides on browser requests: global
    // setup trades it for the derived short-lived cookie in this storage
    // state, so retained failure traces record no loaded secret value.
    storageState: path.join(process.cwd(), VERCEL_BYPASS_STATE_FILENAME),
    trace: RETAIN_ON_FAILURE,
    screenshot: "only-on-failure",
    // Video stays off: it is not part of the release evidence (raw results,
    // report, traces, screenshots), and recording every test across multiple
    // concurrent workers starves the suite into blanket timeouts.
    video: "off",
  },
  projects: [
    {
      name: PROJECT_PREFLIGHT,
      testMatch: PREFLIGHT_SPEC,
      use: CHROMIUM,
    },
    {
      name: PROJECT_JOURNEYS,
      testIgnore: [
        PREFLIGHT_SPEC,
        OPERATOR_CATALOG_SPEC,
        OPERATOR_JOURNEY_SPEC,
        LIVE_COUNTER_SPEC,
        PULSE_WALL_SPEC,
        FETCH_AUDIT_SPEC,
      ],
      dependencies: [PROJECT_PREFLIGHT],
      use: CHROMIUM,
    },
    {
      name: PROJECT_OPERATOR_CATALOG,
      testMatch: OPERATOR_CATALOG_SPEC,
      dependencies: [PROJECT_PREFLIGHT],
      use: CHROMIUM,
    },
    {
      name: PROJECT_OPERATOR_JOURNEY,
      testMatch: OPERATOR_JOURNEY_SPEC,
      dependencies: [PROJECT_OPERATOR_CATALOG],
      use: CHROMIUM,
    },
    {
      name: PROJECT_LIVE_COUNTER,
      testMatch: LIVE_COUNTER_SPEC,
      dependencies: [PROJECT_JOURNEYS],
      use: CHROMIUM,
    },
    {
      name: PROJECT_PULSE_WALL,
      testMatch: PULSE_WALL_SPEC,
      dependencies: [PROJECT_LIVE_COUNTER],
      use: CHROMIUM,
    },
    {
      name: PROJECT_FETCH_AUDIT,
      testMatch: FETCH_AUDIT_SPEC,
      // Strictly last: the audit reset touches an app-global counter, so it
      // must outlast BOTH the wall chain and the operator chain.
      dependencies: [PROJECT_PULSE_WALL, PROJECT_OPERATOR_JOURNEY],
      use: CHROMIUM,
    },
  ],
  webServer: process.env.BASE_URL
    ? undefined
    : {
        command: `bun run build && AGENTSFLEET_E2E_AUDIT=${ACCEPTANCE_AUDIT_ENABLED} AGENTSFLEET_E2E_AUDIT_TOKEN=${ACCEPTANCE_AUDIT_TOKEN} bun run start -- --port ${E2E_PORT}`,
        url: `http://localhost:${E2E_PORT}/sign-in`,
        reuseExistingServer: false,
        timeout: 120_000,
      },
  outputDir: RAW_RESULTS_DIR,
});
