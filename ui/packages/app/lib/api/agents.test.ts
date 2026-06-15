import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

const agent = { id: "zom_1", name: "platform-ops", status: "active", created_at: 0, updated_at: 0 };

describe("listAgents", () => {
  it("GET /v1/workspaces/:ws/agents with bearer, returns envelope", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [agent], total: 1, next_cursor: null }) });
    const { listAgents } = await import("./agents");
    const res = await listAgents("ws_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/agents"),
      expect.objectContaining({ method: "GET", headers: expect.objectContaining({ Authorization: "Bearer tok" }) }),
    );
    expect(res.items[0]?.id).toBe("zom_1");
  });

  it("throws ApiError on 401", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 401, json: async () => ({ detail: "unauthorized", error_code: "UZ-AUTH-001" }) });
    const { listAgents } = await import("./agents");
    await expect(listAgents("ws_1", "bad")).rejects.toBeInstanceOf(ApiError);
  });

  it("appends cursor + limit query params when paginating", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [], total: 0, next_cursor: null }) });
    const { listAgents } = await import("./agents");
    await listAgents("ws_1", "tok", { cursor: "cur_2", limit: 10 });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("cursor=cur_2");
    expect(url).toContain("limit=10");
  });
});

describe("getAgent", () => {
  it("returns agent matching id from list", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [agent], total: 1, next_cursor: null }) });
    const { getAgent } = await import("./agents");
    const result = await getAgent("ws_1", "zom_1", "tok");
    expect(result?.id).toBe("zom_1");
  });

  it("returns null when id not found in list", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [agent], total: 1, next_cursor: null }) });
    const { getAgent } = await import("./agents");
    const result = await getAgent("ws_1", "missing", "tok");
    expect(result).toBeNull();
  });

  it("throws ApiError UZ-AGT-SCAN-CAP (404) when id absent and cursor signals more pages exist", async () => {
    // Workspace has >100 agents — the first page doesn't contain the target
    // id but `cursor` is non-null, meaning there ARE more pages we can't scan.
    // The function must surface a distinct error rather than silently returning null.
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ items: [agent], total: 999, cursor: "cursor_abc" }),
    });
    const { getAgent } = await import("./agents");
    const err = await getAgent("ws_1", "not_in_first_page", "tok").catch((e) => e) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(404);
    expect(err.code).toBe("UZ-AGT-SCAN-CAP");
  });
});

describe("setAgentStatus", () => {
  it("PATCH /v1/workspaces/:ws/agents/:id with body {status:'stopped'} returns updated agent", async () => {
    const stopped = { ...agent, status: "stopped" };
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => stopped });
    const { stopAgent } = await import("./agents");
    const result = await stopAgent("ws_1", "zom_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/agents/zom_1"),
      expect.objectContaining({
        method: "PATCH",
        body: JSON.stringify({ status: "stopped" }),
      }),
    );
    expect(result.status).toBe("stopped");
  });

  it("resumeAgent sends body {status:'active'}", async () => {
    const active = { ...agent, status: "active" };
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => active });
    const { resumeAgent } = await import("./agents");
    await resumeAgent("ws_1", "zom_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ method: "PATCH", body: JSON.stringify({ status: "active" }) }),
    );
  });

  it("killAgent sends body {status:'killed'}", async () => {
    const killed = { ...agent, status: "killed" };
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => killed });
    const { killAgent } = await import("./agents");
    await killAgent("ws_1", "zom_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ method: "PATCH", body: JSON.stringify({ status: "killed" }) }),
    );
  });

  it("throws ApiError UZ-AGT-010 on 409 (transition not allowed)", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 409, json: async () => ({ detail: "transition not allowed", error_code: "UZ-AGT-010" }) });
    const { stopAgent } = await import("./agents");
    const err = await stopAgent("ws_1", "zom_1", "tok").catch((e) => e) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(409);
    expect(err.code).toBe("UZ-AGT-010");
  });
});

describe("deleteAgent", () => {
  it("DELETE /v1/workspaces/:ws/agents/:id returns undefined on 204", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204, json: vi.fn() });
    const { deleteAgent } = await import("./agents");
    const result = await deleteAgent("ws_1", "zom_1", "tok");
    expect(result).toBeUndefined();
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/agents/zom_1"),
      expect.objectContaining({ method: "DELETE" }),
    );
  });

  it("throws ApiError on 404", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 404, json: async () => ({ detail: "not found", error_code: "UZ-AGT-009" }) });
    const { deleteAgent } = await import("./agents");
    await expect(deleteAgent("ws_1", "zom_1", "tok")).rejects.toBeInstanceOf(ApiError);
  });
});

describe("steerAgent", () => {
  it("POSTs {message} to /v1/workspaces/:ws/agents/:id/messages and returns event_id", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 202,
      json: async () => ({ event_id: "evt_steer_1" }),
    });
    const { steerAgent } = await import("./agents");
    const result = await steerAgent("ws_1", "zom_1", "howdy", "tok");
    expect(result).toEqual({ event_id: "evt_steer_1" });
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/agents/zom_1/messages"),
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
    const { steerAgent } = await import("./agents");
    const err = await steerAgent("ws_1", "zom_1", "", "tok").catch((e) => e) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(400);
  });
});
