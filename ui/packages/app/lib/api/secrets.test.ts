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

describe("listSecrets — tagged-union passthrough", () => {
  it("returns each kind variant verbatim (provider_key / custom_endpoint / custom_secret)", async () => {
    const items = [
      { kind: "provider_key", name: "anthropic", created_at: 1, provider: "anthropic", model: "claude-sonnet-4-6" },
      { kind: "custom_endpoint", name: "vllm", created_at: 2, provider: "openai-compatible", base_url: "https://vllm.corp/v1" },
      { kind: "custom_secret", name: "STRIPE", created_at: 3 },
    ];
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ secrets: items }) });
    const { listSecrets } = await import("./secrets");
    const res = await listSecrets("ws_1", "tok");
    // The client never re-derives kind — it surfaces the server projection as-is.
    expect(res.secrets).toEqual(items);
  });
});

describe("secret narrowing helpers", () => {
  const MIXED = [
    { kind: "provider_key", name: "anthropic", created_at: 1, provider: "anthropic", model: "claude-sonnet-4-6" },
    { kind: "provider_key", name: "openai", created_at: 2, provider: "openai" },
    { kind: "custom_endpoint", name: "vllm", created_at: 3, provider: "openai-compatible", base_url: "https://vllm.corp/v1" },
    { kind: "custom_secret", name: "STRIPE", created_at: 4 },
  ] as const;

  it("providerKeysOf keeps only provider_key rows", async () => {
    const { providerKeysOf } = await import("./secrets");
    const keys = providerKeysOf([...MIXED]);
    expect(keys.map((k) => k.name)).toEqual(["anthropic", "openai"]);
    expect(keys.every((k) => k.kind === "provider_key")).toBe(true);
  });

  it("customEndpointsOf keeps only custom_endpoint rows (with base_url)", async () => {
    const { customEndpointsOf } = await import("./secrets");
    const endpoints = customEndpointsOf([...MIXED]);
    expect(endpoints.map((e) => e.name)).toEqual(["vllm"]);
    expect(endpoints[0]!.base_url).toBe("https://vllm.corp/v1");
  });

  it("customSecretsOf keeps only custom_secret rows", async () => {
    const { customSecretsOf } = await import("./secrets");
    const secrets = customSecretsOf([...MIXED]);
    expect(secrets.map((s) => s.name)).toEqual(["STRIPE"]);
    expect(secrets[0]!.kind).toBe("custom_secret");
  });

  it("each helper returns an empty array when no row matches", async () => {
    const { providerKeysOf, customEndpointsOf, customSecretsOf } = await import("./secrets");
    const onlySecret = [{ kind: "custom_secret", name: "X", created_at: 1 }] as const;
    expect(providerKeysOf([...onlySecret])).toEqual([]);
    expect(customEndpointsOf([...onlySecret])).toEqual([]);
    expect(customSecretsOf([])).toEqual([]);
  });
});

describe("rotateSecret", () => {
  it("PATCHes /secrets/:name with a {api_key} body and URL-encoded name", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ name: "anthropic prod" }) });
    const { rotateSecret } = await import("./secrets");
    const res = await rotateSecret("ws_1", "anthropic prod", "sk-ant-rotated", "tok");
    expect(res.name).toBe("anthropic prod");
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(url).toContain("/v1/workspaces/ws_1/secrets/anthropic%20prod");
    expect(init.method).toBe("PATCH");
    expect(JSON.parse(init.body as string)).toEqual({ api_key: "sk-ant-rotated" });
    expect((init.headers as Record<string, string>).Authorization).toBe("Bearer tok");
  });

  it("propagates a typed 404 when the secret name is missing", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 404,
      json: async () => ({ detail: "not found", error_code: "UZ-VAULT-404" }),
    });
    const { rotateSecret } = await import("./secrets");
    const err = (await rotateSecret("ws_1", "ghost", "sk-x", "tok").catch((e) => e)) as ApiError;
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
