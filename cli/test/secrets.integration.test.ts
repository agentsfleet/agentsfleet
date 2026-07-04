import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "ws_cred_test";
const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_cred" }, fn);

describe("secret commands", () => {
  test("`secret add` with no existing secret GETs list (empty), POSTs the secret, prints stored", async () => {
    await authedScope(async () => {
      let postBody: string | null = null;
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/secrets`]: () => jsonResponse(200, { secrets: [] }),
        [`POST /v1/workspaces/${WS_ID}/secrets`]: async (_req, _url, body) => {
          postBody = body;
          return jsonResponse(201, { name: "github", created_at: Date.now() });
        },
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "add", "github", `--data={"token":"ghp_test_value"}`],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/stored/i);
        // Ledger: the skip-if-exists GET fires first, then the POST.
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/secrets`,
          `POST /v1/workspaces/${WS_ID}/secrets`,
        ]);
        // The POST body carries the name + opaque data object intact.
        const parsed = JSON.parse(postBody ?? "{}") as {
          name?: string;
          data?: Record<string, unknown>;
        };
        expect(parsed.name).toBe("github");
        expect(parsed.data).toEqual({ token: "ghp_test_value" });
      });
    });
  });

  test("`secret add` skips silently when the name already exists (default upsert guard)", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/secrets`]: () => jsonResponse(200, {
          secrets: [{ name: "github", created_at: 1700000000000 }],
        }),
        // No POST handler — if the CLI hits POST under default upsert, the mock
        // returns 404 and the test surfaces an unexpected call.
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "add", "github", `--data={"token":"ghp_will_not_be_sent"}`],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/already exists/i);
        // Only the preflight GET fired; no POST.
        expect(calls.filter((c) => c.method === "POST")).toHaveLength(0);
      });
    });
  });

  test("`secret add --force` skips the preflight GET and POSTs immediately as overwritten", async () => {
    await authedScope(async () => {
      let postBody: string | null = null;
      const routes: MockRoutes = {
        // No GET handler — if --force trips the preflight, this becomes a 404
        // and the CLI errors out, which the test catches.
        [`POST /v1/workspaces/${WS_ID}/secrets`]: async (_req, _url, body) => {
          postBody = body;
          return jsonResponse(200, { name: "github", created_at: Date.now() });
        },
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "add", "github", `--data={"token":"ghp_force"}`, "--force"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/overwritten/i);
        expect(calls.map((c) => c.method)).toEqual(["POST"]);
        const parsed = JSON.parse(postBody ?? "{}") as {
          name?: string;
          data?: Record<string, unknown>;
        };
        expect(parsed.data).toEqual({ token: "ghp_force" });
      });
    });
  });

  test("`secret list` GETs the vault and prints names without secret bytes", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/secrets`]: () => jsonResponse(200, {
          secrets: [
            { name: "github", created_at: 1700000000000 },
            { name: "slack", created_at: 1700000000001 },
          ],
        }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "list"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
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
          `GET /v1/workspaces/${WS_ID}/secrets`,
        ]);
      });
    });
  });
});
