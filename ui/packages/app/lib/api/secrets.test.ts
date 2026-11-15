import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

describe("listSecrets", () => {
  it("GET /v1/workspaces/:ws/secrets with bearer, returns envelope", async () => {
    const items = [
      { name: "fly", created_at: "2026-04-26T00:00:00Z" },
      { name: "slack", created_at: "2026-04-26T00:00:01Z" },
    ];
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ secrets: items }),
    });
    const { listSecrets } = await import("./secrets");
    const res = await listSecrets("ws_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/secrets"),
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({ Authorization: "Bearer tok" }),
      }),
    );
    expect(res.secrets).toEqual(items);
  });

  it("throws ApiError on 401", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 401,
      json: async () => ({ detail: "unauthorized", error_code: "UZ-AUTH-001" }),
    });
    const { listSecrets } = await import("./secrets");
    await expect(listSecrets("ws_1", "bad")).rejects.toBeInstanceOf(ApiError);
  });
});

describe("createSecret", () => {
  it("POST with JSON body containing name + data, returns {name}", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 201,
      json: async () => ({ name: "fly" }),
    });
    const { createSecret } = await import("./secrets");
    const res = await createSecret(
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
      json: async () => ({ detail: "data must be a non-empty JSON object", error_code: "UZ-VAULT-001" }),
    });
    const { createSecret } = await import("./secrets");
    const err = await createSecret("ws_1", { name: "x", data: {} }, "tok").catch(
      (e) => e,
    ) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.code).toBe("UZ-VAULT-001");
    expect(err.status).toBe(400);
  });
});



describe("replaceSecret", () => {
  it("PUTs /secrets/:name with a {data} whole-body payload and URL-encoded name", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ name: "anthropic prod" }) });
    const { replaceSecret } = await import("./secrets");
    const body = { provider: "anthropic", api_key: "sk-ant-replaced" };
    const res = await replaceSecret("ws_1", "anthropic prod", body, "tok");
    expect(res.name).toBe("anthropic prod");
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(url).toContain("/v1/workspaces/ws_1/secrets/anthropic%20prod");
    expect(init.method).toBe("PUT");
    expect(JSON.parse(init.body as string)).toEqual({ data: body });
    expect((init.headers as Record<string, string>).Authorization).toBe("Bearer tok");
  });

  it("propagates a typed 404 when the workspace does not hold the name", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 404,
      json: async () => ({ detail: "not found", error_code: "UZ-VAULT-404" }),
    });
    const { replaceSecret } = await import("./secrets");
    const err = (await replaceSecret("ws_1", "ghost", { api_key: "sk-x" }, "tok").catch((e: unknown) => e)) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(404);
    expect(err.code).toBe("UZ-VAULT-404");
  });
});

describe("deleteSecret", () => {
  it("DELETE /v1/workspaces/:ws/secrets/:name with URL-encoded name", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204, json: async () => undefined });
    const { deleteSecret } = await import("./secrets");
    await deleteSecret("ws_1", "name with space", "tok");
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(url).toContain("/v1/workspaces/ws_1/secrets/name%20with%20space");
    expect(init.method).toBe("DELETE");
  });

  it("returns undefined on 204 (idempotent)", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204, json: async () => undefined });
    const { deleteSecret } = await import("./secrets");
    const res = await deleteSecret("ws_1", "fly", "tok");
    expect(res).toBeUndefined();
  });

  it("throws ApiError on 403", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 403,
      json: async () => ({ detail: "forbidden", error_code: "UZ-AUTH-003" }),
    });
    const { deleteSecret } = await import("./secrets");
    await expect(deleteSecret("ws_1", "fly", "tok")).rejects.toBeInstanceOf(ApiError);
  });
});
