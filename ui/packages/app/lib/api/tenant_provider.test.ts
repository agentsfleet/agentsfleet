import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";
import { PROVIDER_MODE } from "../types";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

describe("getTenantProvider", () => {
  it("GET /v1/tenants/me/provider with bearer, returns resolved config", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        mode: PROVIDER_MODE.platform,
        provider: "fireworks",
        model: "kimi-k2.6",
        context_cap_tokens: 256000,
        secret_ref: null,
      }),
    });
    const { getTenantProvider } = await import("./tenant_provider");
    const res = await getTenantProvider("tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/tenants/me/provider"),
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({ Authorization: "Bearer tok" }),
      }),
    );
    expect(res.mode).toBe(PROVIDER_MODE.platform);
    expect(res.provider).toBe("fireworks");
    expect(res.secret_ref).toBeNull();
  });
});

describe("setTenantProviderSelfManaged", () => {
  it("PUTs mode=self_managed with secret_ref + optional model", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        mode: PROVIDER_MODE.self_managed,
        provider: "fireworks",
        model: "kimi-k2.6",
        context_cap_tokens: 256000,
        secret_ref: "fw-key",
      }),
    });
    const { setTenantProviderSelfManaged } = await import("./tenant_provider");
    await setTenantProviderSelfManaged({ secret_ref: "fw-key", model: "kimi-k2.6" }, "tok");
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [, init] = fetchMock.mock.calls[0]!;
    expect(init).toMatchObject({ method: "PUT" });
    const body = JSON.parse((init as { body: string }).body);
    expect(body).toEqual({ mode: PROVIDER_MODE.self_managed, secret_ref: "fw-key", model: "kimi-k2.6" });
  });

  it("omits model when not provided so backend uses vault default", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        mode: PROVIDER_MODE.self_managed,
        provider: "fireworks",
        model: "kimi-k2.6",
        context_cap_tokens: 256000,
        secret_ref: "fw-key",
      }),
    });
    const { setTenantProviderSelfManaged } = await import("./tenant_provider");
    await setTenantProviderSelfManaged({ secret_ref: "fw-key" }, "tok");
    const [, init] = fetchMock.mock.calls[0]!;
    const body = JSON.parse((init as { body: string }).body);
    expect(body.model).toBeUndefined();
  });

  it("throws ApiError on 400 credential_data_malformed", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 400,
      json: async () => ({ detail: "credential body missing required fields", error_code: "UZ-PROVIDER-002" }),
    });
    const { setTenantProviderSelfManaged } = await import("./tenant_provider");
    await expect(setTenantProviderSelfManaged({ secret_ref: "junk" }, "tok"))
      .rejects.toBeInstanceOf(ApiError);
  });
});

describe("resetTenantProvider", () => {
  it("DELETEs /v1/tenants/me/provider and returns the platform default", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        mode: PROVIDER_MODE.platform,
        provider: "fireworks",
        model: "kimi-k2.6",
        context_cap_tokens: 256000,
        secret_ref: null,
      }),
    });
    const { resetTenantProvider } = await import("./tenant_provider");
    const res = await resetTenantProvider("tok");
    const [, init] = fetchMock.mock.calls[0]!;
    expect(init).toMatchObject({ method: "DELETE" });
    expect(res.mode).toBe(PROVIDER_MODE.platform);
  });
});
