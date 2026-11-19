// The built `agentsfleet fleet steer @name` against the mock API: the name is
// looked up, then the message goes to that fleet's messages route. A value the
// address rule refuses stops at the parser, so no request leaves the process.
import { expect, test } from "bun:test";
import { composeEnv, runFleetctl } from "./acceptance/fixtures/cli.js";
import { jsonResponse, withMockApi, type MockCall } from "./helpers-mock-api.ts";
import { authedScope, EVENT_ID, FLEET_ID, POST, WS_ID } from "./helpers-fleet-steer.ts";

const FLEETS = `/v1/workspaces/${WS_ID}/fleets`;
const VALIDATION_EXIT_CODE = 4;
const ROW = {
  id: FLEET_ID, name: "agentbob", description: null, status: "active", created_at: "2026-11-19T00:00:00Z",
  events_processed: "0", budget_used_nanos: "0",
};
const COMPLETE_FRAME = `event: event_complete\ndata: ${JSON.stringify({ event_id: EVENT_ID, status: "processed" })}\n\n`;

const requested = (calls: ReadonlyArray<MockCall>): ReadonlyArray<string> =>
  calls.map((call) => `${call.method} ${call.path}${call.search}`);

test("fleet_steer_binary_reaches_a_fleet_by_name", async () => {
  await authedScope((stateDir) => withMockApi({
    [`GET ${FLEETS}`]: (_req, url) =>
      jsonResponse(200, { items: url.searchParams.get("name") === ROW.name ? [ROW] : [], total: null, next_cursor: null }),
    [`GET ${FLEETS}/${FLEET_ID}/events/stream`]: () =>
      new Response(COMPLETE_FRAME, { headers: { "content-type": "text/event-stream" } }),
    [`POST ${FLEETS}/${FLEET_ID}/messages`]: () => jsonResponse(202, { event_id: EVENT_ID, disposition: "started" }),
  }, async (apiUrl, calls) => {
    const env = composeEnv({ AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_STATE_DIR: stateDir, NO_COLOR: "1" });

    for (const value of ["@agent_bob", "@", `@${"a".repeat(65)}`]) {
      const refused = await runFleetctl(["fleet", "steer", value, "retry", "--json"], { env });
      expect({ value, code: refused.code, error: JSON.parse(refused.stderr).error.code }).toEqual({
        value, code: VALIDATION_EXIT_CODE, error: "INVALID_ARGUMENT",
      });
    }
    expect(requested(calls)).toEqual([]);

    const sent = await runFleetctl(["fleet", "steer", "@agentbob", "retry", "--json"], { env });
    expect(sent.code).toBe(0);
    expect(requested(calls)).toEqual([
      `GET ${FLEETS}?name=agentbob`,
      `GET ${FLEETS}/${FLEET_ID}/events/stream`,
      `POST ${FLEETS}/${FLEET_ID}/messages`,
    ]);
    expect(calls.filter((call) => call.method === POST).map((call) => JSON.parse(call.body ?? "null")))
      .toEqual([{ message: "retry" }]);
    expect(JSON.parse(sent.stdout)).toEqual({
      event_id: EVENT_ID, disposition: "started", kind: "complete", status: "processed",
    });
  }));
});
