import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

describe("getTenantBilling", () => {
  it("GET /v1/tenants/me/billing with bearer", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        balance_nanos: 4_710_000_000, updated_at: 1, is_exhausted: false, exhausted_at: null,
      }),
    });
    const { getTenantBilling } = await import("./tenant_billing");
    const res = await getTenantBilling("tok");
    expect(res.balance_nanos).toBe(4_710_000_000);
  });

  it("throws ApiError on 500", async () => {
    fetchMock.mockResolvedValue({
      ok: false, status: 500,
      json: async () => ({ error: "internal", code: "ERR_INTERNAL_OPERATION_FAILED" }),
    });
    const { getTenantBilling } = await import("./tenant_billing");
    await expect(getTenantBilling("tok")).rejects.toBeInstanceOf(ApiError);
  });
});

describe("listTenantBillingCharges", () => {
  it("GET /v1/tenants/me/billing/charges?limit=50 by default", async () => {
    fetchMock.mockResolvedValue({
      ok: true, status: 200, json: async () => ({ items: [] }),
    });
    const { listTenantBillingCharges } = await import("./tenant_billing");
    await listTenantBillingCharges("tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/tenants/me/billing/charges?limit=50"),
      expect.objectContaining({ method: "GET" }),
    );
  });

  it("passes through custom limit", async () => {
    fetchMock.mockResolvedValue({
      ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }),
    });
    const { listTenantBillingCharges } = await import("./tenant_billing");
    await listTenantBillingCharges("tok", { limit: 10 });
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/tenants/me/billing/charges?limit=10"),
      expect.any(Object),
    );
  });

  it("URI-encodes the cursor token in the query string", async () => {
    fetchMock.mockResolvedValue({
      ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }),
    });
    const { listTenantBillingCharges } = await import("./tenant_billing");
    await listTenantBillingCharges("tok", { cursor: "tok+with/special=chars" });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("cursor=tok%2Bwith%2Fspecial%3Dchars");
  });

  it("omits cursor when null/undefined (first page)", async () => {
    fetchMock.mockResolvedValue({
      ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }),
    });
    const { listTenantBillingCharges } = await import("./tenant_billing");
    await listTenantBillingCharges("tok", { cursor: null });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).not.toContain("cursor=");
  });
});
