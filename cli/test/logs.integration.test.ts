import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-00000010c105";
const FLEET_ID = "01900000-0000-7000-8000-0000007090c5";
const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_logs" }, fn);

describe("logs (paginated event tail)", () => {
  test("`logs <fleet_id>` with no events prints the empty-state message and exits 0", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/events`]:
          () => jsonResponse(200, { items: [], next_cursor: null }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["logs", FLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/no events yet/i);
        expect(calls).toHaveLength(1);
        expect(calls[0]?.method).toBe("GET");
        expect(calls[0]?.path).toBe(`/v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/events`);
        // The default limit=20 query is preserved on the wire.
        expect(calls[0]?.search).toContain("limit=20");
      });
    });
  });

  test("`logs <fleet_id>` with events prints one row per event with timestamp + actor + summary", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/events`]:
          () => jsonResponse(200, {
            items: [
              { created_at: 1700000000000, actor: "user",   status: "processed", response_text: "Hello, world." },
              { created_at: 1700000060000, actor: "fleet",  status: "processed", response_text: "Acknowledged. Working on it." },
              { created_at: 1700000120000, actor: "system", status: "gate_blocked", response_text: null },
            ],
            next_cursor: "cur_next_page",
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["logs", FLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        // Every actor and summary appears.
        expect(text).toContain("user");
        expect(text).toContain("fleet");
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

  test("`logs <fleet_id>` survives a malformed created_at — renders — and exits 0, never RangeError", async () => {
    // Regression: an unparseable created_at used to throw
    // `RangeError: Invalid time value` from formatTimestamp and crash the whole
    // command. The row must degrade to the — literal while the stream continues.
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/events`]:
          () => jsonResponse(200, {
            items: [
              { created_at: "not-a-real-date", actor: "user", status: "processed", response_text: "still rendered" },
              { created_at: 1700000000000, actor: "fleet", status: "processed", response_text: "valid row" },
            ],
            next_cursor: null,
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["logs", FLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        // The command completed cleanly instead of throwing.
        expect(code).toBe(0);
        const text = out.read();
        // The malformed row degraded to the fallback literal but still printed.
        expect(text).toContain("—");
        expect(text).toContain("still rendered");
        // The valid row's ISO timestamp is unaffected.
        expect(text).toMatch(/\d{4}-\d{2}-\d{2}T/);
        expect(text).toContain("valid row");
        // No RangeError leaked to stderr.
        expect(err.read()).not.toMatch(/RangeError|Invalid time value/);
      });
    });
  });

  test("`logs` with no fleet_id exits ValidationError (4) with a missing-argument error", async () => {
    await authedScope(async () => {
      // No mock routes — the CLI's argument validation must fire before any
      // outbound fetch, otherwise the test traps an unexpected request.
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["logs"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        // Effect-shape contract: ValidationError → exit 4 (EXIT_CODE.ValidationError).
        // The pre-Effect path returned 2 via writeError(MISSING_ARGUMENT, …); the
        // Effect dispatcher now classifies missing positionals as ValidationError.
        expect(code).toBe(4);
        expect(err.read()).toMatch(/fleet/i);
        expect(calls).toHaveLength(0);
      });
    });
  });
});
