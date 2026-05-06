import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.js";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.js";
import { withMockApi, jsonResponse } from "./helpers-mock-api.js";

const WS_ID = "ws_cred_test";
const authedScope = (fn) => withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_cred" }, fn);

describe("credential commands", () => {
  test("`credential add` with no existing credential GETs list (empty), POSTs the secret, prints stored", async () => {
    await authedScope(async () => {
      let postBody = null;
      const routes = {
        [`GET /v1/workspaces/${WS_ID}/credentials`]: () => jsonResponse(200, { credentials: [] }),
        [`POST /v1/workspaces/${WS_ID}/credentials`]: async (_req, _url, body) => {
          postBody = body;
          return jsonResponse(201, { name: "github", created_at: Date.now() });
        },
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "add", "github", `--data={"token":"ghp_test_value"}`],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/stored/i);
        // Ledger: the skip-if-exists GET fires first, then the POST.
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/credentials`,
          `POST /v1/workspaces/${WS_ID}/credentials`,
        ]);
        // The POST body carries the name + opaque data object intact.
        const parsed = JSON.parse(postBody);
        expect(parsed.name).toBe("github");
        expect(parsed.data).toEqual({ token: "ghp_test_value" });
      });
    });
  });

  test("`credential add` skips silently when the name already exists (default upsert guard)", async () => {
    await authedScope(async () => {
      const routes = {
        [`GET /v1/workspaces/${WS_ID}/credentials`]: () => jsonResponse(200, {
          credentials: [{ name: "github", created_at: 1700000000000 }],
        }),
        // No POST handler — if the CLI hits POST under default upsert, the mock
        // returns 404 and the test surfaces an unexpected call.
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "add", "github", `--data={"token":"ghp_will_not_be_sent"}`],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/already exists/i);
        // Only the preflight GET fired; no POST.
        expect(calls.filter((c) => c.method === "POST")).toHaveLength(0);
      });
    });
  });

  test("`credential add --force` skips the preflight GET and POSTs immediately as overwritten", async () => {
    await authedScope(async () => {
      let postBody = null;
      const routes = {
        // No GET handler — if --force trips the preflight, this becomes a 404
        // and the CLI errors out, which the test catches.
        [`POST /v1/workspaces/${WS_ID}/credentials`]: async (_req, _url, body) => {
          postBody = body;
          return jsonResponse(200, { name: "github", created_at: Date.now() });
        },
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "add", "github", `--data={"token":"ghp_force"}`, "--force"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/overwritten/i);
        expect(calls.map((c) => c.method)).toEqual(["POST"]);
        const parsed = JSON.parse(postBody);
        expect(parsed.data).toEqual({ token: "ghp_force" });
      });
    });
  });

  test("`credential list` GETs the vault and prints names without secret bytes", async () => {
    await authedScope(async () => {
      const routes = {
        [`GET /v1/workspaces/${WS_ID}/credentials`]: () => jsonResponse(200, {
          credentials: [
            { name: "github", created_at: 1700000000000 },
            { name: "slack", created_at: 1700000000001 },
          ],
        }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "list"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("github");
        expect(text).toContain("slack");
        // Negative assertion: secret-shaped substrings never appear in list output
        // (the API would never return secret bytes here, but locking it down so a
        // future regression that prints an unexpected field surfaces immediately).
        expect(text).not.toContain("ghp_");
        expect(text).not.toMatch(/token/i);
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/credentials`,
        ]);
      });
    });
  });
});
