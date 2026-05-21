/**
 * Real-handshake acceptance scenario — `zombiectl login` end-to-end
 * against api-dev with a Playwright Chromium browser leg.
 *
 *   - handshake: drive `login --no-open --no-input`, parse login_url,
 *     complete the dashboard's CLI-auth approve action via browser.js,
 *     assert credentials.json mode 0600 + 3-segment JWT (WS-E #C3).
 *   - persisted-credentials read-only sweep (ZOMBIE_TOKEN explicitly
 *     absent from spawn env; proves credentials.json is the load-
 *     bearing auth source).
 *   - prefix-scoped post-teardown emptiness (zombie list).
 *   - persisted-credentials install + lifecycle walk.
 *
 * Skip posture:
 *   - Live API target — ZOMBIE_ACCEPTANCE_TARGET must be an https URL.
 *   - Dashboard URL is *derived* from the API URL via
 *     `resolveDashboardUrl` — no separate env gate. Override via
 *     `ZOMBIE_ACCEPTANCE_DASHBOARD_URL` for `localhost:3000` runs.
 *   - Dashboard `/cli-auth/{session_id}` page must be deployed.
 *     Until that page ships, the dashboard redirects unknown routes
 *     to `/sign-in`, breaking the handshake. Override the skip with
 *     `ZOMBIE_ACCEPTANCE_LOGIN_HANDSHAKE=1` once the page is live.
 *
 * WS-E #C1 regression: assertNoSecretLeak fires after every spawn.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { READ_ONLY_COMMANDS } from "./fixtures/command-matrix.ts";
import { ACCEPTANCE_RUN_PREFIX } from "./fixtures/constants.ts";
import { composeEnv, runZombiectl, spawnZombiectl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveDashboardUrl,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { completeCliAuthHandoff } from "./fixtures/browser.ts";
import { installPlatformOpsZombie } from "./fixtures/seed.ts";
import { cleanWorkspaceZombies } from "./fixtures/teardown.ts";
import {
  expectStatus,
  killZombie,
  resumeZombie,
  stopZombie,
} from "./fixtures/lifecycle.ts";

import type { ChildProcessWithoutNullStreams } from "node:child_process";

const target = process.env.ZOMBIE_ACCEPTANCE_TARGET ?? "";
const isLive = target.startsWith("https://");

// The dashboard's CLI-auth handoff page (`/cli-auth/{session_id}` with
// `data-testid="cli-auth-approve"`) is not implemented in
// `ui/packages/app/` yet — verified by source grep. Without it the
// browser leg redirects to `/sign-in` and the login flow can't
// complete. Remove this skip in the same PR that ships the page.
const dashboardHandshakeImplemented = false;

interface ExitCapture {
  readonly code: number | null;
  readonly stdout: string;
  readonly stderr: string;
}

function parseLoginUrl(stdout: string): string {
  // The CLI prints "login_url: <URL>" inside the Login session block.
  const match = stdout.match(/login_url:\s*(https?:\/\/\S+)/i);
  if (!match || !match[1]) throw new Error(`could not find login_url in CLI stdout: ${stdout.slice(0, 400)}`);
  return match[1];
}

function rewriteHost(loginUrl: string, dashboardBase: string): string {
  // The CLI's login_url is the API-host shape (api-dev.usezombie.com). The
  // dashboard's CLI-auth handoff page lives on the dashboard host. We swap
  // the host while preserving path + query (which carries session_id).
  const src = new URL(loginUrl);
  const dst = new URL(dashboardBase);
  src.protocol = dst.protocol;
  src.host = dst.host;
  return src.toString();
}

function waitForLine(
  child: ChildProcessWithoutNullStreams,
  predicate: (line: string) => boolean,
  timeoutMs: number,
): Promise<string> {
  return new Promise((resolve, reject) => {
    let buffer = "";
    const timer = setTimeout(() => {
      child.stdout.off("data", onData);
      reject(new Error(`timed out waiting for stdout line; saw: ${buffer.slice(0, 400)}`));
    }, timeoutMs);
    function onData(chunk: Buffer | string): void {
      buffer += String(chunk);
      const lines = buffer.split(/\r?\n/);
      for (const line of lines) {
        if (predicate(line)) {
          clearTimeout(timer);
          child.stdout.off("data", onData);
          resolve(buffer);
          return;
        }
      }
    }
    child.stdout.on("data", onData);
  });
}

function awaitExit(child: ChildProcessWithoutNullStreams): Promise<ExitCapture> {
  return new Promise((resolve) => {
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (c: Buffer | string) => { stdout += String(c); });
    child.stderr.on("data", (c: Buffer | string) => { stderr += String(c); });
    child.on("close", (code: number | null) => resolve({ code, stdout, stderr }));
  });
}

if (!isLive) {
  describe("lifecycle-after-login.spec.ts", () => {
    it.skip("requires ZOMBIE_ACCEPTANCE_TARGET to be an https URL", () => {});
  });
} else if (!dashboardHandshakeImplemented) {
  describe("lifecycle-after-login.spec.ts", () => {
    it.skip("dashboard /cli-auth page not implemented in ui/packages/app/ yet — flip dashboardHandshakeImplemented when it ships", () => {});
  });
} else {
  describe("lifecycle-after-login — real login → persisted credentials", () => {
    let apiUrl: string = "";
    let dashboardUrl: string = "";
    let sessionJwt: string = "";
    let cookieJwt: string = "";
    let stateDir: string = "";
    let baseEnv: Record<string, string> = {};
    let credentialsPath: string = "";

    async function spawn(args: ReadonlyArray<string>, extraEnv?: Record<string, string>): Promise<RunResult> {
      const env = extraEnv ? { ...baseEnv, ...extraEnv } : baseEnv;
      const result = await runZombiectl(args, { env });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    beforeAll(async () => {
      apiUrl = resolveAcceptanceEnv().apiUrl;
      dashboardUrl = resolveDashboardUrl(apiUrl);
      const clerkSecret = resolveClerkSecret();
      const email = resolveFixtureEmail("regular");
      const minted = await attachJwt(clerkSecret, { email });
      sessionJwt = minted.sessionJwt;
      cookieJwt = minted.cookieJwt;

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-login-"));
      credentialsPath = path.join(stateDir, "credentials.json");
      baseEnv = composeEnv({
        ZOMBIE_API_URL: apiUrl,
        ZOMBIE_STATE_DIR: stateDir,
        NO_COLOR: "1",
        // ZOMBIE_TOKEN intentionally absent — every spawn proves
        // credentials.json is the load-bearing auth source.
      });
    });

    afterAll(async () => {
      try { await cleanWorkspaceZombies(baseEnv, { runPrefix: ACCEPTANCE_RUN_PREFIX }); } catch { /* best-effort teardown */ }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // CLI login handshake — drive the dashboard's /cli-auth approve action.
    describe("handshake", () => {
      it("login --no-open --no-input → approve via Chromium → credentials.json 0600", async () => {
        // NOTE: the device flow is now terminal-only (a spawned non-TTY login
        // with no token fast-fails). Driving it here will need a PTY harness;
        // until then this live case cannot reach the code prompt.
        const args = ["login", "--no-open", "--no-input"];
        const child = spawnZombiectl(args, { env: baseEnv });
        const seen = await waitForLine(child, (line: string) => /login_url/i.test(line), 30_000);
        const apiLoginUrl = parseLoginUrl(seen);
        const handoffUrl = rewriteHost(apiLoginUrl, dashboardUrl);

        await completeCliAuthHandoff({ loginUrl: handoffUrl, cookieJwt, timeoutMs: 60_000 });

        const finished = await awaitExit(child);
        assert.equal(finished.code, 0, `login exited ${finished.code}; stderr=${finished.stderr}; stdout=${finished.stdout}`);

        const stat = await fs.stat(credentialsPath);
        assert.equal(stat.mode & 0o777, 0o600, `credentials.json mode is ${(stat.mode & 0o777).toString(8)} — expected 600 (WS-E #C3)`);

        const creds = JSON.parse(await fs.readFile(credentialsPath, "utf8")) as { token: string };
        assert.equal(typeof creds.token, "string");
        assert.equal(creds.token.split(".").length, 3, `token is not a 3-segment JWT: ${creds.token}`);

        const combined = `${finished.stdout}\n${finished.stderr}`;
        assert.ok(!combined.includes(sessionJwt), "WS-E #C1: minted JWT leaked into login stdout/stderr");
      });
    });

    // Persisted-credentials read-only sweep (no ZOMBIE_TOKEN).
    describe("read-only sweep using persisted credentials", () => {
      for (const row of READ_ONLY_COMMANDS) {
        const label = row.label ?? row.args.join(" ");
        it(`${label} exits 0 against persisted credentials.json`, async () => {
          // Helper guards: env constructed here MUST NOT carry ZOMBIE_TOKEN.
          assert.equal(baseEnv["ZOMBIE_TOKEN"], undefined, "baseEnv must not contain ZOMBIE_TOKEN");
          const result = await spawn(row.args);
          assert.equal(result.code, 0, `${label} exited ${result.code}: ${result.stderr}`);
          const parsed = JSON.parse(result.stdout.trim()) as Record<string, unknown>;
          if (row.requiredKey) {
            assert.ok(row.requiredKey in parsed, `${label}: missing ${row.requiredKey} in ${result.stdout}`);
          }
          if (row.isList && row.itemsKey) {
            assert.ok(Array.isArray(parsed[row.itemsKey]), `${label}: ${row.itemsKey} not an array`);
          }
        });
      }
    });

    // Prefix-scoped post-teardown emptiness (zombie list).
    // Same contract as the ZOMBIE_TOKEN spec: shared DEV tenants carry
    // residual zombies; the only assertion that holds is "none of MY
    // run's zombies remain after teardown".
    describe("post-teardown emptiness (prefix-scoped)", () => {
      beforeAll(async () => {
        await cleanWorkspaceZombies(baseEnv, { runPrefix: ACCEPTANCE_RUN_PREFIX });
      });

      it(`zombie list --json: no items match ACCEPTANCE_RUN_PREFIX`, async () => {
        const result = await spawn(["list", "--json"]);
        assert.equal(result.code, 0);
        const parsed = JSON.parse(result.stdout.trim()) as { items?: unknown };
        const items = Array.isArray(parsed.items) ? (parsed.items as Array<{ name?: string }>) : [];
        const mine = items.filter((z) => typeof z.name === "string" && z.name.startsWith(ACCEPTANCE_RUN_PREFIX));
        assert.equal(mine.length, 0,
          `expected zero zombies starting with ${ACCEPTANCE_RUN_PREFIX}; got ${mine.length}: ${JSON.stringify(mine)}`);
      });
    });

    // Persisted-credentials install + lifecycle (no ZOMBIE_TOKEN).
    describe("install + lifecycle (no ZOMBIE_TOKEN)", () => {
      let zombieId: string = "";

      it("install platform-ops uses persisted creds", async () => {
        const installed = await installPlatformOpsZombie({ env: baseEnv, runPrefix: ACCEPTANCE_RUN_PREFIX });
        const id = installed.id ?? installed.zombie_id;
        assert.ok(id, `install missing id: ${JSON.stringify(installed)}`);
        zombieId = id as string;
      });

      it("status → stop → resume → kill walks state", async () => {
        await expectStatus(baseEnv, zombieId, ["active", "starting", "running"]);
        await stopZombie(baseEnv, zombieId);
        await expectStatus(baseEnv, zombieId, ["paused", "stopped"]);
        await resumeZombie(baseEnv, zombieId);
        await expectStatus(baseEnv, zombieId, ["active", "running", "starting"]);
        await killZombie(baseEnv, zombieId);
        await expectStatus(baseEnv, zombieId, ["killed", "errored", "terminated"]);
      });
    });
  });
}
