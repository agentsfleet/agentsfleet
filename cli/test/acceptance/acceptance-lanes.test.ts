import { describe, expect, it } from "bun:test";
import path from "node:path";

import {
  DETERMINISTIC_ACCEPTANCE_FILES,
  LIVE_ACCEPTANCE_FILES,
  LIVE_FILE_CONCURRENCY,
  LIVE_HANDSHAKE_FILE,
  LIVE_SERIAL_FILE,
  liveExecutionPlan,
  parseLaneCounts,
} from "./run-lane.ts";
import { resolveLiveAcceptancePreflight } from "./global-setup.ts";
import { extractLoginUrl } from "./fixtures/pty.ts";

const CLI_ROOT = path.resolve(import.meta.dir, "..", "..");
const ACCEPTANCE_SPEC_GLOB = "test/acceptance/*.spec.ts";
const LIVE_STEER_SPEC = "test/acceptance/steer-live.spec.ts";
const LIVE_STREAMING_SPEC = "test/acceptance/streaming-follow.spec.ts";
const API_URL = "https://api-dev.agentsfleet.net";
const DASHBOARD_URL = "https://app-dev.agentsfleet.net";
const CLERK_SECRET = "fixture-clerk-secret";
const CLERK_PUBLISHABLE_KEY = "fixture-clerk-publishable-key";
const CLERK_WEBHOOK_SECRET = "fixture-clerk-webhook-secret";
const REGULAR_EMAIL = "regular@example.test";
const ADMIN_EMAIL = "admin@example.test";

function completeLiveEnv(): NodeJS.ProcessEnv {
  return {
    AGENTSFLEET_ACCEPTANCE_TARGET: API_URL,
    AGENTSFLEET_ACCEPTANCE_DASHBOARD_URL: DASHBOARD_URL,
    AGENTSFLEET_ACCEPTANCE_LOGIN_HANDSHAKE: "1",
    CLERK_SECRET_KEY: CLERK_SECRET,
    CLERK_PUBLISHABLE_KEY,
    CLERK_WEBHOOK_SECRET,
    AUTH_E2E_REGULAR_EMAIL: REGULAR_EMAIL,
    AUTH_E2E_ADMIN_EMAIL: ADMIN_EMAIL,
  };
}

describe("CLI acceptance lane membership", () => {
  it("classifies every acceptance spec into exactly one lane", async () => {
    const discovered: string[] = [];
    const glob = new Bun.Glob(ACCEPTANCE_SPEC_GLOB);
    for await (const file of glob.scan({ cwd: CLI_ROOT })) discovered.push(file);

    const classified = [
      ...DETERMINISTIC_ACCEPTANCE_FILES,
      ...LIVE_ACCEPTANCE_FILES,
    ].filter((file) => file.endsWith(".spec.ts"));

    expect([...new Set(classified)].sort()).toEqual(discovered.sort());
    expect(classified).toHaveLength(discovered.length);
  });

  it("keeps remote steer work out of the deterministic lane", () => {
    expect(DETERMINISTIC_ACCEPTANCE_FILES).not.toContain(LIVE_STEER_SPEC);
    expect(DETERMINISTIC_ACCEPTANCE_FILES).toContain(LIVE_STREAMING_SPEC);
    expect(LIVE_ACCEPTANCE_FILES).toContain(LIVE_STEER_SPEC);
    expect(LIVE_ACCEPTANCE_FILES).not.toContain(LIVE_STREAMING_SPEC);
  });

  it("bounds live file concurrency and serializes tenant-wide provider mutation", () => {
    expect(LIVE_FILE_CONCURRENCY).toBe(2);
    expect(LIVE_ACCEPTANCE_FILES).toContain(LIVE_SERIAL_FILE);
  });

  it("runs the release-critical browser handoff before every other live file", () => {
    const plan = liveExecutionPlan();
    expect(plan.handshake).toBe(LIVE_HANDSHAKE_FILE);
    expect(plan.parallel).not.toContain(LIVE_HANDSHAKE_FILE);
    expect(plan.parallel).not.toContain(LIVE_SERIAL_FILE);
    expect(plan.serial).toBe(LIVE_SERIAL_FILE);
  });

  it("parses registered, passed, failed, and skipped counts from Bun output", () => {
    const output = [
      "2 tests skipped:",
      " 15 pass",
      " 2 skip",
      " 0 fail",
      "Ran 17 tests across 4 files.",
    ].join("\n");

    expect(parseLaneCounts(output)).toEqual({
      registered: 17,
      passed: 15,
      failed: 0,
      skipped: 2,
    });
  });

  it("rejects output without a complete release summary", () => {
    expect(parseLaneCounts("15 pass\nRan 15 tests")).toBeNull();
  });

  it("accepts a complete browser and fixture preflight environment", () => {
    expect(resolveLiveAcceptancePreflight(completeLiveEnv())).toEqual({
      apiUrl: API_URL,
      dashboardUrl: DASHBOARD_URL,
      clerkSecret: CLERK_SECRET,
      regularEmail: REGULAR_EMAIL,
      adminEmail: ADMIN_EMAIL,
    });
  });

  it("fails preflight when the release-critical browser handoff is disabled", () => {
    const env = completeLiveEnv();
    delete env.AGENTSFLEET_ACCEPTANCE_LOGIN_HANDSHAKE;
    expect(() => resolveLiveAcceptancePreflight(env))
      .toThrow("AGENTSFLEET_ACCEPTANCE_LOGIN_HANDSHAKE must equal 1");
  });

  it("extracts login URLs from human and colon-delimited output", () => {
    expect(extractLoginUrl("  login_url   ·  https://app.test/cli-auth/abc")).toBe(
      "https://app.test/cli-auth/abc",
    );
    expect(extractLoginUrl("login_url: https://app.test/cli-auth/xyz")).toBe(
      "https://app.test/cli-auth/xyz",
    );
    expect(extractLoginUrl("browser: not opened")).toBeNull();
  });
});
