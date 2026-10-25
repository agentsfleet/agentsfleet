import { afterEach, describe, expect, it, vi } from "vitest";
import { getBillingReport, listBillingActivityCharges } from "./billing_report";
import { ApiError } from "./errors";
import { BILLING_AT, BILLING_FLEET, BILLING_MODEL, billingCharge, billingReport } from "@/tests/helpers/billing-report";

const fetchMock = vi.fn();
afterEach(() => { vi.unstubAllGlobals(); fetchMock.mockReset(); });

function respond(body: unknown, status = 200) {
  vi.stubGlobal("fetch", fetchMock.mockImplementation(async () => Response.json(body, { status })));
}

describe("validated settled billing reads", () => {
  it("sends every exact filter and opaque continuation through the authenticated read", async () => {
    respond(billingReport());
    const report = await getBillingReport("fixture-token", { limit: 25, period: "today", fleet_id: BILLING_FLEET,
      activity_type: "runtime", model: BILLING_MODEL, through_at: BILLING_AT, starting_after: "opaque+/=" });
    // pin test: literal is the contract
    expect(report.totals.charged_nanos).toBe("100000");
    const [input, init] = fetchMock.mock.calls[0] ?? [];
    const url = new URL(String(input), "https://example.test");
    expect(url.pathname).toBe("/backend/v1/tenants/me/billing/reports/current");
    expect(Object.fromEntries(url.searchParams)).toEqual({ limit: "25", period: "today", fleet_id: BILLING_FLEET,
      activity_type: "runtime", model: BILLING_MODEL, through_at: BILLING_AT, starting_after: "opaque+/=" });
    expect(init).toMatchObject({ method: "GET", headers: { Authorization: "Bearer fixture-token" } });
  });

  it("encodes a work identifier and keeps tiny amounts as integer strings", async () => {
    respond({ items: [billingCharge({ charged_nanos: "1" })], total: null, next_cursor: "next" });
    const page = await listBillingActivityCharges("fixture-token", "opaque/id", { limit: 25, activity_type: "setup" });
    expect(page.items[0]?.charged_nanos).toBe("1");
    expect(page.next_cursor).toBe("next");
    expect(String(fetchMock.mock.calls[0]?.[0])).toContain("activities/opaque%2Fid/charges");
  });

  it.each([1, null, "-1", "01", "9223372036854775808"])("rejects an invalid served money value %j", async charged_nanos => {
    respond({ ...billingReport(), totals: { ...billingReport().totals, charged_nanos } });
    await expect(getBillingReport("fixture-token", { limit: 25 })).rejects.toThrow();
  });

  it.each([null, { period: "year" }, { activities: { items: [] } }, { filters: { fleets: [], models: 3 } }])("rejects incomplete report shape %j", async body => {
    respond(body);
    await expect(getBillingReport("fixture-token", { limit: 25 })).rejects.toThrow();
  });

  it("rejects malformed usage rather than presenting it as a free charge", async () => {
    respond({ items: [{ ...billingCharge(), input_tokens: null }], total: null, next_cursor: null });
    await expect(listBillingActivityCharges("fixture-token", "work", { limit: 25 })).rejects.toThrow();
  });

  it("propagates a permission refusal without replacing it with an empty report", async () => {
    respond({ detail: "Forbidden", error_code: "UZ-AUTH-002" }, 403);
    await expect(getBillingReport("fixture-token", { limit: 25 })).rejects.toBeInstanceOf(ApiError);
  });
});
