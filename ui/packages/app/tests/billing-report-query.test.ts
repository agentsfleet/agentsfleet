import {describe, expect, it} from "vitest";
import {BILLING_ALL, BILLING_QUERY, BILLING_SETUP, changedFilter, first, reportOptions, selectedBillingMonth} from "@/app/(dashboard)/settings/billing/lib/report-query";
import {reportingMonths} from "@/app/(dashboard)/settings/billing/lib/report-months";

describe("billing report navigation", () => {
  it("requests the current month without removed filters or pagination", () => {
    expect(reportOptions({})).toEqual({period: "month", fleet_id: undefined, activity_type: undefined, through_at: undefined});
    expect(reportOptions({model: "old", limit: "50", c: "old", ps: "50", activity: "old", dc: "old"})).toEqual(reportOptions({}));
    expect(first(["first", "second"])).toBe("first");
    expect(first(undefined)).toBeUndefined();
  });
  it.each([BILLING_ALL, BILLING_SETUP, "fleet-id"])("maps fleet choice %s without widening explicit input", fleet => {
    expect(reportOptions({fleet, through_at: "cutoff", period: "today"})).toEqual({period: "today", through_at: "cutoff",
      fleet_id: fleet === BILLING_ALL || fleet === BILLING_SETUP ? undefined : fleet,
      activity_type: fleet === BILLING_SETUP ? "setup" : undefined});
  });
  it("passes explicit invalid filters to server validation", () => {
    expect(reportOptions({period: "year", fleet: "bad-id"})).toMatchObject({period: "year", fleet_id: "bad-id"});
  });
  it("keeps the cutoff and clears obsolete URL state on a fleet change", () => {
    const original = new URLSearchParams("period=month&fleet=old&model=old&c=cursor&cps=50&ps=50&through_at=cutoff&activity=work&dc=detail");
    const next = changedFilter(original, BILLING_QUERY.fleet, "new-fleet");
    expect(Object.fromEntries(next)).toEqual({period: "month", fleet: "new-fleet", through_at: "cutoff"});
    expect(changedFilter(original, BILLING_QUERY.period, "today").has("through_at")).toBe(false);
    expect(changedFilter(original, BILLING_QUERY.fleet, BILLING_ALL).has("fleet")).toBe(false);
    expect(original.get("c")).toBe("cursor");
  });
  it.each([["2026-11-01T00:00:00Z", "2026-10", "2026-10-31T23:59:59.999Z"],
    ["2026-01-01T00:00:00Z", "2025-12", "2025-12-31T23:59:59.999Z"],
    ["2024-03-01T00:00:00Z", "2024-02", "2024-02-29T23:59:59.999Z"]])(
    "selects the prior calendar month from %s using the API cutoff", (from, expected, cutoff) => {
      const params = new URLSearchParams("period=today&fleet=deleted-fleet&model=old&c=old&through_at=old&activity=old&dc=old");
      const month = reportingMonths(new Date(from), from)[1];
      expect(month?.value).toBe(expected);
      expect(month?.through).toBe(cutoff);
      expect(Object.fromEntries(selectedBillingMonth(params, month?.through))).toEqual({period: "month", fleet: "deleted-fleet", through_at: cutoff});
      expect(params.get("period")).toBe("today");
    });
  it("keeps older history reachable with bounded choices and current month without a cutoff", () => {
    const months = reportingMonths(new Date("2026-11-15T12:00:00Z"), "2024-02-01T00:00:00Z");
    expect(months[0]).toMatchObject({value: "2026-11"});
    expect(months[0]?.through).toBeUndefined();
    expect(months.at(-2)?.value).toBe("2024-02");
    expect(months.at(-1)?.value).toBe("2024-01");
    expect(months).toHaveLength(14);
    expect(selectedBillingMonth(new URLSearchParams("through_at=old")).has("through_at")).toBe(false);
    expect(reportingMonths(new Date("2026-11-15T12:00:00Z"), "2025-12-01T00:00:00Z").at(-1)?.value).toBe("2025-11");
  });
});
