import { afterEach, describe, expect, it, vi } from "vitest";
import { FLEET_DETAIL, FLEET_ID, WORKSPACE } from "@/tests/helpers/draft-fixtures";
import { ApiError } from "./errors";
import { FleetListSchema } from "./fleet-detail";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

const fleet = { id: "zom_1", name: "platform-ops", description: null, status: "active", created_at: "1970-01-01T00:00:00Z", updated_at: "1970-01-01T00:00:00Z", events_processed: "0", budget_used_nanos: "0" };

describe("listFleets", () => {
  it.each([{description: null}, {description: "Watch production errors"}])("preserves the saved purpose $description", async ({description}) => {
    fetchMock.mockResolvedValue({ok: true, status: 200, json: async () => ({items: [{...fleet, description}], total: null, next_cursor: null})});
    const {listFleets} = await import("./fleets");
    expect((await listFleets("ws_1", "tok")).items[0]?.description).toBe(description);
  });

  it.each([{description: undefined}, {description: 7}, {description: {}}, {description: ""}])("refuses malformed saved purpose $description", ({description}) => {
    const parsed = FleetListSchema.safeParse({items: [{...fleet, description}], total: null, next_cursor: null});
    expect(parsed).toMatchObject({success: false, error: {issues: [expect.objectContaining({path: ["items", 0, "description"]})]}});
  });

  it("GET /v1/workspaces/:ws/fleets with bearer, returns envelope", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [fleet], total: null, next_cursor: null }) });
    const { listFleets } = await import("./fleets");
    const res = await listFleets("ws_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets"),
      expect.objectContaining({ method: "GET", headers: expect.objectContaining({ Authorization: "Bearer tok" }) }),
    );
    expect(res.items[0]?.id).toBe("zom_1");
  });

  it("decodes required decimal-string counters without rounding above 2^53", async () => {
    const exact = "9007199254740993";
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [{ ...fleet, events_processed: exact, budget_used_nanos: exact }], total: null, next_cursor: null }) });
    const { listFleets } = await import("./fleets");
    const result = await listFleets("ws_1", "tok");
    expect(result.total).toBeNull();
    expect(result.items[0]?.events_processed).toBe(9007199254740993n);
    expect(result.items[0]?.budget_used_nanos).toBe(9007199254740993n);
  });

  it.each([undefined, 9, "1.5", "-1", "1e3"])("refuses an invalid REST counter %j", async (value) => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [{ ...fleet, events_processed: value, budget_used_nanos: "0" }], total: null, next_cursor: null }) });
    const { listFleets } = await import("./fleets");
    await expect(listFleets("ws_1", "tok")).rejects.toThrow();
  });

  it("throws ApiError on 401", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 401, json: async () => ({ detail: "unauthorized", error_code: "UZ-AUTH-001" }) });
    const { listFleets } = await import("./fleets");
    await expect(listFleets("ws_1", "bad")).rejects.toBeInstanceOf(ApiError);
  });

  it("test_fleets_client_uses_guideline_cursor_names", async () => {
    // Sends the guideline request spelling, never the retired one, and reads
    // the continuation from next_cursor.
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [], total: null, next_cursor: "cur_3" }) });
    const { listFleets } = await import("./fleets");
    const res = await listFleets("ws_1", "tok", { starting_after: "cur_2", limit: 10 });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("starting_after=cur_2");
    expect(url).not.toContain("cursor=cur_2");
    expect(url).toContain("limit=10");
    expect(res.next_cursor).toBe("cur_3");
  });
});

const detail = FLEET_DETAIL;
const ETAG = detail.etag;
function answer(value: unknown = detail, etag: string = ETAG) {
  return {ok: true, status: 200, headers: new Headers({etag}), json: async () => value};
}

describe("native fleet detail", () => {
  it("keeps the body version for writes when compression weakens the response tag", async () => {
    fetchMock.mockResolvedValue(answer(detail, `W/${ETAG}`));
    const {getFleet, setFleetStatus} = await import("./fleets");
    const observed = await getFleet(WORKSPACE, FLEET_ID, "tok");
    expect(observed.etag).toBe(ETAG);
    fetchMock.mockResolvedValue(answer({...detail, status: "stopped"}));
    const stopped = await setFleetStatus(WORKSPACE, FLEET_ID, "stopped", observed.etag, "tok");
    expect(stopped.status).toBe("stopped");
    expect(fetchMock.mock.calls.at(-1)?.[1]?.headers).toMatchObject({"If-Match": ETAG});
  });
  it("preserves exact nanos and independent permissions and derives triggers from native setup", async () => {
    fetchMock.mockResolvedValue(answer());
    const {getFleet} = await import("./fleets");
    const result = await getFleet(WORKSPACE, FLEET_ID, "tok");
    expect(result.etag).toBe(ETAG);
    expect(result.fleet.budget_used_nanos).toBe("9007199254740993");
    expect(result.fleet.pending_approvals).toBe(2);
    expect(result.fleet.memory_access).toEqual({read: true, publish: false});
    expect(result.fleet.triggers).toEqual([]);
    expect(result.fleet.definition.config).toEqual(detail.definition.config);
  });
  it("decodes an omitted trigger list as chat and steer only", async () => {
    const config = {...detail.definition.config, "x-agentsfleet": {tools: []}};
    fetchMock.mockResolvedValue(answer({...detail, definition: {...detail.definition, config}}));
    const {getFleet} = await import("./fleets");
    expect((await getFleet(WORKSPACE, FLEET_ID, "tok")).fleet.triggers).toEqual([]);
  });
  it("rejects the deleted trigger variant in a server projection", async () => {
    const config = {...detail.definition.config, "x-agentsfleet": {triggers: [{type: "api"}]}};
    fetchMock.mockResolvedValue(answer({...detail, definition: {...detail.definition, config}}));
    const {getFleet} = await import("./fleets");
    await expect(getFleet(WORKSPACE, FLEET_ID, "tok")).rejects.toThrow();
  });
  it.each(["", "   "])("refuses an empty ETag %j", async (etag) => {
    fetchMock.mockResolvedValue(answer(detail, etag));
    const {getFleet, FLEET_ETAG_REQUIRED} = await import("./fleets");
    await expect(getFleet(WORKSPACE, FLEET_ID, "tok")).rejects.toThrow(FLEET_ETAG_REQUIRED);
  });
  it.each(['\"other\"', 'W/\"other\"', `W/W/${ETAG}`])("refuses mismatched body and header versions %s", async (etag) => {
    fetchMock.mockResolvedValue(answer(detail, etag));
    const {getFleet} = await import("./fleets");
    await expect(getFleet(WORKSPACE, FLEET_ID, "tok")).rejects.toThrow("disagree");
  });
  it("does not promote a weak resource tag into a conditional-write version", async () => {
    const etag = `W/${ETAG}`;
    fetchMock.mockResolvedValue(answer({...detail, etag}, etag));
    const {getFleet} = await import("./fleets");
    await expect(getFleet(WORKSPACE, FLEET_ID, "tok")).rejects.toThrow("disagree");
  });
  it.each([{...detail, definition: undefined}, {...detail, budget_used_nanos: Number.MAX_SAFE_INTEGER + 1},
    {...detail, definition: {...detail.definition, config: {}}}])("refuses malformed native projections", async (invalid) => {
    fetchMock.mockResolvedValue(answer(invalid));
    const {getFleet} = await import("./fleets");
    await expect(getFleet(WORKSPACE, FLEET_ID, "tok")).rejects.toThrow();
  });
  it("encodes caller-controlled segments", async () => {
    fetchMock.mockResolvedValue(answer());
    const {getFleet} = await import("./fleets");
    await getFleet("ws/1", "fleet?outside", "tok");
    expect(fetchMock).toHaveBeenCalledWith(expect.stringContaining("/ws%2F1/fleets/fleet%3Foutside"), expect.anything());
  });
  it("preserves a private or foreign resource refusal", async () => {
    fetchMock.mockResolvedValue({ok: false, status: 404, json: async () => ({detail: "unavailable"})});
    const {getFleet} = await import("./fleets");
    await expect(getFleet(WORKSPACE, FLEET_ID, "tok")).rejects.toMatchObject({status: 404});
  });
});

describe("conditional fleet lifecycle", () => {
  it.each(["active", "stopped", "killed"] as const)("sends the displayed resource tag for %s", async (status) => {
    fetchMock.mockResolvedValue(answer({...detail, status}));
    const {setFleetStatus} = await import("./fleets");
    const result = await setFleetStatus(WORKSPACE, FLEET_ID, status, ETAG, "tok");
    expect(fetchMock).toHaveBeenCalledWith(expect.stringContaining(`/fleets/${FLEET_ID}`), expect.objectContaining({
      method: "PATCH", headers: expect.objectContaining({"If-Match": ETAG}), body: JSON.stringify({status}),
    }));
    expect(result.status).toBe(status);
    expect(result.setup_revision).toBe(detail.setup_revision);
  });
  it("requires a resource tag before sending", async () => {
    const {setFleetStatus} = await import("./fleets");
    await expect(setFleetStatus(WORKSPACE, FLEET_ID, "active", "", "tok")).rejects.toThrow("ETag");
    expect(fetchMock).not.toHaveBeenCalled();
  });
  it.each([409, 412])("preserves %i without rebasing and replaying a lifecycle change", async (status) => {
    fetchMock.mockResolvedValue({ok: false, status, json: async () => ({detail: "changed", error_code: "UZ-AGT-014"})});
    const {setFleetStatus} = await import("./fleets");
    await expect(setFleetStatus(WORKSPACE, FLEET_ID, "stopped", ETAG, "tok")).rejects.toMatchObject({status});
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});

describe("deleteFleet", () => {
  it("DELETE /v1/workspaces/:ws/fleets/:id returns undefined on 204", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204, json: vi.fn() });
    const { deleteFleet } = await import("./fleets");
    const result = await deleteFleet("ws_1", "zom_1", ETAG, "tok");
    expect(result).toBeUndefined();
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_1"),
      expect.objectContaining({ method: "DELETE", headers: expect.objectContaining({"If-Match": ETAG}) }),
    );
  });

  it("throws ApiError on 404", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 404, json: async () => ({ detail: "not found", error_code: "UZ-AGT-009" }) });
    const { deleteFleet } = await import("./fleets");
    await expect(deleteFleet("ws_1", "zom_1", ETAG, "tok")).rejects.toBeInstanceOf(ApiError);
  });
});

describe("steerFleet", () => {
  it("POSTs {message, operation_id} to /v1/workspaces/:ws/fleets/:id/messages and returns event_id", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 202,
      json: async () => ({ event_id: "evt_steer_1" }),
    });
    const { steerFleet } = await import("./fleets");
    const request = { message: "howdy", operation_id: "op-howdy-1" };
    const result = await steerFleet("ws_1", "zom_1", request, "tok");
    expect(result).toEqual({ event_id: "evt_steer_1" });
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_1/messages"),
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify(request),
      }),
    );
  });

  it("throws ApiError on 4xx", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 400,
      json: async () => ({ detail: "empty message", error_code: "UZ-AGT-020" }),
    });
    const { steerFleet } = await import("./fleets");
    const err = await steerFleet("ws_1", "zom_1", { message: "", operation_id: "op-empty" }, "tok").catch((e) => e) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(400);
  });
});
