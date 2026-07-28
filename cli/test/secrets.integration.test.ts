import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "ws_cred_test";
const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_cred" }, fn);

describe("secret commands", () => {
  test("`secret create` POSTs once and prints stored — no preflight read", async () => {
    await authedScope(async () => {
      let postBody: string | null = null;
      const routes: MockRoutes = {
        // No GET handler at all. A preflight would 404 here and fail the test —
        // which is the point: the server owns the name-is-free decision now.
        [`POST /v1/workspaces/${WS_ID}/secrets`]: async (_req, _url, body) => {
          postBody = body;
          return jsonResponse(201, { name: "github", created_at: Date.now() });
        },
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "create", "github", `--data={"token":"ghp_test_value"}`],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/stored/i);
        // Ledger: exactly one round-trip. The check-then-write pair this
        // replaced could not have been atomic anyway.
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
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

  test("`secret create` on a taken name is a skip, not a failure — exit 0, no stack, delete named as the way out", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST /v1/workspaces/${WS_ID}/secrets`]: () =>
          jsonResponse(409, {
            error: { code: "UZ-VAULT-005", message: "Secret name already taken" },
          }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "create", "github", `--data={"token":"ghp_second_attempt"}`],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        // Exit 0: re-running a provisioning script over names it already
        // created must be quiet, not an aborted run.
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toMatch(/already exists/i);
        // The recovery is named, because there is no longer a flag for it.
        expect(text).toMatch(/secret delete github/);
        expect(text).not.toMatch(/UZ-VAULT-005/);
        expect(calls.map((c) => c.method)).toEqual(["POST"]);
      });
    });
  });

  test("a 409 that is NOT a taken name still fails loudly", async () => {
    // The skip is keyed on `UZ-VAULT-005`, not on the bare status, so a future
    // conflict on this route surfaces instead of being swallowed as a no-op.
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST /v1/workspaces/${WS_ID}/secrets`]: () =>
          jsonResponse(409, {
            error: { code: "UZ-VAULT-009", message: "Vault sealed" },
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "create", "github", `--data={"token":"ghp_x"}`],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        expect(`${out.read()}${err.read()}`).toMatch(/UZ-VAULT-009/);
      });
    });
  });

  test("`--force` is gone — the endpoint no longer upserts, so the flag could only have failed", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "create", "github", `--data={"token":"ghp_x"}`, "--force"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        // Rejected at parse time: nothing reaches the API, so a script still
        // passing the flag fails before it sends a secret anywhere.
        expect(calls).toHaveLength(0);
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
