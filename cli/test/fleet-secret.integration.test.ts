// Integration tests for `secret show`, `secret delete`, --json modes,
// and human-mode list rows. The baseline add/list happy paths are in
// secrets.integration.test.ts. Validation error branches that cannot be
// reached through the CLI parser live in fleet-secret-errors.unit.test.ts.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "ws_cred_ext_test";
const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_cred_ext" }, fn);

// ---------------------------------------------------------------------------
// secret show
// ---------------------------------------------------------------------------

describe("secret show", () => {
  test("prints existence confirmation when secret is found (human mode)", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/secrets`]: () =>
          jsonResponse(200, { secrets: [{ name: "github", created_at: 1700000000000 }] }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "show", "github"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toMatch(/exists/i);
        expect(text).toContain("github");
        expect(text).toContain("1700000000000");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/secrets`,
        ]);
      });
    });
  });

  test("prints JSON payload when secret is found (--json mode)", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/secrets`]: () =>
          jsonResponse(200, { secrets: [{ name: "slack", created_at: 1700000001000 }] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "show", "slack", "--json"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const parsed = JSON.parse(out.read()) as {
          name?: string;
          exists?: boolean;
          created_at?: number;
        };
        expect(parsed.name).toBe("slack");
        expect(parsed.exists).toBe(true);
        expect(parsed.created_at).toBe(1700000001000);
      });
    });
  });

  test("returns non-zero and prints not-found message when secret is missing (human mode)", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/secrets`]: () =>
          jsonResponse(200, { secrets: [] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "show", "missing-key"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        expect(out.read() + err.read()).toMatch(/not found/i);
      });
    });
  });

  test("returns non-zero and emits JSON exists:false when secret is missing (--json mode)", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/secrets`]: () =>
          jsonResponse(200, { secrets: [] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "show", "missing-key", "--json"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        const parsed = JSON.parse(out.read()) as { name?: string; exists?: boolean };
        expect(parsed.name).toBe("missing-key");
        expect(parsed.exists).toBe(false);
      });
    });
  });

  test("show with null created_at omits the dim created_at line (human mode)", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/secrets`]: () =>
          jsonResponse(200, { secrets: [{ name: "bare", created_at: null }] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "show", "bare"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toMatch(/exists/i);
        expect(text).not.toContain("created_at:");
      });
    });
  });
});

// ---------------------------------------------------------------------------
// secret delete
// ---------------------------------------------------------------------------

describe("secret delete", () => {
  test("DELETEs the named secret and confirms removal (human mode)", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`DELETE /v1/workspaces/${WS_ID}/secrets/github`]: () =>
          jsonResponse(204, {}),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "delete", "github"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toMatch(/removed/i);
        expect(text).toContain("github");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `DELETE /v1/workspaces/${WS_ID}/secrets/github`,
        ]);
      });
    });
  });

  test("DELETEs and prints JSON status when --json flag is passed", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`DELETE /v1/workspaces/${WS_ID}/secrets/slack`]: () =>
          jsonResponse(204, {}),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "delete", "slack", "--json"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const parsed = JSON.parse(out.read()) as { status?: string; name?: string };
        expect(parsed.status).toBe("deleted");
        expect(parsed.name).toBe("slack");
      });
    });
  });

  test("returns non-zero when API returns 404 for delete (no route registered)", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "delete", "no-such"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
      });
    });
  });
});

// ---------------------------------------------------------------------------
// secret list — --json mode, empty-vault hint, human row rendering
// ---------------------------------------------------------------------------

describe("secret list extra branches", () => {
  test("emits raw JSON response body when --json flag is passed", async () => {
    await authedScope(async () => {
      const payload = {
        secrets: [
          { name: "github", created_at: 1700000000000 },
          { name: "slack", created_at: 1700000000001 },
        ],
      };
      await withMockApi(
        { [`GET /v1/workspaces/${WS_ID}/secrets`]: () => jsonResponse(200, payload) },
        async (apiUrl) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(
            ["secret", "list", "--json"],
            { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
          );
          expect(code).toBe(0);
          const parsed = JSON.parse(out.read()) as typeof payload;
          expect(parsed.secrets).toHaveLength(2);
          expect(parsed.secrets[0]?.name).toBe("github");
        },
      );
    });
  });

  test("prints empty-vault hint when no secrets exist (human mode)", async () => {
    await authedScope(async () => {
      await withMockApi(
        { [`GET /v1/workspaces/${WS_ID}/secrets`]: () => jsonResponse(200, { secrets: [] }) },
        async (apiUrl) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(
            ["secret", "list"],
            { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
          );
          expect(code).toBe(0);
          expect(out.read()).toMatch(/no secrets/i);
        },
      );
    });
  });

  test("prints each secret row name when list is non-empty (human mode)", async () => {
    await authedScope(async () => {
      await withMockApi(
        {
          [`GET /v1/workspaces/${WS_ID}/secrets`]: () =>
            jsonResponse(200, {
              secrets: [
                { name: "alpha", created_at: 1700000000000 },
                { name: "beta", created_at: null },
              ],
            }),
        },
        async (apiUrl) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(
            ["secret", "list"],
            { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
          );
          expect(code).toBe(0);
          const text = out.read();
          expect(text).toContain("alpha");
          expect(text).toContain("beta");
        },
      );
    });
  });
});

// ---------------------------------------------------------------------------
// secret add — human-mode already-exists skip (--json variant in errors
// unit test; this covers the else branch at lines 167-169)
// ---------------------------------------------------------------------------

describe("secret add already-exists human mode", () => {
  test("prints human-mode skip message when secret already exists (no --json)", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/secrets`]: () =>
          jsonResponse(200, { secrets: [{ name: "existing", created_at: 1700000000000 }] }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "add", "existing", `--data={"token":"x"}`],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toMatch(/already exists/i);
        expect(text).toMatch(/--force/i);
        expect(calls.filter((c) => c.method === "POST")).toHaveLength(0);
      });
    });
  });
});
