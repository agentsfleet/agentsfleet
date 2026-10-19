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
        balance_nanos: "4710000000", updated_at: "1970-01-01T00:00:00.001Z", is_exhausted: false, exhausted_at: null,
      }),
    });
    const { getTenantBilling } = await import("./tenant_billing");
    const res = await getTenantBilling("tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringMatching(/\/v1\/tenants\/me\/billing$/),
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({ Authorization: "Bearer tok" }),
      }),
    );
    expect(res.balance_nanos).toBe("4710000000");
  });

  it("throws ApiError on 500", async () => {
    fetchMock.mockResolvedValue({
      ok: false, status: 500,
      json: async () => ({ detail: "internal", error_code: "UZ-INTERNAL-003" }),
    });
    const { getTenantBilling } = await import("./tenant_billing");
    await expect(getTenantBilling("tok")).rejects.toBeInstanceOf(ApiError);
  });

  it.each([12, null, "01", "-0", "-01", "+1", "9223372036854775808", "-9223372036854775809"])("refuses malformed balance %j", async balance_nanos => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({
      balance_nanos, updated_at: "2026-10-18T12:00:00Z", is_exhausted: false, exhausted_at: null,
    }) });
    const { getTenantBilling } = await import("./tenant_billing");
    await expect(getTenantBilling("tok")).rejects.toThrow();
  });

  it.each(["-50", "-50000000", "-9223372036854775808"])("retains exact debt %s", async balance_nanos => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({
      balance_nanos, updated_at: "2026-10-18T12:00:00Z", is_exhausted: true, exhausted_at: null,
    }) });
    const { getTenantBilling } = await import("./tenant_billing");
    expect(await getTenantBilling("tok")).toMatchObject({ balance_nanos, is_exhausted: true });
  });

  it("retains the largest supported balance without converting it to a number", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({
      balance_nanos: "9223372036854775807", updated_at: "2026-10-18T12:00:00Z", is_exhausted: false, exhausted_at: null,
    }) });
    const { getTenantBilling } = await import("./tenant_billing");
    expect((await getTenantBilling("tok")).balance_nanos).toBe("9223372036854775807");
  });
});

describe("listTenantBillingCharges", () => {
  it("GET /v1/tenants/me/billing/charges?limit=50 by default", async () => {
    fetchMock.mockResolvedValue({
      ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }),
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
