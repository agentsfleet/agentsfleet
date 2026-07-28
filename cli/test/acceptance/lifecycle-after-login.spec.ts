/**
 * Real-handshake acceptance scenario — `agentsfleet login` end-to-end
 * against api-dev with a Playwright Chromium browser leg and a real pty.
 *
 *   - handshake: drive `login --no-open` inside a pseudo-terminal (the
 *     device flow refuses a non-TTY stdin), parse login_url, complete the
 *     dashboard's CLI-auth approve action via browser.ts, scrape the 6-digit
 *     code it displays, type it into the pty prompt, assert credentials.json
 *     mode 0600 + 3-segment JWT (WS-E #C3).
 *   - immediate auth-status proof with no env API key
 *     (AGENTSFLEET_API_KEY), so credentials.json is the load-bearing source.
 *
 * Skip posture:
 *   - Live API target — AGENTSFLEET_ACCEPTANCE_TARGET must be an https URL.
 *   - Dashboard URL is *derived* from the API URL via `resolveDashboardUrl`
 *     — no separate env gate. Override via `AGENTSFLEET_ACCEPTANCE_DASHBOARD_URL`
 *     for `localhost:3000` runs.
 *
 * WS-E #C1 regression: assertNoSecretLeak fires after every spawn.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { extractLoginUrl, PtyProcess } from "./fixtures/pty.ts";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveDashboardUrl,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { completeCliAuthHandoff } from "./fixtures/browser.ts";

const target = process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "";
const isLive = target.startsWith("https://");

const CODE_PROMPT_RE = /verification code/i;
const CREDENTIALS_MODE = 0o600;
const JWT_SEGMENTS = 3;
const HANDSHAKE_TIMEOUT_MS = 60_000;
const AUTH_SOURCE_FILE = "file" as const;

function parseLoginUrl(output: string): string {
  const loginUrl = extractLoginUrl(output);
  if (!loginUrl) throw new Error(`could not find login_url in CLI output: ${output.slice(0, 400)}`);
  return loginUrl;
}

function rewriteHost(loginUrl: string, dashboardBase: string): string {
  // The CLI's login_url is the dashboard-host shape already, but when the
  // acceptance dashboard override points elsewhere (e.g. localhost:3000) we
  // swap host while preserving path + query (which carries session_id).
  const src = new URL(loginUrl);
  const dst = new URL(dashboardBase);
  src.protocol = dst.protocol;
  src.host = dst.host;
  return src.toString();
}

if (!isLive) {
  describe("lifecycle-after-login.spec.ts", () => {
    it.skip("requires AGENTSFLEET_ACCEPTANCE_TARGET to be an https URL", () => {});
  });
} else {
  describe("lifecycle-after-login — real login → persisted credentials", () => {
    let apiUrl: string = "";
    let dashboardUrl: string = "";
    let sessionJwt: string = "";
    let clerkUserId: string = "";
    let fixtureEmail: string = "";
    let stateDir: string = "";
    let baseEnv: Record<string, string> = {};
    let credentialsPath: string = "";

    async function spawn(args: ReadonlyArray<string>, extraEnv?: Record<string, string>): Promise<RunResult> {
      const env = extraEnv ? { ...baseEnv, ...extraEnv } : baseEnv;
      const result = await runFleetctl(args, { env });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    beforeAll(async () => {
      apiUrl = resolveAcceptanceEnv().apiUrl;
      dashboardUrl = resolveDashboardUrl(apiUrl);
      const clerkSecret = resolveClerkSecret();
      fixtureEmail = resolveFixtureEmail("regular");
      const minted = await attachJwt(clerkSecret, { email: fixtureEmail });
      sessionJwt = minted.sessionJwt;
      clerkUserId = minted.clerkUserId;

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-login-"));
      credentialsPath = path.join(stateDir, "credentials.json");
      baseEnv = composeEnv({
        AGENTSFLEET_API_URL: apiUrl,
        AGENTSFLEET_STATE_DIR: stateDir,
        NO_COLOR: "1",
        // No env API key (AGENTSFLEET_API_KEY) — every spawn proves
        // credentials.json is the load-bearing auth source.
      });
    });

    afterAll(async () => {
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // CLI login handshake — drive the device flow through a pty, complete
    // the browser approve leg, and type the displayed code back into the CLI.
    describe("handshake", () => {
      it("login --no-open → approve via Chromium → credentials.json 0600", async () => {
        // No --no-input: the pty makes stdin a terminal, so the device flow
        // runs the interactive verification prompt instead of fast-failing.
        const cli = PtyProcess.spawnFleetctl(["login", "--no-open"], { env: baseEnv });
        try {
          const announced = await cli.waitForLine((line) => extractLoginUrl(line) !== null, HANDSHAKE_TIMEOUT_MS);
          const handoffUrl = rewriteHost(parseLoginUrl(announced), dashboardUrl);

          const code = await completeCliAuthHandoff({
            loginUrl: handoffUrl,
            clerkUserId,
            timeoutMs: HANDSHAKE_TIMEOUT_MS,
          });

          await cli.waitForLine((line) => CODE_PROMPT_RE.test(line), HANDSHAKE_TIMEOUT_MS);
          cli.writeLine(code);

          const exitCode = await cli.exited;
          assert.equal(exitCode, 0, `login exited ${exitCode}; output=${cli.output}`);
        } finally {
          cli.kill();
        }

        const stat = await fs.stat(credentialsPath);
        assert.equal(stat.mode & 0o777, CREDENTIALS_MODE, `credentials.json mode is ${(stat.mode & 0o777).toString(8)} — expected 600 (WS-E #C3)`);

        const creds = JSON.parse(await fs.readFile(credentialsPath, "utf8")) as { token: string };
        assert.equal(typeof creds.token, "string");
        assert.equal(creds.token.split(".").length, JWT_SEGMENTS, `token is not a 3-segment JWT: ${creds.token}`);

        const authStatus = await spawn(["auth", "status", "--json"]);
        assert.equal(authStatus.code, 0,
          `persisted auth status exited ${authStatus.code}: ${authStatus.stderr}`);
        const status = JSON.parse(authStatus.stdout.trim()) as {
          authenticated?: boolean;
          source?: string;
        };
        assert.equal(status.authenticated, true);
        assert.equal(status.source, AUTH_SOURCE_FILE);

        // WS-E #C1: the minted browser-leg JWT must never surface on the pty.
        assertNoSecretLeak({ stdout: cli.output, stderr: "" }, sessionJwt);
      });
    });
  });
}
