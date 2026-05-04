import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

const zombie = { id: "zom_1", name: "platform-ops", status: "active", created_at: 0, updated_at: 0 };

describe("listZombies", () => {
  it("GET /v1/workspaces/:ws/zombies with bearer, returns envelope", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [zombie], total: 1, next_cursor: null }) });
    const { listZombies } = await import("./zombies");
    const res = await listZombies("ws_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/zombies"),
      expect.objectContaining({ method: "GET", headers: expect.objectContaining({ Authorization: "Bearer tok" }) }),
    );
    expect(res.items[0]?.id).toBe("zom_1");
  });

  it("throws ApiError on 401", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 401, json: async () => ({ error: "unauthorized", code: "UZ-AUTH-001" }) });
    const { listZombies } = await import("./zombies");
    await expect(listZombies("ws_1", "bad")).rejects.toBeInstanceOf(ApiError);
  });
});

describe("getZombie", () => {
  it("returns zombie matching id from list", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [zombie], total: 1, next_cursor: null }) });
    const { getZombie } = await import("./zombies");
    const result = await getZombie("ws_1", "zom_1", "tok");
    expect(result?.id).toBe("zom_1");
  });

  it("returns null when id not found in list", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [zombie], total: 1, next_cursor: null }) });
    const { getZombie } = await import("./zombies");
    const result = await getZombie("ws_1", "missing", "tok");
    expect(result).toBeNull();
  });
});

describe("setZombieStatus", () => {
  it("PATCH /v1/workspaces/:ws/zombies/:id with body {status:'stopped'} returns updated zombie", async () => {
    const stopped = { ...zombie, status: "stopped" };
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => stopped });
    const { stopZombie } = await import("./zombies");
    const result = await stopZombie("ws_1", "zom_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/zombies/zom_1"),
      expect.objectContaining({
        method: "PATCH",
        body: JSON.stringify({ status: "stopped" }),
      }),
    );
    expect(result.status).toBe("stopped");
  });

  it("resumeZombie sends body {status:'active'}", async () => {
    const active = { ...zombie, status: "active" };
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => active });
    const { resumeZombie } = await import("./zombies");
    await resumeZombie("ws_1", "zom_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ method: "PATCH", body: JSON.stringify({ status: "active" }) }),
    );
  });

  it("killZombie sends body {status:'killed'}", async () => {
    const killed = { ...zombie, status: "killed" };
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => killed });
    const { killZombie } = await import("./zombies");
    await killZombie("ws_1", "zom_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ method: "PATCH", body: JSON.stringify({ status: "killed" }) }),
    );
  });

  it("throws ApiError UZ-ZMB-010 on 409 (transition not allowed)", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 409, json: async () => ({ error: "transition not allowed", code: "UZ-ZMB-010" }) });
    const { stopZombie } = await import("./zombies");
    const err = await stopZombie("ws_1", "zom_1", "tok").catch((e) => e) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(409);
    expect(err.code).toBe("UZ-ZMB-010");
  });
});

describe("deleteZombie", () => {
  it("DELETE /v1/workspaces/:ws/zombies/:id returns undefined on 204", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204, json: vi.fn() });
    const { deleteZombie } = await import("./zombies");
    const result = await deleteZombie("ws_1", "zom_1", "tok");
    expect(result).toBeUndefined();
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/zombies/zom_1"),
      expect.objectContaining({ method: "DELETE" }),
    );
  });

  it("throws ApiError on 404", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 404, json: async () => ({ error: "not found", code: "UZ-ZMB-009" }) });
    const { deleteZombie } = await import("./zombies");
    await expect(deleteZombie("ws_1", "zom_1", "tok")).rejects.toBeInstanceOf(ApiError);
  });
});
