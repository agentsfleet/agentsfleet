// @vitest-environment node
import { afterEach, describe, expect, it, vi } from "vitest";
import { FLEET_DETAIL, FLEET_ID, GRANT_ID, MODEL_REF, OPERATION_ID, SCHEDULE, TRIGGER_ID, TURN_ID, WORKSPACE, response } from "@/tests/helpers/fleet-fixtures";
import { changeFleet, createFleet, forgetPinnedMemory, readGrants, readPinnedMemory, readSchedules, readTriggers, removeTrigger, revokeGrant } from "./fleet-client";

const BASE = `/live/v1/workspaces/${WORKSPACE}/fleets`;
const signal = () => new AbortController().signal;
const OTHER_FLEET = "0199a801-1000-7000-8000-0000000000ff";
type Call = {path: string; init: RequestInit};
const sent = (call: Call | undefined): unknown => typeof call?.init.body === "string" ? JSON.parse(call.init.body) : undefined;
function stub(...answers: Response[]): Call[] {
  const calls: Call[] = [];
  const queue = [...answers];
  vi.stubGlobal("fetch", vi.fn(async (path: string, init: RequestInit) => {calls.push({path, init}); return queue.shift() ?? response({}, 500);}));
  return calls;
}
const memory = (key: string, category: string, writer = FLEET_ID) => ({key, content: `${key} — 東京 🦉`, category, updated_at: "2026-11-24T10:00:00Z", visibility: "fleet", writer_fleet_id: writer});
afterEach(() => vi.unstubAllGlobals());

describe("creating a fleet from its first message", () => {
  it("posts the message, its operation and the chosen model, and answers with the fleet and its turn", async () => {
    const calls = stub(response({fleet: FLEET_DETAIL, accepted: {turn_id: TURN_ID, disposition: "queued"}, replayed: false}, 201));
    const body = {message: "Watch checkout errors", operation_id: OPERATION_ID, model_ref: MODEL_REF};
    expect(await createFleet(WORKSPACE, body, signal())).toEqual({fleetId: FLEET_ID, turnId: TURN_ID, replayed: false});
    expect(calls[0]?.path).toBe(BASE);
    expect(calls[0]?.init).toMatchObject({method: "POST", credentials: "same-origin"});
    expect(sent(calls[0])).toEqual(body);
  });
  it("reads a repeated operation as the same fleet, whatever else the fleet carries", async () => {
    stub(response({fleet: {id: FLEET_ID, unknown_field: true}, accepted: {turn_id: TURN_ID, disposition: "started"}, replayed: true}));
    expect(await createFleet(WORKSPACE, {message: "again", operation_id: OPERATION_ID}, signal())).toMatchObject({replayed: true});
  });
  it.each([
    [response({user_message: "No model is available to this workspace.", error_code: "UZ-SETUP-008"}, 409), {status: 409, code: "UZ-SETUP-008"}],
    [response({fleet: {id: "not-an-id"}, accepted: {turn_id: TURN_ID}, replayed: false}, 201), {status: 502}],
  ])("refuses a create the daemon refused or answered without a fleet", async (answer, failure) => {
    stub(answer);
    await expect(createFleet(WORKSPACE, {message: "x", operation_id: OPERATION_ID}, signal())).rejects.toMatchObject(failure);
  });
});

describe("changing one field of a fleet", () => {
  it.each([{model_ref: MODEL_REF}, {http_egress: "public" as const}, {package_domains: true}])("sends %o with the version read", async (change) => {
    const calls = stub(response({...FLEET_DETAIL, ...change, etag: '"two"'}));
    expect((await changeFleet(WORKSPACE, FLEET_ID, '"one"', change, signal())).etag).toBe('"two"');
    expect(calls[0]?.path).toBe(`${BASE}/${FLEET_ID}`);
    expect(calls[0]?.init).toMatchObject({method: "PATCH", headers: expect.objectContaining({"If-Match": '"one"'})});
    expect(sent(calls[0])).toEqual(change);
  });
  it("leaves the field to the daemon's refusal", async () => {
    stub(response({user_message: "Only a signed-in person can open a fleet's network."}, 403));
    await expect(changeFleet(WORKSPACE, FLEET_ID, '"one"', {http_egress: "public"}, signal())).rejects.toMatchObject({status: 403});
  });
});

describe("the lists beside a fleet's chat", () => {
  it("reads its schedules, triggers and grants", async () => {
    const trigger = {id: TRIGGER_ID, kind: "mention", source: "slack", channel: "C42", created_at: "2026-11-24T10:00:00Z"};
    const grant = {id: GRANT_ID, kind: "secret", name: "sentry", status: "pending"};
    const calls = stub(response({schedules: [SCHEDULE]}), response({items: [trigger]}), response({items: [grant], total: 1}));
    expect(await readSchedules(WORKSPACE, FLEET_ID, signal())).toEqual([SCHEDULE]);
    expect(await readTriggers(WORKSPACE, FLEET_ID, signal())).toEqual([trigger]);
    expect(await readGrants(WORKSPACE, FLEET_ID, signal())).toEqual([grant]);
    expect(calls.map(call => call.path)).toEqual([`${BASE}/${FLEET_ID}/schedules`, `${BASE}/${FLEET_ID}/triggers`, `${BASE}/${FLEET_ID}/integration-grants`]);
  });
  it("walks every memory page and keeps only this fleet's own core entries", async () => {
    const calls = stub(
      response({items: [memory("evidence", "core"), memory("note", "note")], total: 4, next_cursor: "note"}),
      response({items: [memory("shared", "core", OTHER_FLEET), memory("links", "core")], total: 4, next_cursor: null}),
    );
    expect((await readPinnedMemory(WORKSPACE, FLEET_ID, signal())).map(entry => entry.key)).toEqual(["evidence", "links"]);
    expect(calls.map(call => call.path)).toEqual([`${BASE}/${FLEET_ID}/memories?limit=100`, `${BASE}/${FLEET_ID}/memories?limit=100&starting_after=note`]);
  });
  it("refuses a memory list that repeats its cursor", async () => {
    stub(response({items: [], total: 0, next_cursor: "same"}), response({items: [], total: 0, next_cursor: "same"}));
    await expect(readPinnedMemory(WORKSPACE, FLEET_ID, signal())).rejects.toThrow("repeated page cursor");
  });
  it("removes a trigger, revokes a grant and forgets a pinned entry by its encoded key", async () => {
    const calls = stub(response(null, 204), response(null, 204), response(null, 204));
    await removeTrigger(WORKSPACE, FLEET_ID, TRIGGER_ID, signal());
    await revokeGrant(WORKSPACE, FLEET_ID, GRANT_ID, signal());
    await forgetPinnedMemory(WORKSPACE, FLEET_ID, "links/evidence ✓", signal());
    expect(calls.map(call => [call.init.method, call.path])).toEqual([
      ["DELETE", `${BASE}/${FLEET_ID}/triggers/${TRIGGER_ID}`],
      ["DELETE", `${BASE}/${FLEET_ID}/integration-grants/${GRANT_ID}`],
      ["DELETE", `${BASE}/${FLEET_ID}/memories/${encodeURIComponent("links/evidence ✓")}`],
    ]);
  });
  it("keeps a removal the daemon refused a refusal", async () => {
    stub(response({user_message: "Only a signed-in person can remove access."}, 403));
    await expect(revokeGrant(WORKSPACE, FLEET_ID, GRANT_ID, signal())).rejects.toMatchObject({status: 403, message: "Only a signed-in person can remove access."});
  });
});
