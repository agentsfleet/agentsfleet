import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.js";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.js";
import { withMockApi, jsonResponse } from "./helpers-mock-api.js";

const WS_ID = "ws_logs_test";
const ZOMBIE_ID = "zmb_logs_test";
const authedScope = (fn) => withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_logs" }, fn);

describe("logs (paginated event tail)", () => {
  test("`logs <zombie_id>` with no events prints the empty-state message and exits 0", async () => {
    await authedScope(async () => {
      const routes = {
        [`GET /v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/events`]:
          () => jsonResponse(200, { items: [], next_cursor: null }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["logs", ZOMBIE_ID],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/no events yet/i);
        expect(calls).toHaveLength(1);
        expect(calls[0].method).toBe("GET");
        expect(calls[0].path).toBe(`/v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/events`);
        // The default limit=20 query is preserved on the wire.
        expect(calls[0].search).toContain("limit=20");
      });
    });
  });

  test("`logs <zombie_id>` with events prints one row per event with timestamp + actor + summary", async () => {
    await authedScope(async () => {
      const routes = {
        [`GET /v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/events`]:
          () => jsonResponse(200, {
            items: [
              { created_at: 1700000000000, actor: "user",   status: "processed", response_text: "Hello, world." },
              { created_at: 1700000060000, actor: "agent",  status: "processed", response_text: "Acknowledged. Working on it." },
              { created_at: 1700000120000, actor: "system", status: "gate_blocked", response_text: null },
            ],
            next_cursor: "cur_next_page",
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["logs", ZOMBIE_ID],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        // Every actor and summary appears.
        expect(text).toContain("user");
        expect(text).toContain("agent");
        expect(text).toContain("system");
        expect(text).toContain("Hello, world.");
        expect(text).toContain("Acknowledged");
        expect(text).toContain("gate_blocked");
        // ISO-8601 timestamp from epoch ms is rendered (any T...Z form is fine).
        expect(text).toMatch(/\d{4}-\d{2}-\d{2}T/);
        // Pagination hint when the server returned a next cursor.
        expect(text).toContain("--cursor=cur_next_page");
      });
    });
  });

  test("`logs` with no zombie_id exits 2 with a missing-argument error", async () => {
    await authedScope(async () => {
      // No mock routes — the CLI's argument validation must fire before any
      // outbound fetch, otherwise the test traps an unexpected request.
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["logs"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(2);
        expect(err.read()).toMatch(/zombie/i);
        expect(calls).toHaveLength(0);
      });
    });
  });
});
