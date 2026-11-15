import { afterEach, describe, expect, it, vi } from "vitest";
import { getBillingReport } from "./billing_report";
import { ApiError } from "./errors";
import { BILLING_AT, BILLING_FLEET, billingReport } from "@/tests/helpers/billing-report";

const fetchMock = vi.fn();
afterEach(() => { vi.unstubAllGlobals(); fetchMock.mockReset(); });

function respond(body: unknown, status = 200) {
  vi.stubGlobal("fetch", fetchMock.mockImplementation(async () => Response.json(body, { status })));
}

describe("validated settled billing reads", () => {
  it("sends every exact filter through the authenticated read", async () => {
    respond(billingReport());
    const report = await getBillingReport("fixture-token", { period: "today", fleet_id: BILLING_FLEET,
      activity_type: "runtime", through_at: BILLING_AT });
    // pin test: literal is the contract
    expect(report.totals.charged_nanos).toBe("100000");
    const [input, init] = fetchMock.mock.calls[0] ?? [];
    const url = new URL(String(input), "https://example.test");
    expect(url.pathname).toBe("/backend/v1/tenants/me/billing/reports/current");
    expect(Object.fromEntries(url.searchParams)).toEqual({ period: "today", fleet_id: BILLING_FLEET,
      activity_type: "runtime", through_at: BILLING_AT });
    expect(init).toMatchObject({ method: "GET", headers: { Authorization: "Bearer fixture-token" } });
  });

  it("omits an unset filter instead of sending it empty", async () => {
    respond(billingReport());
    await getBillingReport("fixture-token", { period: "month" });
    const url = new URL(String(fetchMock.mock.calls[0]?.[0]), "https://example.test");
    expect(Object.fromEntries(url.searchParams)).toEqual({ period: "month" });
  });

  it.each([1, null, "-1", "01", "9223372036854775808"])("rejects an invalid served money value %j", async charged_nanos => {
    respond({ ...billingReport(), totals: { ...billingReport().totals, charged_nanos } });
    await expect(getBillingReport("fixture-token", {})).rejects.toThrow();
  });

  it.each([null, { period: "year" }, { filters: { fleets: [], models: 3 } }])("rejects incomplete report shape %j", async body => {
    respond(body);
    await expect(getBillingReport("fixture-token", {})).rejects.toThrow();
  });

  it("accepts day and model rows that carry no activity count", async () => {
    const served = billingReport();
    expect(served.daily.every(day => !Object.hasOwn(day.totals, "activity_count"))).toBe(true);
    expect(served.spending.by_model.every(row => !Object.hasOwn(row.totals, "activity_count"))).toBe(true);
    respond(served);
    const report = await getBillingReport("fixture-token", {});
    expect(report.spending.by_model[0]?.totals.activity_count).toBeUndefined();
    expect(report.totals.activity_count).toBe(1);
  });

  it("rejects period or fleet totals that omit the activity count rather than reporting none", async () => {
    const { activity_count: _counted, ...uncounted } = billingReport().totals;
    respond({ ...billingReport(), totals: uncounted });
    await expect(getBillingReport("fixture-token", {})).rejects.toThrow(/activity_count/);
    const report = billingReport();
    respond({ ...report, spending: { ...report.spending, by_fleet: report.spending.by_fleet.map(row => ({ ...row, totals: uncounted })) } });
    await expect(getBillingReport("fixture-token", {})).rejects.toThrow(/activity_count/);
  });

  it("propagates a permission refusal without replacing it with an empty report", async () => {
    respond({ detail: "Forbidden", error_code: "UZ-AUTH-002" }, 403);
    await expect(getBillingReport("fixture-token", {})).rejects.toBeInstanceOf(ApiError);
  });

  it.each([undefined, [{day: "2026-02-30", totals: billingReport().totals}]])("rejects absent or invalid daily projections", async daily => {
    respond({...billingReport(), daily});
    await expect(getBillingReport("fixture-token", {})).rejects.toThrow(/daily/);
  });

  it("rejects a fleet projection without recorded models", async () => {
    const report = billingReport();
    respond({...report, spending: {...report.spending, by_fleet: report.spending.by_fleet.map(row => ({...row, models: undefined}))}});
    await expect(getBillingReport("fixture-token", {})).rejects.toThrow(/models/);
  });

  it("refuses more daily bars than any supported reporting period can contain", async () => {
    respond({...billingReport(), daily: Array.from({length: 32}, (_, index) => ({
      day: new Date(Date.UTC(2026, 9, index + 1)).toISOString().slice(0, 10), totals: billingReport().totals,
    }))});
    await expect(getBillingReport("fixture-token", {})).rejects.toThrow(/daily/);
  });
});
