/**
 * Real-handshake acceptance scenario — `agentsfleet login` end-to-end
 * against api-dev with a Playwright Chromium browser leg and a real pty.
 *
 *   - handshake: drive `login --no-open` inside a pseudo-terminal (the
 *     device flow refuses a non-TTY stdin), parse login_url, complete the
 *     dashboard's CLI-auth approve action via browser.ts, scrape the 6-digit
 *     code it displays, type it into the pty prompt, assert credentials.json
 *     mode 0600 + 3-segment JWT (WS-E #C3).
 *   - persisted-credentials read-only sweep (AGENTSFLEET_TOKEN explicitly
 *     absent from spawn env; proves credentials.json is the load-
 *     bearing auth source).
 *   - prefix-scoped post-teardown emptiness (agent list).
 *   - persisted-credentials install + lifecycle walk.
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

import { READ_ONLY_COMMANDS } from "./fixtures/command-matrix.ts";
import { ACCEPTANCE_RUN_PREFIX } from "./fixtures/constants.ts";
import { composeEnv, runAgentctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { PtyProcess } from "./fixtures/pty.ts";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveDashboardUrl,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { completeCliAuthHandoff } from "./fixtures/browser.ts";
import { installPlatformOpsAgent } from "./fixtures/seed.ts";
import { cleanWorkspaceAgents } from "./fixtures/teardown.ts";
import {
  expectStatus,
  killAgent,
  resumeAgent,
  stopAgent,
} from "./fixtures/lifecycle.ts";

const target = process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "";
const isLive = target.startsWith("https://");

// The browser leg requires the dashboard at AGENTSFLEET_ACCEPTANCE_DASHBOARD_URL
// to actually SERVE `/cli-auth/{session_id}`. Verified against api-dev's
// dashboard (agentsfleet.vercel.app, 2026-06-19) that route currently
// returns 404 — the page exists in source but is not deployed there yet, so
// the whole real-login handshake (and the persisted-credential sweep that
// depends on it seeding credentials.json) cannot complete. Opt in with
// AGENTSFLEET_ACCEPTANCE_LOGIN_HANDSHAKE=1 once the dashboard ships the route;
// the token-injection suite (lifecycle-with-token) covers the post-auth
// surface live in the meantime.
const handshakeEnabled = process.env.AGENTSFLEET_ACCEPTANCE_LOGIN_HANDSHAKE === "1";

// printKeyValue renders the key space-aligned ("login_url   https://…"), not
// "login_url: …" — match an optional colon then whitespace before the URL.
const LOGIN_URL_RE = /login_url:?\s+(https?:\/\/\S+)/i;
const CODE_PROMPT_RE = /verification code/i;
const CREDENTIALS_MODE = 0o600;
const JWT_SEGMENTS = 3;
const HANDSHAKE_TIMEOUT_MS = 60_000;

function parseLoginUrl(output: string): string {
  // The CLI prints "login_url: <URL>" inside the Login session block.
  const match = output.match(LOGIN_URL_RE);
  if (!match || !match[1]) throw new Error(`could not find login_url in CLI output: ${output.slice(0, 400)}`);
  return match[1];
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
} else if (!handshakeEnabled) {
  describe("lifecycle-after-login.spec.ts", () => {
    it.skip("dashboard /cli-auth route not deployed — set AGENTSFLEET_ACCEPTANCE_LOGIN_HANDSHAKE=1 once it ships", () => {});
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
      const result = await runAgentctl(args, { env });
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

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-login-"));
      credentialsPath = path.join(stateDir, "credentials.json");
      baseEnv = composeEnv({
        AGENTSFLEET_API_URL: apiUrl,
        AGENTSFLEET_STATE_DIR: stateDir,
        NO_COLOR: "1",
        // AGENTSFLEET_TOKEN intentionally absent — every spawn proves
        // credentials.json is the load-bearing auth source.
      });
    });

    afterAll(async () => {
      try { await cleanWorkspaceAgents(baseEnv, { runPrefix: ACCEPTANCE_RUN_PREFIX }); } catch { /* best-effort teardown */ }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // CLI login handshake — drive the device flow through a pty, complete
    // the browser approve leg, and type the displayed code back into the CLI.
    describe("handshake", () => {
      it("login --no-open → approve via Chromium → credentials.json 0600", async () => {
        // No --no-input: the pty makes stdin a terminal, so the device flow
        // runs the interactive verification prompt instead of fast-failing.
        const cli = PtyProcess.spawnAgentctl(["login", "--no-open"], { env: baseEnv });
        try {
          const announced = await cli.waitForLine((line) => LOGIN_URL_RE.test(line), HANDSHAKE_TIMEOUT_MS);
          const handoffUrl = rewriteHost(parseLoginUrl(announced), dashboardUrl);

          const code = await completeCliAuthHandoff({ loginUrl: handoffUrl, cookieJwt, timeoutMs: HANDSHAKE_TIMEOUT_MS });

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

        // WS-E #C1: the minted browser-leg JWT must never surface on the pty.
        assertNoSecretLeak({ stdout: cli.output, stderr: "" }, sessionJwt);
      });
    });

    // Persisted-credentials read-only sweep (no AGENTSFLEET_TOKEN).
    describe("read-only sweep using persisted credentials", () => {
      for (const row of READ_ONLY_COMMANDS) {
        const label = row.label ?? row.args.join(" ");
        it(`${label} exits 0 against persisted credentials.json`, async () => {
          // Helper guards: env constructed here MUST NOT carry AGENTSFLEET_TOKEN.
          assert.equal(baseEnv["AGENTSFLEET_TOKEN"], undefined, "baseEnv must not contain AGENTSFLEET_TOKEN");
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

    // Prefix-scoped post-teardown emptiness (agent list).
    // Same contract as the AGENTSFLEET_TOKEN spec: shared DEV tenants carry
    // residual agents; the only assertion that holds is "none of MY
    // run's agents remain after teardown".
    describe("post-teardown emptiness (prefix-scoped)", () => {
      beforeAll(async () => {
        await cleanWorkspaceAgents(baseEnv, { runPrefix: ACCEPTANCE_RUN_PREFIX });
      });

      it(`agent list --json: no items match ACCEPTANCE_RUN_PREFIX`, async () => {
        const result = await spawn(["list", "--json"]);
        assert.equal(result.code, 0);
        const parsed = JSON.parse(result.stdout.trim()) as { items?: unknown };
        const items = Array.isArray(parsed.items) ? (parsed.items as Array<{ name?: string }>) : [];
        const mine = items.filter((z) => typeof z.name === "string" && z.name.startsWith(ACCEPTANCE_RUN_PREFIX));
        assert.equal(mine.length, 0,
          `expected zero agents starting with ${ACCEPTANCE_RUN_PREFIX}; got ${mine.length}: ${JSON.stringify(mine)}`);
      });
    });

    // Persisted-credentials install + lifecycle (no AGENTSFLEET_TOKEN).
    describe("install + lifecycle (no AGENTSFLEET_TOKEN)", () => {
      let agentId: string = "";

      it("install platform-ops uses persisted creds", async () => {
        const installed = await installPlatformOpsAgent({ env: baseEnv, runPrefix: ACCEPTANCE_RUN_PREFIX });
        const id = installed.id ?? installed.agent_id;
        assert.ok(id, `install missing id: ${JSON.stringify(installed)}`);
        agentId = id as string;
      });

      it("status → stop → resume → kill walks state", async () => {
        await expectStatus(baseEnv, agentId, ["active", "starting", "running"]);
        await stopAgent(baseEnv, agentId);
        await expectStatus(baseEnv, agentId, ["paused", "stopped"]);
        await resumeAgent(baseEnv, agentId);
        await expectStatus(baseEnv, agentId, ["active", "running", "starting"]);
        await killAgent(baseEnv, agentId);
        await expectStatus(baseEnv, agentId, ["killed", "errored", "terminated"]);
      });
    });
  });
}
