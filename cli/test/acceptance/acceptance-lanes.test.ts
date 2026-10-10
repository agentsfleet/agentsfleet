import { describe, expect, it } from "bun:test";
import { mkdir, mkdtemp, rm, symlink } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import {
  LIVE_ACCEPTANCE_FILES,
  LIVE_HANDSHAKE_FILE,
  LIVE_SERIAL_FILE,
  acceptanceTestCommand,
  deterministicAcceptanceFiles,
  laneOutcome,
  liveExecutionPlan,
  parseLaneCounts,
} from "./run-lane.ts";
import { resolveLiveAcceptancePreflight } from "./global-setup.ts";
import { extractLoginUrl } from "./fixtures/pty.ts";

const CLI_ROOT = path.resolve(import.meta.dir, "..", "..");
const NEW_SPEC = "test/acceptance/brand-new.spec.ts";
const NEW_HARNESS_CHECK = "test/acceptance/fixtures/brand-new.test.ts";
const HARNESS_HELPER = "test/acceptance/fixtures/brand-new.ts";
const COUNTS_CLEAN = { registered: 3, passed: 3, failed: 0, skipped: 0 };
const COUNTS_WITH_SKIP = { registered: 3, passed: 2, failed: 0, skipped: 1 };
const API_URL = "https://api-dev.agentsfleet.net";
const DASHBOARD_URL = "https://app-dev.agentsfleet.net";
const CLERK_SECRET = "fixture-clerk-secret";
const CLERK_PUBLISHABLE_KEY = "fixture-clerk-publishable-key";
const CLERK_WEBHOOK_SECRET = "fixture-clerk-webhook-secret";
const REGULAR_EMAIL = "regular@example.test";
const ADMIN_EMAIL = "admin@example.test";
const CHILD_DEADLINE_MS = 35_000;

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
  it("keeps the live lane deadline after the shared unit preload runs", async () => {
    const directory = await mkdtemp(path.join(os.tmpdir(), "agentsfleet-live-deadline-"));
    try {
      await Bun.write(path.join(directory, "bunfig.toml"), Bun.file(path.join(CLI_ROOT, "bunfig.toml")));
      await symlink(path.join(CLI_ROOT, "test"), path.join(directory, "test"), "dir");
      const fixture = path.join(directory, "deadline.test.ts");
      // The real unit preload sets 15s. Crossing it proves the live preload
      // restores the lane's existing 120s budget, beyond a source-string check.
      await Bun.write(fixture, [
        'import { it } from "bun:test";',
        'it("live setup outlasts the unit deadline", async () => { await Bun.sleep(16_000); });',
      ].join("\n"));
      const child = Bun.spawn([...acceptanceTestCommand("live", [fixture])], {
        cwd: directory,
        env: {
          PATH: process.env.PATH ?? "",
          AGENTSFLEET_ACCEPTANCE_TARGET: API_URL,
          AGENTSFLEET_STATE_DIR: path.join(directory, "state"),
          AGENTSFLEET_TELEMETRY_DISABLED: "1",
        },
        stdout: "pipe",
        stderr: "pipe",
      });
      const deadline = setTimeout(() => child.kill(), CHILD_DEADLINE_MS);
      try {
        const [stdout, stderr, code] = await Promise.all([
          new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited,
        ]);
        expect(code, `${stdout}\n${stderr}`).toBe(0);
        expect(parseLaneCounts(`${stdout}\n${stderr}`)).toEqual({
          registered: 1, passed: 1, failed: 0, skipped: 0,
        });
      } finally {
        clearTimeout(deadline);
        if (child.exitCode === null) child.kill();
        await child.exited;
      }
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  }, 45_000);

  it("runs every acceptance file the live list does not name in the deterministic lane", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "agentsfleet-lane-tree-"));
    try {
      await mkdir(path.join(root, "test", "acceptance", "fixtures"), { recursive: true });
      for (const file of [NEW_SPEC, NEW_HARNESS_CHECK, HARNESS_HELPER, LIVE_HANDSHAKE_FILE, LIVE_SERIAL_FILE]) {
        await Bun.write(path.join(root, file), "");
      }

      expect(deterministicAcceptanceFiles(root)).toEqual([NEW_HARNESS_CHECK, NEW_SPEC].sort());
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  it("names only live files that exist, so no live spec is lost to a typo", async () => {
    const missing: string[] = [];
    for (const file of LIVE_ACCEPTANCE_FILES) {
      if (!(await Bun.file(path.join(CLI_ROOT, file)).exists())) missing.push(file);
    }
    expect(missing).toEqual([]);
  });

  it("fails the deterministic lane when any test skips", () => {
    expect(laneOutcome("deterministic", 0, COUNTS_WITH_SKIP)).toEqual({
      exitCode: 1,
      terminalOutcome: "tests_skipped",
    });
    expect(laneOutcome("deterministic", 0, COUNTS_CLEAN)).toEqual({ exitCode: 0, terminalOutcome: "passed" });
  });

  it("lets the live lane skip, and reports a failing run as failed in either lane", () => {
    expect(laneOutcome("live", 0, COUNTS_WITH_SKIP)).toEqual({ exitCode: 0, terminalOutcome: "passed" });
    expect(laneOutcome("deterministic", 1, COUNTS_CLEAN)).toEqual({ exitCode: 1, terminalOutcome: "failed" });
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
