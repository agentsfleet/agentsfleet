// Integration tests for zombie.ts status / stop / resume / kill / delete
// handlers. Each test exercises one CLI invocation against a mock API server
// and asserts the exit code, output text, and API calls recorded.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-000000b00b00";
const AGENTSFLEET_ID = "01900000-0000-7000-8000-000000c0ffee";

const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_zombie_test" }, fn);

function errorEnvelope(code: string, message: string, requestId = "req_zombie_test") {
  return { error: { code, message }, request_id: requestId };
}

// ---------------------------------------------------------------------------
// status (top-level `status` command)
// ---------------------------------------------------------------------------

describe("status", () => {
  test("populated list prints name, status, events, and budget columns", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/zombies`]: () =>
          jsonResponse(200, {
            items: [
              { name: "my-bot", status: "active", events_processed: 42, budget_used_dollars: 1.23 },
              { name: "idle-bot", status: "stopped", events_processed: 0, budget_used_dollars: null },
            ],
          }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const code = await runCli(
          ["status"],
          { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("my-bot");
        expect(text).toContain("$1.23");
        expect(text).toContain("idle-bot");
        expect(text).toContain("—");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([`GET /v1/workspaces/${WS_ID}/zombies`]);
      });
    });
  });

  test("empty items array prints the no-zombies hint", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/zombies`]: () => jsonResponse(200, { items: [] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const code = await runCli(
          ["status"],
          { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toContain("No zombies running");
      });
    });
  });

  test("missing items field treated as empty list", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/zombies`]: () => jsonResponse(200, {}),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const code = await runCli(
          ["status"],
          { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toContain("No zombies running");
      });
    });
  });

  test("--json emits the raw API JSON envelope", async () => {
    await authedScope(async () => {
      const payload = { items: [{ name: "bot-a", status: "active", events_processed: 7 }] };
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/zombies`]: () => jsonResponse(200, payload),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const code = await runCli(
          ["status", "--json"],
          { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const parsed = JSON.parse(out.read()) as typeof payload;
        expect(parsed.items?.[0]?.name).toBe("bot-a");
      });
    });
  });

  test("API 5xx surfaces error code and exits non-zero", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/zombies`]: () =>
          jsonResponse(503, errorEnvelope("UZ-INTERNAL-001", "Database unavailable")),
      };
      await withMockApi(routes, async (apiUrl) => {
        const err = bufferStream();
        const code = await runCli(
          ["status"],
          { stdout: bufferStream().stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        expect(err.read()).toContain("UZ-INTERNAL-001");
      });
    });
  });

});

// ---------------------------------------------------------------------------
// stop (top-level `stop <zombie_id>` command)
// ---------------------------------------------------------------------------

describe("stop", () => {
  test("PATCHes status=stopped and prints confirmation", async () => {
    await authedScope(async () => {
      let patchBody: string | null = null;
      const routes: MockRoutes = {
        [`PATCH /v1/workspaces/${WS_ID}/zombies/${AGENTSFLEET_ID}`]: async (_req, _url, body) => {
          patchBody = body;
          return jsonResponse(200, { zombie_id: AGENTSFLEET_ID, status: "stopped" });
        },
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const code = await runCli(
          ["stop", AGENTSFLEET_ID],
          { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toContain("stopped");
        expect((JSON.parse(patchBody ?? "{}") as { status?: string }).status).toBe("stopped");
      });
    });
  });

  test("--json emits the PATCH response as JSON", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`PATCH /v1/workspaces/${WS_ID}/zombies/${AGENTSFLEET_ID}`]: () =>
          jsonResponse(200, { zombie_id: AGENTSFLEET_ID, status: "stopped" }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const code = await runCli(
          ["stop", AGENTSFLEET_ID, "--json"],
          { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect((JSON.parse(out.read()) as { status?: string }).status).toBe("stopped");
      });
    });
  });

  test("API 4xx exits non-zero and surfaces the error code", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`PATCH /v1/workspaces/${WS_ID}/zombies/${AGENTSFLEET_ID}`]: () =>
          jsonResponse(404, errorEnvelope("UZ-ZMB-001", "Zombie not found")),
      };
      await withMockApi(routes, async (apiUrl) => {
        const err = bufferStream();
        const code = await runCli(
          ["stop", AGENTSFLEET_ID],
          { stdout: bufferStream().stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        expect(err.read()).toContain("UZ-ZMB-001");
      });
    });
  });
});

// ---------------------------------------------------------------------------
// resume (top-level `resume <zombie_id>` command)
// ---------------------------------------------------------------------------

describe("resume", () => {
  test("PATCHes status=active and prints confirmation", async () => {
    await authedScope(async () => {
      let patchBody: string | null = null;
      const routes: MockRoutes = {
        [`PATCH /v1/workspaces/${WS_ID}/zombies/${AGENTSFLEET_ID}`]: async (_req, _url, body) => {
          patchBody = body;
          return jsonResponse(200, { zombie_id: AGENTSFLEET_ID, status: "active" });
        },
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const code = await runCli(
          ["resume", AGENTSFLEET_ID],
          { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toContain("resumed");
        expect((JSON.parse(patchBody ?? "{}") as { status?: string }).status).toBe("active");
      });
    });
  });

  test("--json emits the PATCH response as JSON", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`PATCH /v1/workspaces/${WS_ID}/zombies/${AGENTSFLEET_ID}`]: () =>
          jsonResponse(200, { zombie_id: AGENTSFLEET_ID, status: "active" }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const code = await runCli(
          ["resume", AGENTSFLEET_ID, "--json"],
          { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect((JSON.parse(out.read()) as { status?: string }).status).toBe("active");
      });
    });
  });
});

// ---------------------------------------------------------------------------
// kill (top-level `kill <zombie_id>` command)
// ---------------------------------------------------------------------------

describe("kill", () => {
  test("PATCHes status=killed and prints confirmation", async () => {
    await authedScope(async () => {
      let patchBody: string | null = null;
      const routes: MockRoutes = {
        [`PATCH /v1/workspaces/${WS_ID}/zombies/${AGENTSFLEET_ID}`]: async (_req, _url, body) => {
          patchBody = body;
          return jsonResponse(200, { zombie_id: AGENTSFLEET_ID, status: "killed" });
        },
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const code = await runCli(
          ["kill", AGENTSFLEET_ID],
          { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toContain("killed");
        expect((JSON.parse(patchBody ?? "{}") as { status?: string }).status).toBe("killed");
      });
    });
  });

  test("--json emits the PATCH response as JSON", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`PATCH /v1/workspaces/${WS_ID}/zombies/${AGENTSFLEET_ID}`]: () =>
          jsonResponse(200, { zombie_id: AGENTSFLEET_ID, status: "killed" }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const code = await runCli(
          ["kill", AGENTSFLEET_ID, "--json"],
          { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect((JSON.parse(out.read()) as { status?: string }).status).toBe("killed");
      });
    });
  });
});

// ---------------------------------------------------------------------------
// delete (top-level `delete <zombie_id>` command)
// ---------------------------------------------------------------------------

describe("delete", () => {
  test("DELETEs the zombie and prints confirmation", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`DELETE /v1/workspaces/${WS_ID}/zombies/${AGENTSFLEET_ID}`]: () => jsonResponse(204, {}),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const code = await runCli(
          ["delete", AGENTSFLEET_ID],
          { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toContain("deleted");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `DELETE /v1/workspaces/${WS_ID}/zombies/${AGENTSFLEET_ID}`,
        ]);
      });
    });
  });

  test("--json prints JSON with zombie_id and deleted:true", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`DELETE /v1/workspaces/${WS_ID}/zombies/${AGENTSFLEET_ID}`]: () => jsonResponse(204, {}),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const code = await runCli(
          ["delete", AGENTSFLEET_ID, "--json"],
          { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const parsed = JSON.parse(out.read()) as { zombie_id?: string; deleted?: boolean };
        expect(parsed.zombie_id).toBe(AGENTSFLEET_ID);
        expect(parsed.deleted).toBe(true);
      });
    });
  });

  test("API 4xx exits non-zero and surfaces the error code", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`DELETE /v1/workspaces/${WS_ID}/zombies/${AGENTSFLEET_ID}`]: () =>
          jsonResponse(404, errorEnvelope("UZ-ZMB-001", "Zombie not found")),
      };
      await withMockApi(routes, async (apiUrl) => {
        const err = bufferStream();
        const code = await runCli(
          ["delete", AGENTSFLEET_ID],
          { stdout: bufferStream().stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        expect(err.read()).toContain("UZ-ZMB-001");
      });
    });
  });
});
