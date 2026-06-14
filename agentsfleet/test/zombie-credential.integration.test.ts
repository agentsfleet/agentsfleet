// Integration tests for `credential show`, `credential delete`, --json modes,
// and human-mode list rows. The baseline add/list happy paths are in
// credentials.integration.test.ts. Validation error branches that cannot be
// reached through the CLI parser live in zombie-credential-errors.unit.test.ts.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "ws_cred_ext_test";
const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_cred_ext" }, fn);

// ---------------------------------------------------------------------------
// credential show
// ---------------------------------------------------------------------------

describe("credential show", () => {
  test("prints existence confirmation when credential is found (human mode)", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/credentials`]: () =>
          jsonResponse(200, { credentials: [{ name: "github", created_at: 1700000000000 }] }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "show", "github"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toMatch(/exists/i);
        expect(text).toContain("github");
        expect(text).toContain("1700000000000");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/credentials`,
        ]);
      });
    });
  });

  test("prints JSON payload when credential is found (--json mode)", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/credentials`]: () =>
          jsonResponse(200, { credentials: [{ name: "slack", created_at: 1700000001000 }] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "show", "slack", "--json"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
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

  test("returns non-zero and prints not-found message when credential is missing (human mode)", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/credentials`]: () =>
          jsonResponse(200, { credentials: [] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "show", "missing-key"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        expect(out.read() + err.read()).toMatch(/not found/i);
      });
    });
  });

  test("returns non-zero and emits JSON exists:false when credential is missing (--json mode)", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/credentials`]: () =>
          jsonResponse(200, { credentials: [] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "show", "missing-key", "--json"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
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
        [`GET /v1/workspaces/${WS_ID}/credentials`]: () =>
          jsonResponse(200, { credentials: [{ name: "bare", created_at: null }] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "show", "bare"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
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
// credential delete
// ---------------------------------------------------------------------------

describe("credential delete", () => {
  test("DELETEs the named credential and confirms removal (human mode)", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`DELETE /v1/workspaces/${WS_ID}/credentials/github`]: () =>
          jsonResponse(204, {}),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "delete", "github"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toMatch(/removed/i);
        expect(text).toContain("github");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `DELETE /v1/workspaces/${WS_ID}/credentials/github`,
        ]);
      });
    });
  });

  test("DELETEs and prints JSON status when --json flag is passed", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`DELETE /v1/workspaces/${WS_ID}/credentials/slack`]: () =>
          jsonResponse(204, {}),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "delete", "slack", "--json"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
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
          ["credential", "delete", "no-such"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
      });
    });
  });
});

// ---------------------------------------------------------------------------
// credential list — --json mode, empty-vault hint, human row rendering
// ---------------------------------------------------------------------------

describe("credential list extra branches", () => {
  test("emits raw JSON response body when --json flag is passed", async () => {
    await authedScope(async () => {
      const payload = {
        credentials: [
          { name: "github", created_at: 1700000000000 },
          { name: "slack", created_at: 1700000000001 },
        ],
      };
      await withMockApi(
        { [`GET /v1/workspaces/${WS_ID}/credentials`]: () => jsonResponse(200, payload) },
        async (apiUrl) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(
            ["credential", "list", "--json"],
            { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
          );
          expect(code).toBe(0);
          const parsed = JSON.parse(out.read()) as typeof payload;
          expect(parsed.credentials).toHaveLength(2);
          expect(parsed.credentials[0]?.name).toBe("github");
        },
      );
    });
  });

  test("prints empty-vault hint when no credentials exist (human mode)", async () => {
    await authedScope(async () => {
      await withMockApi(
        { [`GET /v1/workspaces/${WS_ID}/credentials`]: () => jsonResponse(200, { credentials: [] }) },
        async (apiUrl) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(
            ["credential", "list"],
            { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
          );
          expect(code).toBe(0);
          expect(out.read()).toMatch(/no credentials/i);
        },
      );
    });
  });

  test("prints each credential row name when list is non-empty (human mode)", async () => {
    await authedScope(async () => {
      await withMockApi(
        {
          [`GET /v1/workspaces/${WS_ID}/credentials`]: () =>
            jsonResponse(200, {
              credentials: [
                { name: "alpha", created_at: 1700000000000 },
                { name: "beta", created_at: null },
              ],
            }),
        },
        async (apiUrl) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(
            ["credential", "list"],
            { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
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
// credential add — human-mode already-exists skip (--json variant in errors
// unit test; this covers the else branch at lines 167-169)
// ---------------------------------------------------------------------------

describe("credential add already-exists human mode", () => {
  test("prints human-mode skip message when credential already exists (no --json)", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/credentials`]: () =>
          jsonResponse(200, { credentials: [{ name: "existing", created_at: 1700000000000 }] }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "add", "existing", `--data={"token":"x"}`],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
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
