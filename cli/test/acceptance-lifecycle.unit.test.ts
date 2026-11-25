// Exercise the acceptance reader through the built terminal client and real HTTP.
import { expect, test } from "bun:test";
import { authedScope, FLEET_ID, WS_ID, withMockApi, jsonResponse } from "./helpers-fleet-install.ts";
import { composeEnv, runFleetctl } from "./acceptance/fixtures/cli.js";
import { getStatus } from "./acceptance/fixtures/lifecycle.ts";

const FLEET = {
  id: FLEET_ID, name: "counter-probe", status: "active", etag: '"resource"',
  model_ref: "owner/runtime", http_egress: "restricted", package_domains: false, schedules: [], triggers: [], grants: [],
};
const COUNTERS = {events_processed: "9007199254740993", budget_used_nanos: "9223372036854775807"};

async function read(counters: Record<string, unknown>) {
  return authedScope(async (stateDir) => withMockApi({
    [`GET /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}`]: () => jsonResponse(200, {...FLEET, ...counters}),
  }, async (apiUrl, calls) => {
    const fleet = await getStatus(composeEnv({AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_STATE_DIR: stateDir}), FLEET_ID);
    expect(calls).toHaveLength(1);
    return fleet;
  }));
}

test("the acceptance reader preserves counters larger than the safe integer limit", async () => {
  expect(await read(COUNTERS)).toMatchObject(COUNTERS);
});

for (const field of Object.keys(COUNTERS)) {
  for (const value of [undefined, 0, "-1", "1.5"]) {
    test(`the acceptance reader refuses ${field}=${String(value)}`, async () => {
      await expect(read({...COUNTERS, [field]: value})).rejects.toThrow();
    });
  }
}

test("fleet create with no message makes no API request", async () => {
  await authedScope(async (stateDir) => withMockApi({}, async (apiUrl, calls) => {
    const result = await runFleetctl(["--json", "fleet", "create"], {
      env: composeEnv({AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_STATE_DIR: stateDir}),
    });
    expect(result.code).toBe(4);
    expect(JSON.parse(result.stderr).error).toMatchObject({code: "INVALID_ARGUMENT", message: "message is required"});
    expect(result.stdout.trim()).toBe("");
    expect(calls).toEqual([]);
  }));
});

test("the built binary refuses the removed chat and names fleet create and fleet steer", async () => {
  await authedScope(async (stateDir) => withMockApi({}, async (apiUrl, calls) => {
    const result = await runFleetctl(["chat", "--save"], {
      env: composeEnv({AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_STATE_DIR: stateDir, NO_COLOR: "1"}),
    });
    expect(result.code).toBe(4);
    expect(result.stderr).toContain("the chat command was removed");
    expect(result.stderr).toContain('agentsfleet fleet create "<message>"');
    expect(result.stderr).toContain("agentsfleet fleet steer <fleet_id|@name>");
    expect(calls).toEqual([]);
  }));
});
