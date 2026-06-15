import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-0000005e4e71";
const AGENTSFLEET_ID = "01900000-0000-7000-8000-0000005e4e72";
const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_events" }, fn);

describe("events — happy path (human output)", () => {
  test("`events <agent_id>` prints section header + one row per event with all status variants", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/events`]:
          () => jsonResponse(200, {
            items: [
              { created_at: 1700000000000, actor: "user",  status: "processed",   response_text: "Hello." },
              { created_at: 1700000060000, actor: "agent", status: "agent_error", response_text: "Boom." },
              { created_at: 1700000120000, actor: "gate",  status: "gate_blocked", response_text: null },
              { created_at: 1700000180000, actor: "sys",   status: "pending_custom", response_text: "" },
            ],
            next_cursor: null,
          }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["events", AGENTSFLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toMatch(/events/i);
        expect(text).toContain("user");
        expect(text).toContain("agent");
        expect(text).toContain("gate");
        expect(text).toContain("processed");
        expect(text).toContain("agent_error");
        expect(text).toContain("gate_blocked");
        // unknown status passes through as dimmed text (not dropped)
        expect(text).toContain("pending_custom");
        expect(text).toContain("Hello.");
        expect(text).toContain("Boom.");
        // ISO-8601 timestamp rendered from epoch ms
        expect(text).toMatch(/\d{4}-\d{2}-\d{2}T/);
        // default limit on the wire
        expect(calls[0]?.search).toContain("limit=50");
      });
    });
  });

  test("`events <agent_id>` with next_cursor prints pagination hint", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/events`]:
          () => jsonResponse(200, {
            items: [{ created_at: 1700000000000, actor: "user", status: "processed", response_text: "ok" }],
            next_cursor: "cur_abc123",
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["events", AGENTSFLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("--cursor=cur_abc123");
        expect(text).toContain(AGENTSFLEET_ID);
      });
    });
  });

  test("`events <agent_id>` with empty items prints no-events message", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/events`]:
          () => jsonResponse(200, { items: [], next_cursor: null }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["events", AGENTSFLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/no events yet/i);
        expect(calls[0]?.path).toBe(`/v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/events`);
      });
    });
  });

  test("missing items field (undefined envelope) is treated as empty", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/events`]:
          () => jsonResponse(200, {}),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["events", AGENTSFLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/no events yet/i);
      });
    });
  });

  test("null created_at and null actor both render as em-dash", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/events`]:
          () => jsonResponse(200, {
            items: [{ created_at: null, actor: null, status: null, response_text: "msg" }],
            next_cursor: null,
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["events", AGENTSFLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        // null created_at + null actor + null status all render "—"
        const text = out.read();
        expect(text).toContain("—");
      });
    });
  });

  test("long response_text is truncated to 80-char preview with ellipsis", async () => {
    await authedScope(async () => {
      const longText = "A".repeat(200);
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/events`]:
          () => jsonResponse(200, {
            items: [{ created_at: 1700000000000, actor: "user", status: "processed", response_text: longText }],
            next_cursor: null,
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["events", AGENTSFLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).not.toContain(longText);
        expect(text).toContain("…");
      });
    });
  });
});

describe("events — query parameter forwarding", () => {
  test("--actor, --since, --cursor, --limit all forwarded as query params", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/events`]:
          () => jsonResponse(200, { items: [], next_cursor: null }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        await runCli(
          ["events", AGENTSFLEET_ID, "--actor", "user", "--since", "2h", "--cursor", "tok_xyz", "--limit", "10"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        const search = calls[0]?.search ?? "";
        expect(search).toContain("actor=user");
        expect(search).toContain("since=2h");
        expect(search).toContain("cursor=tok_xyz");
        expect(search).toContain("limit=10");
        expect(search).not.toContain("limit=50");
      });
    });
  });

  test("omitting optional flags keeps default limit=50 and no extra params", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/events`]:
          () => jsonResponse(200, { items: [], next_cursor: null }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        await runCli(
          ["events", AGENTSFLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        const search = calls[0]?.search ?? "";
        expect(search).toContain("limit=50");
        expect(search).not.toContain("actor=");
        expect(search).not.toContain("since=");
        expect(search).not.toContain("cursor=");
      });
    });
  });
});

describe("events — JSON output", () => {
  test("`events <agent_id> --json` emits raw API envelope to stdout", async () => {
    await authedScope(async () => {
      const envelope = {
        items: [{ created_at: 1700000000000, actor: "user", status: "processed", response_text: "hi" }],
        next_cursor: "c2",
      };
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/events`]:
          () => jsonResponse(200, envelope),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["events", AGENTSFLEET_ID, "--json"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const parsed = JSON.parse(out.read()) as typeof envelope;
        expect(parsed.items).toHaveLength(1);
        expect(parsed.next_cursor).toBe("c2");
      });
    });
  });

  test("global `--json` flag also triggers JSON output mode", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/events`]:
          () => jsonResponse(200, { items: [{ actor: "agent", status: "processed" }], next_cursor: null }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["--json", "events", AGENTSFLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const parsed = JSON.parse(out.read()) as { items: unknown[]; next_cursor: string | null };
        expect(parsed.items).toBeDefined();
      });
    });
  });
});

describe("events — API error paths", () => {
  test("API 401 (expired token) surfaces error code on stderr and exits non-zero", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/events`]:
          () => jsonResponse(401, {
            error: { code: "UZ-AUTH-003", message: "Token expired" },
            request_id: "req_ev_401",
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["events", AGENTSFLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        expect(err.read()).toContain("UZ-AUTH-003");
      });
    });
  });

  test("API 500 surfaces error code on stderr and exits non-zero", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/events`]:
          () => jsonResponse(500, {
            error: { code: "UZ-INTERNAL-001", message: "Database unavailable" },
            request_id: "req_ev_500",
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["events", AGENTSFLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        expect(err.read()).toContain("UZ-INTERNAL-001");
      });
    });
  });
});
