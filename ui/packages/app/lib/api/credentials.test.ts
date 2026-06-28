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
      json: async () => ({ detail: "unauthorized", error_code: "UZ-AUTH-001" }),
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
      json: async () => ({ detail: "data must be a non-empty JSON object", error_code: "UZ-VAULT-001" }),
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

describe("listCredentials — tagged-union passthrough", () => {
  it("returns each kind variant verbatim (provider_key / custom_endpoint / custom_secret)", async () => {
    const items = [
      { kind: "provider_key", name: "anthropic", created_at: 1, provider: "anthropic", model: "claude-sonnet-4-6" },
      { kind: "custom_endpoint", name: "vllm", created_at: 2, provider: "openai-compatible", base_url: "https://vllm.corp/v1" },
      { kind: "custom_secret", name: "STRIPE", created_at: 3 },
    ];
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ credentials: items }) });
    const { listCredentials } = await import("./credentials");
    const res = await listCredentials("ws_1", "tok");
    // The client never re-derives kind — it surfaces the server projection as-is.
    expect(res.credentials).toEqual(items);
  });
});

describe("credential narrowing helpers", () => {
  const MIXED = [
    { kind: "provider_key", name: "anthropic", created_at: 1, provider: "anthropic", model: "claude-sonnet-4-6" },
    { kind: "provider_key", name: "openai", created_at: 2, provider: "openai" },
    { kind: "custom_endpoint", name: "vllm", created_at: 3, provider: "openai-compatible", base_url: "https://vllm.corp/v1" },
    { kind: "custom_secret", name: "STRIPE", created_at: 4 },
  ] as const;

  it("providerKeysOf keeps only provider_key rows", async () => {
    const { providerKeysOf } = await import("./credentials");
    const keys = providerKeysOf([...MIXED]);
    expect(keys.map((k) => k.name)).toEqual(["anthropic", "openai"]);
    expect(keys.every((k) => k.kind === "provider_key")).toBe(true);
  });

  it("customEndpointsOf keeps only custom_endpoint rows (with base_url)", async () => {
    const { customEndpointsOf } = await import("./credentials");
    const endpoints = customEndpointsOf([...MIXED]);
    expect(endpoints.map((e) => e.name)).toEqual(["vllm"]);
    expect(endpoints[0]!.base_url).toBe("https://vllm.corp/v1");
  });

  it("customSecretsOf keeps only custom_secret rows", async () => {
    const { customSecretsOf } = await import("./credentials");
    const secrets = customSecretsOf([...MIXED]);
    expect(secrets.map((s) => s.name)).toEqual(["STRIPE"]);
    expect(secrets[0]!.kind).toBe("custom_secret");
  });

  it("each helper returns an empty array when no row matches", async () => {
    const { providerKeysOf, customEndpointsOf, customSecretsOf } = await import("./credentials");
    const onlySecret = [{ kind: "custom_secret", name: "X", created_at: 1 }] as const;
    expect(providerKeysOf([...onlySecret])).toEqual([]);
    expect(customEndpointsOf([...onlySecret])).toEqual([]);
    expect(customSecretsOf([])).toEqual([]);
  });
});

describe("rotateCredential", () => {
  it("PATCHes /credentials/:name with a {api_key} body and URL-encoded name", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ name: "anthropic prod" }) });
    const { rotateCredential } = await import("./credentials");
    const res = await rotateCredential("ws_1", "anthropic prod", "sk-ant-rotated", "tok");
    expect(res.name).toBe("anthropic prod");
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(url).toContain("/v1/workspaces/ws_1/credentials/anthropic%20prod");
    expect(init.method).toBe("PATCH");
    expect(JSON.parse(init.body as string)).toEqual({ api_key: "sk-ant-rotated" });
    expect((init.headers as Record<string, string>).Authorization).toBe("Bearer tok");
  });

  it("propagates a typed 404 when the credential name is missing", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 404,
      json: async () => ({ detail: "not found", error_code: "UZ-VAULT-404" }),
    });
    const { rotateCredential } = await import("./credentials");
    const err = (await rotateCredential("ws_1", "ghost", "sk-x", "tok").catch((e) => e)) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(404);
    expect(err.code).toBe("UZ-VAULT-404");
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
      json: async () => ({ detail: "forbidden", error_code: "UZ-AUTH-003" }),
    });
    const { deleteCredential } = await import("./credentials");
    await expect(deleteCredential("ws_1", "fly", "tok")).rejects.toBeInstanceOf(ApiError);
  });
});
