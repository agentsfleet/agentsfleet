import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import path from "node:path";

import { runCli } from "../src/cli.js";
import { loadCredentials, loadWorkspaces } from "../src/lib/state.js";
import { bufferStream, withFreshStateDir } from "./helpers-cli-state.js";
import { withMockApi, jsonResponse } from "./helpers-mock-api.js";

const completeLoginRoutes = (token = "fake-jwt-token") => ({
  "POST /v1/auth/sessions": () => jsonResponse(201, {
    session_id: "sess_onboard",
    login_url: "https://login.test/sess_onboard",
  }),
  "GET /v1/auth/sessions/sess_onboard": () => jsonResponse(200, {
    status: "complete",
    token,
  }),
});

describe("first-time user onboarding", () => {
  test("login from a fresh state dir bootstraps credentials.json with the right shape", async () => {
    await withFreshStateDir(async (stateDir) => {
      await withMockApi(completeLoginRoutes("jwt_first_login"), async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["login", "--no-open", "--no-input", "--timeout-sec", "5", "--poll-ms", "50"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toContain("login complete");

        // Side effect: credentials.json now exists with the right shape + token.
        const creds = await loadCredentials();
        expect(creds.token).toBe("jwt_first_login");
        expect(creds.session_id).toBe("sess_onboard");
        expect(creds.api_url).toBe(apiUrl);
        expect(typeof creds.saved_at).toBe("number");

        // File is at 0600.
        const credPath = path.join(stateDir, "credentials.json");
        const stat = await fs.stat(credPath);
        expect(stat.mode & 0o777).toBe(0o600);

        // Outbound call ledger: POST then ≥1 GET poll.
        expect(calls[0]).toMatchObject({ method: "POST", path: "/v1/auth/sessions" });
        expect(calls[1]).toMatchObject({ method: "GET", path: "/v1/auth/sessions/sess_onboard" });
      });
    });
  });

  test("login then workspace add creates workspaces.json with the new id as current", async () => {
    await withFreshStateDir(async () => {
      const routes = {
        ...completeLoginRoutes("jwt_for_workspace"),
        "POST /v1/workspaces": () => jsonResponse(201, {
          workspace_id: "ws_onboard_001",
          name: "jolly-harbor-482",
        }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const env = { ZOMBIE_API_URL: apiUrl };
        // Step 1: login.
        const loginOut = bufferStream();
        const loginErr = bufferStream();
        const loginCode = await runCli(
          ["login", "--no-open", "--no-input", "--timeout-sec", "5", "--poll-ms", "50"],
          { stdout: loginOut.stream, stderr: loginErr.stream, env },
        );
        expect(loginCode).toBe(0);

        // Step 2: workspace add (token now persisted; auth guard passes).
        const wsOut = bufferStream();
        const wsErr = bufferStream();
        const wsCode = await runCli(["workspace", "add", "my-first-repo"], {
          stdout: wsOut.stream, stderr: wsErr.stream, env,
        });
        expect(wsCode).toBe(0);

        const ws = await loadWorkspaces();
        expect(ws.current_workspace_id).toBe("ws_onboard_001");
        expect(ws.items).toHaveLength(1);
        expect(ws.items[0].workspace_id).toBe("ws_onboard_001");
      });
    });
  });

  test("login flow exits 1 cleanly when the auth session expires", async () => {
    await withFreshStateDir(async () => {
      const routes = {
        "POST /v1/auth/sessions": () => jsonResponse(201, {
          session_id: "sess_expire", login_url: "https://login.test/sess_expire",
        }),
        "GET /v1/auth/sessions/sess_expire": () => jsonResponse(200, { status: "expired" }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["login", "--no-open", "--no-input", "--timeout-sec", "5", "--poll-ms", "50"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(1);
        expect(err.read()).toContain("expired");
        // No credentials should have been written.
        const creds = await loadCredentials();
        expect(creds.token).toBeNull();
      });
    });
  });

  test("login flow exits 1 cleanly when the polling deadline elapses", async () => {
    await withFreshStateDir(async () => {
      const routes = {
        "POST /v1/auth/sessions": () => jsonResponse(201, {
          session_id: "sess_timeout", login_url: "https://login.test/sess_timeout",
        }),
        "GET /v1/auth/sessions/sess_timeout": () => jsonResponse(200, { status: "pending" }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["login", "--no-open", "--no-input", "--timeout-sec", "1", "--poll-ms", "100"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(1);
        expect(err.read()).toContain("timed out");
        const creds = await loadCredentials();
        expect(creds.token).toBeNull();
      });
    });
  });

  test("an auth-required command without credentials exits 1 with a clear message", async () => {
    await withFreshStateDir(async () => {
      // No mock API needed — the auth guard short-circuits before any fetch.
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["doctor"], {
        stdout: out.stream, stderr: err.stream, env: {},
      });
      expect(code).toBe(1);
      expect(err.read()).toMatch(/not authenticated/i);
    });
  });

  test("corrupt credentials.json is treated as no-token; auth guard fires cleanly", async () => {
    await withFreshStateDir(async (stateDir) => {
      // Pre-seed a broken credentials.json. loadCredentials' readJson catches
      // SyntaxError and falls back to the empty sentinel — the customer is
      // effectively logged out and the auth guard short-circuits doctor.
      await fs.mkdir(stateDir, { recursive: true });
      await fs.writeFile(path.join(stateDir, "credentials.json"), "{ this is not json", { mode: 0o600 });
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["doctor"], {
        stdout: out.stream, stderr: err.stream, env: {},
      });
      expect(code).toBe(1);
      expect(err.read()).toMatch(/not authenticated/i);
      // The corrupt file is preserved untouched; loadCredentials never overwrites
      // on parse failure (only saveCredentials writes, and we didn't save anything).
      const stillBroken = await fs.readFile(path.join(stateDir, "credentials.json"), "utf8");
      expect(stillBroken).toBe("{ this is not json");
    });
  });
});
