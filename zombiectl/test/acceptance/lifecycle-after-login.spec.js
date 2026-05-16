/**
 * Real-handshake acceptance scenario — `zombiectl login` end-to-end
 * against api-dev with a Playwright Chromium browser leg.
 *
 *   §5a — drive `login --no-open --no-input`, parse login_url, complete
 *          the dashboard's CLI-auth approve action via browser.js,
 *          assert credentials.json mode 0600 + 3-segment JWT (WS-E #C3).
 *   §5b — persisted-credentials read-only sweep (ZOMBIE_TOKEN explicitly
 *          absent from spawn env; proves credentials.json is the load-
 *          bearing auth source).
 *   §5b' — empty-list parity vs §4b' (zombie list).
 *   §5c — persisted-credentials install + lifecycle.
 *
 * Skip posture (live + dashboard handoff required):
 *   - ZOMBIE_ACCEPTANCE_TARGET must be an https URL (api-dev).
 *   - ZOMBIE_ACCEPTANCE_DASHBOARD_URL must be set — the dashboard's
 *     CLI-auth handoff page (`/cli-auth/{session_id}`) lands in a
 *     sibling PR and the CI job supplies the URL.  Until both
 *     conditions hold, the entire suite skips loudly so the gap is
 *     visible.
 *
 * WS-E #C1 regression: assertNoSecretLeak fires after every spawn.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import {
  EMPTY_LIST_CONVENTIONS,
  READ_ONLY_COMMANDS,
} from "./fixtures/command-matrix.js";
import { composeEnv, runZombiectl, spawnZombiectl } from "./fixtures/cli.js";
import { assertNoSecretLeak } from "./fixtures/negatives.js";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveFixtureEmail,
} from "./global-setup.js";
import { attachJwt } from "./fixtures/clerk-admin.js";
import { completeCliAuthHandoff } from "./fixtures/browser.js";
import { installPlatformOpsZombie } from "./fixtures/seed.js";
import { cleanWorkspaceZombies } from "./fixtures/teardown.js";
import {
  expectStatus,
  killZombie,
  resumeZombie,
  stopZombie,
} from "./fixtures/lifecycle.js";

const target = process.env.ZOMBIE_ACCEPTANCE_TARGET ?? "";
const dashboardUrl = process.env.ZOMBIE_ACCEPTANCE_DASHBOARD_URL ?? "";
const isLive = target.startsWith("https://");
const hasDashboard = dashboardUrl.startsWith("https://") || dashboardUrl.startsWith("http://localhost");

function parseLoginUrl(stdout) {
  // The CLI prints "login_url: <URL>" inside the Login session block.
  const match = stdout.match(/login_url:\s*(https?:\/\/\S+)/i);
  if (!match) throw new Error(`could not find login_url in CLI stdout: ${stdout.slice(0, 400)}`);
  return match[1];
}

function rewriteHost(loginUrl, dashboardBase) {
  // The CLI's login_url is the API-host shape (api-dev.usezombie.com). The
  // dashboard's CLI-auth handoff page lives on the dashboard host. We swap
  // the host while preserving path + query (which carries session_id).
  const src = new URL(loginUrl);
  const dst = new URL(dashboardBase);
  src.protocol = dst.protocol;
  src.host = dst.host;
  return src.toString();
}

function waitForLine(child, predicate, timeoutMs) {
  return new Promise((resolve, reject) => {
    let buffer = "";
    const timer = setTimeout(() => {
      child.stdout.off("data", onData);
      reject(new Error(`timed out waiting for stdout line; saw: ${buffer.slice(0, 400)}`));
    }, timeoutMs);
    function onData(chunk) {
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

function awaitExit(child) {
  return new Promise((resolve) => {
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (c) => { stdout += String(c); });
    child.stderr.on("data", (c) => { stderr += String(c); });
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });
}

if (!isLive || !hasDashboard) {
  describe("lifecycle-after-login.spec.js", () => {
    it.skip("requires https ZOMBIE_ACCEPTANCE_TARGET + ZOMBIE_ACCEPTANCE_DASHBOARD_URL (dashboard /cli-auth route)", () => {});
  });
} else {
  describe("lifecycle-after-login — real login → persisted credentials", () => {
    let apiUrl;
    let sessionJwt;
    let cookieJwt;
    let stateDir;
    let baseEnv;
    let credentialsPath;

    async function spawn(args, extraEnv) {
      const env = extraEnv ? { ...baseEnv, ...extraEnv } : baseEnv;
      const result = await runZombiectl(args, { env });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    beforeAll(async () => {
      apiUrl = resolveAcceptanceEnv().apiUrl;
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
      try { await cleanWorkspaceZombies(baseEnv); } catch {}
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // §5a — handshake
    describe("§5a handshake", () => {
      it("login --no-open --no-input → approve via Chromium → credentials.json 0600", async () => {
        const args = [
          "login", "--no-open", "--no-input",
          "--timeout-sec", "90",
          "--poll-ms", "500",
        ];
        const child = spawnZombiectl(args, { env: baseEnv });
        const seen = await waitForLine(child, (line) => /login_url/i.test(line), 30_000);
        const apiLoginUrl = parseLoginUrl(seen);
        const handoffUrl = rewriteHost(apiLoginUrl, dashboardUrl);

        await completeCliAuthHandoff({ loginUrl: handoffUrl, cookieJwt, timeoutMs: 60_000 });

        const finished = await awaitExit(child);
        assert.equal(finished.code, 0, `login exited ${finished.code}; stderr=${finished.stderr}; stdout=${finished.stdout}`);

        const stat = await fs.stat(credentialsPath);
        assert.equal(stat.mode & 0o777, 0o600, `credentials.json mode is ${(stat.mode & 0o777).toString(8)} — expected 600 (WS-E #C3)`);

        const creds = JSON.parse(await fs.readFile(credentialsPath, "utf8"));
        assert.equal(typeof creds.token, "string");
        assert.equal(creds.token.split(".").length, 3, `token is not a 3-segment JWT: ${creds.token}`);

        const combined = `${finished.stdout}\n${finished.stderr}`;
        assert.ok(!combined.includes(sessionJwt), "WS-E #C1: minted JWT leaked into login stdout/stderr");
      });
    });

    // §5b — persisted-credentials read-only sweep (no ZOMBIE_TOKEN)
    describe("§5b read-only sweep using persisted credentials", () => {
      for (const row of READ_ONLY_COMMANDS) {
        const label = row.label ?? row.args.join(" ");
        it(`${label} exits 0 against persisted credentials.json`, async () => {
          // Helper guards: env constructed here MUST NOT carry ZOMBIE_TOKEN.
          assert.equal(baseEnv.ZOMBIE_TOKEN, undefined, "baseEnv must not contain ZOMBIE_TOKEN");
          const result = await spawn(row.args);
          assert.equal(result.code, 0, `${label} exited ${result.code}: ${result.stderr}`);
          const parsed = JSON.parse(result.stdout.trim());
          if (row.requiredKey) {
            assert.ok(row.requiredKey in parsed, `${label}: missing ${row.requiredKey} in ${result.stdout}`);
          }
          if (row.isList) {
            assert.ok(Array.isArray(parsed[row.itemsKey]), `${label}: ${row.itemsKey} not an array`);
          }
        });
      }
    });

    // §5b' — empty-list parity (zombie list — guaranteed empty post-teardown)
    describe("§5b' empty-list parity (zombie list)", () => {
      beforeAll(async () => {
        await cleanWorkspaceZombies(baseEnv);
      });

      it(`zombie list --json: items array empty`, async () => {
        const result = await spawn(["list", "--json"]);
        assert.equal(result.code, 0);
        const parsed = JSON.parse(result.stdout.trim());
        assert.ok(Array.isArray(parsed.items) && parsed.items.length === 0,
          `expected empty items: ${result.stdout}`);
      });

      it(`zombie list (non-JSON) emits standard stem`, async () => {
        const result = await spawn(["list"]);
        assert.equal(result.code, 0);
        const stem = EMPTY_LIST_CONVENTIONS["list"];
        assert.match(result.stdout.toLowerCase(), new RegExp(stem.toLowerCase()));
      });
    });

    // §5c — persisted-credentials install + lifecycle
    describe("§5c install + lifecycle (no ZOMBIE_TOKEN)", () => {
      let zombieId;

      it("install platform-ops uses persisted creds", async () => {
        const installed = await installPlatformOpsZombie({ env: baseEnv });
        zombieId = installed.id ?? installed.zombie_id;
        assert.ok(zombieId, `install missing id: ${JSON.stringify(installed)}`);
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
