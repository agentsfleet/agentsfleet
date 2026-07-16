import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

const fleet = { id: "zom_1", name: "platform-ops", status: "active", created_at: 0, updated_at: 0 };

describe("listFleets", () => {
  it("GET /v1/workspaces/:ws/fleets with bearer, returns envelope", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [fleet], total: 1, next_cursor: null }) });
    const { listFleets } = await import("./fleets");
    const res = await listFleets("ws_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets"),
      expect.objectContaining({ method: "GET", headers: expect.objectContaining({ Authorization: "Bearer tok" }) }),
    );
    expect(res.items[0]?.id).toBe("zom_1");
  });

  it("throws ApiError on 401", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 401, json: async () => ({ detail: "unauthorized", error_code: "UZ-AUTH-001" }) });
    const { listFleets } = await import("./fleets");
    await expect(listFleets("ws_1", "bad")).rejects.toBeInstanceOf(ApiError);
  });

  it("appends cursor + limit query params when paginating", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [], total: 0, next_cursor: null }) });
    const { listFleets } = await import("./fleets");
    await listFleets("ws_1", "tok", { cursor: "cur_2", limit: 10 });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("cursor=cur_2");
    expect(url).toContain("limit=10");
  });
});

const detail = {
  id: "zom_1",
  name: "platform-ops",
  status: "active",
  source_markdown: "# SKILL",
  trigger_markdown: null,
  bundle_content_hash: null,
  triggers: null,
  events_processed: 3,
  budget_used_nanos: 4000,
  created_at: 0,
  updated_at: 0,
};

// A Headers double so the client can read the ETag off the response.
function headers(map: Record<string, string>): Headers {
  return { get: (k: string) => map[k.toLowerCase()] ?? null } as unknown as Headers;
}

describe("getFleet", () => {
  it("hits GET /fleets/{id} and returns the fleet plus its ETag", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      headers: headers({ etag: '"abc123"' }),
      json: async () => detail,
    });
    const { getFleet } = await import("./fleets");
    const { fleet: got, etag } = await getFleet("ws_1", "zom_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_1"),
      expect.objectContaining({ method: "GET" }),
    );
    expect(got.id).toBe("zom_1");
    expect(got.events_processed).toBe(3);
    expect(etag).toBe('"abc123"');
  });

  it("propagates a 404 as ApiError (missing or cross-workspace)", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 404,
      headers: headers({}),
      json: async () => ({ error_code: "UZ-AGT-009", detail: "Fleet not found" }),
    });
    const { getFleet } = await import("./fleets");
    const err = (await getFleet("ws_1", "missing", "tok").catch((e) => e)) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(404);
  });

  it("rejects a successful detail response with a missing or empty ETag", async () => {
    const { FLEET_ETAG_REQUIRED, getFleet } = await import("./fleets");
    for (const responseEtag of [undefined, "", "   "]) {
      fetchMock.mockResolvedValue({
        ok: true,
        status: 200,
        headers: headers(responseEtag === undefined ? {} : { etag: responseEtag }),
        json: async () => detail,
      });
      await expect(getFleet("ws_1", "zom_1", "tok")).rejects.toThrow(FLEET_ETAG_REQUIRED);
    }
  });
});

describe("saveFleetSource", () => {
  it("sends If-Match and returns the fresh etag", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      headers: headers({ etag: '"new"' }),
      json: async () => ({ fleet_id: "zom_1", config_revision: 5, etag: '"new"' }),
    });
    const { saveFleetSource } = await import("./fleets");
    const res = await saveFleetSource("ws_1", "zom_1", { source_markdown: "# edited" }, '"old"', "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_1"),
      expect.objectContaining({
        method: "PATCH",
        headers: expect.objectContaining({ "If-Match": '"old"' }),
      }),
    );
    expect(res.etag).toBe('"new"');
    expect(res.config_revision).toBe(5);
  });

  it("a stale If-Match throws ApiError 412 carrying the current etag", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 412,
      headers: headers({ etag: '"current"' }),
      json: async () => ({ error_code: "UZ-AGT-014", detail: "stale", etag: '"current"' }),
    });
    const { saveFleetSource } = await import("./fleets");
    const err = (await saveFleetSource("ws_1", "zom_1", { source_markdown: "x" }, '"stale"', "tok").catch((e) => e)) as ApiError;
    expect(err.status).toBe(412);
    expect(err.code).toBe("UZ-AGT-014");
    expect(err.etag).toBe('"current"');
  });

  it("rejects a successful save response with a missing or empty ETag", async () => {
    const { FLEET_ETAG_REQUIRED, saveFleetSource } = await import("./fleets");
    for (const responseEtag of [undefined, "", "   "]) {
      fetchMock.mockResolvedValue({
        ok: true,
        status: 200,
        headers: headers({}),
        json: async () => ({ fleet_id: "zom_1", config_revision: 5, etag: responseEtag }),
      });
      await expect(
        saveFleetSource("ws_1", "zom_1", { source_markdown: "# edited" }, '"old"', "tok"),
      ).rejects.toThrow(FLEET_ETAG_REQUIRED);
    }
  });
});

describe("setFleetStatus", () => {
  it("PATCH /v1/workspaces/:ws/fleets/:id with body {status:'stopped'} returns updated fleet", async () => {
    const stopped = { ...fleet, status: "stopped" };
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => stopped });
    const { stopFleet } = await import("./fleets");
    const result = await stopFleet("ws_1", "zom_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_1"),
      expect.objectContaining({
        method: "PATCH",
        body: JSON.stringify({ status: "stopped" }),
      }),
    );
    expect(result.status).toBe("stopped");
  });

  it("resumeFleet sends body {status:'active'}", async () => {
    const active = { ...fleet, status: "active" };
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => active });
    const { resumeFleet } = await import("./fleets");
    await resumeFleet("ws_1", "zom_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ method: "PATCH", body: JSON.stringify({ status: "active" }) }),
    );
  });

  it("killFleet sends body {status:'killed'}", async () => {
    const killed = { ...fleet, status: "killed" };
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => killed });
    const { killFleet } = await import("./fleets");
    await killFleet("ws_1", "zom_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ method: "PATCH", body: JSON.stringify({ status: "killed" }) }),
    );
  });

  it("throws ApiError UZ-AGT-010 on 409 (transition not allowed)", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 409, json: async () => ({ detail: "transition not allowed", error_code: "UZ-AGT-010" }) });
    const { stopFleet } = await import("./fleets");
    const err = await stopFleet("ws_1", "zom_1", "tok").catch((e) => e) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(409);
    expect(err.code).toBe("UZ-AGT-010");
  });
});

describe("deleteFleet", () => {
  it("DELETE /v1/workspaces/:ws/fleets/:id returns undefined on 204", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204, json: vi.fn() });
    const { deleteFleet } = await import("./fleets");
    const result = await deleteFleet("ws_1", "zom_1", "tok");
    expect(result).toBeUndefined();
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_1"),
      expect.objectContaining({ method: "DELETE" }),
    );
  });

  it("throws ApiError on 404", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 404, json: async () => ({ detail: "not found", error_code: "UZ-AGT-009" }) });
    const { deleteFleet } = await import("./fleets");
    await expect(deleteFleet("ws_1", "zom_1", "tok")).rejects.toBeInstanceOf(ApiError);
  });
});

describe("steerFleet", () => {
  it("POSTs {message} to /v1/workspaces/:ws/fleets/:id/messages and returns event_id", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 202,
      json: async () => ({ event_id: "evt_steer_1" }),
    });
    const { steerFleet } = await import("./fleets");
    const result = await steerFleet("ws_1", "zom_1", "howdy", "tok");
    expect(result).toEqual({ event_id: "evt_steer_1" });
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_1/messages"),
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ message: "howdy" }),
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
    const err = await steerFleet("ws_1", "zom_1", "", "tok").catch((e) => e) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(400);
  });
});
