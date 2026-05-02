import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

describe("getTenantProvider", () => {
  it("GET /v1/tenants/me/provider with bearer, returns resolved config", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        mode: "platform",
        provider: "fireworks",
        model: "kimi-k2.6",
        context_cap_tokens: 256000,
        credential_ref: null,
        synthesised_default: true,
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
    expect(res.mode).toBe("platform");
    expect(res.synthesised_default).toBe(true);
  });
});

describe("setTenantProviderByok", () => {
  it("PUTs mode=byok with credential_ref + optional model", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        mode: "byok",
        provider: "fireworks",
        model: "kimi-k2.6",
        context_cap_tokens: 256000,
        credential_ref: "fw-byok",
      }),
    });
    const { setTenantProviderByok } = await import("./tenant_provider");
    await setTenantProviderByok({ credential_ref: "fw-byok", model: "kimi-k2.6" }, "tok");
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [, init] = fetchMock.mock.calls[0]!;
    expect(init).toMatchObject({ method: "PUT" });
    const body = JSON.parse((init as { body: string }).body);
    expect(body).toEqual({ mode: "byok", credential_ref: "fw-byok", model: "kimi-k2.6" });
  });

  it("omits model when not provided so backend uses vault default", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        mode: "byok",
        provider: "fireworks",
        model: "kimi-k2.6",
        context_cap_tokens: 256000,
        credential_ref: "fw-byok",
      }),
    });
    const { setTenantProviderByok } = await import("./tenant_provider");
    await setTenantProviderByok({ credential_ref: "fw-byok" }, "tok");
    const [, init] = fetchMock.mock.calls[0]!;
    const body = JSON.parse((init as { body: string }).body);
    expect(body.model).toBeUndefined();
  });

  it("throws ApiError on 400 credential_data_malformed", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 400,
      json: async () => ({ error: "credential body missing required fields", code: "credential_data_malformed" }),
    });
    const { setTenantProviderByok } = await import("./tenant_provider");
    await expect(setTenantProviderByok({ credential_ref: "junk" }, "tok"))
      .rejects.toBeInstanceOf(ApiError);
  });
});

describe("resetTenantProvider", () => {
  it("DELETEs /v1/tenants/me/provider and returns the platform default", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        mode: "platform",
        provider: "fireworks",
        model: "kimi-k2.6",
        context_cap_tokens: 256000,
        credential_ref: null,
      }),
    });
    const { resetTenantProvider } = await import("./tenant_provider");
    const res = await resetTenantProvider("tok");
    const [, init] = fetchMock.mock.calls[0]!;
    expect(init).toMatchObject({ method: "DELETE" });
    expect(res.mode).toBe("platform");
  });
});
