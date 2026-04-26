import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

describe("listCredentials", () => {
  it("GET /v1/workspaces/:ws/credentials with bearer, returns envelope", async () => {
    const items = [
      { name: "fly", created_at: "2026-04-26T00:00:00Z" },
      { name: "slack", created_at: "2026-04-26T00:00:01Z" },
    ];
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ credentials: items }),
    });
    const { listCredentials } = await import("./credentials");
    const res = await listCredentials("ws_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/credentials"),
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({ Authorization: "Bearer tok" }),
      }),
    );
    expect(res.credentials).toEqual(items);
  });

  it("throws ApiError on 401", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 401,
      json: async () => ({ error: "unauthorized", code: "UZ-AUTH-001" }),
    });
    const { listCredentials } = await import("./credentials");
    await expect(listCredentials("ws_1", "bad")).rejects.toBeInstanceOf(ApiError);
  });
});

describe("createCredential", () => {
  it("POST with JSON body containing name + data, returns {name}", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 201,
      json: async () => ({ name: "fly" }),
    });
    const { createCredential } = await import("./credentials");
    const res = await createCredential(
      "ws_1",
      { name: "fly", data: { host: "api.machines.dev", api_token: "FLY_T" } },
      "tok",
    );
    expect(res.name).toBe("fly");
    const [, init] = fetchMock.mock.calls[0]!;
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body as string)).toEqual({
      name: "fly",
      data: { host: "api.machines.dev", api_token: "FLY_T" },
    });
  });

  it("propagates API error when server rejects shape", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 400,
      json: async () => ({ error: "data must be a non-empty JSON object", code: "UZ-VAULT-001" }),
    });
    const { createCredential } = await import("./credentials");
    const err = await createCredential("ws_1", { name: "x", data: {} }, "tok").catch(
      (e) => e,
    ) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.code).toBe("UZ-VAULT-001");
    expect(err.status).toBe(400);
  });
});

describe("deleteCredential", () => {
  it("DELETE /v1/workspaces/:ws/credentials/:name with URL-encoded name", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204, json: async () => undefined });
    const { deleteCredential } = await import("./credentials");
    await deleteCredential("ws_1", "name with space", "tok");
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(url).toContain("/v1/workspaces/ws_1/credentials/name%20with%20space");
    expect(init.method).toBe("DELETE");
  });

  it("returns undefined on 204 (idempotent)", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204, json: async () => undefined });
    const { deleteCredential } = await import("./credentials");
    const res = await deleteCredential("ws_1", "fly", "tok");
    expect(res).toBeUndefined();
  });

  it("throws ApiError on 403", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 403,
      json: async () => ({ error: "forbidden", code: "UZ-AUTH-003" }),
    });
    const { deleteCredential } = await import("./credentials");
    await expect(deleteCredential("ws_1", "fly", "tok")).rejects.toBeInstanceOf(ApiError);
  });
});
