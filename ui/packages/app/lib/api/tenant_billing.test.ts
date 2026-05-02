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
        plan_tier: "free", plan_sku: "starter", balance_cents: 471,
        updated_at: 1, is_exhausted: false, exhausted_at: null,
      }),
    });
    const { getTenantBilling } = await import("./tenant_billing");
    const res = await getTenantBilling("tok");
    expect(res.balance_cents).toBe(471);
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
      ok: true, status: 200, json: async () => ({ items: [] }),
    });
    const { listTenantBillingCharges } = await import("./tenant_billing");
    await listTenantBillingCharges("tok", 10);
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/tenants/me/billing/charges?limit=10"),
      expect.any(Object),
    );
  });
});
