import { describe, expect, it } from "vitest";
import {
  BILLING_ALL, BILLING_QUERY, BILLING_SETUP, changedFilter, first, previousBillingMonth, reportOptions,
} from "@/app/(dashboard)/settings/billing/lib/report-query";

describe("billing report navigation", () => {
  it("starts with a month report and no other filter", () => {
    expect(reportOptions({})).toEqual({ period: "month", fleet_id: undefined, activity_type: undefined, through_at: undefined });
    expect(first(["today", "month"])).toBe("today");
  });

  it.each([BILLING_ALL, BILLING_SETUP, "fleet-id"])("maps fleet choice %s without widening explicit input", fleet => {
    const result = reportOptions({ fleet, through_at: "cutoff", period: "today" });
    expect(result).toMatchObject({ period: "today", through_at: "cutoff" });
    expect(result.fleet_id).toBe(fleet === "fleet-id" ? fleet : undefined);
    expect(result.activity_type).toBe(fleet === BILLING_SETUP ? "setup" : undefined);
  });

  it("passes explicit invalid filters for server validation and never forwards a removed one", () => {
    const result = reportOptions({ period: "year", fleet: "bad-id", model: "old-model", c: "old", ps: "50" });
    expect(result).toEqual({ period: "year", fleet_id: "bad-id", activity_type: undefined, through_at: undefined });
  });

  it("changing a filter retains the selected cutoff, and changing period returns to now", () => {
    const original = new URLSearchParams("period=month&fleet=fleet&through_at=cutoff");
    const next = changedFilter(original, BILLING_QUERY.fleet, "other-fleet");
    expect(next.get(BILLING_QUERY.fleet)).toBe("other-fleet");
    expect(next.get("through_at")).toBe("cutoff");
    expect(changedFilter(original, BILLING_QUERY.period, "today").has("through_at")).toBe(false);
    expect(original.get("fleet")).toBe("fleet");
    expect(changedFilter(original, BILLING_QUERY.fleet, BILLING_ALL).has(BILLING_QUERY.fleet)).toBe(false);
  });

  it.each([["2026-11-01T00:00:00Z", "2026-10-31T23:59:59.999Z"],
    ["2026-01-01T00:00:00Z", "2025-12-31T23:59:59.999Z"], ["2024-03-01T00:00:00Z", "2024-02-29T23:59:59.999Z"]])(
    "previous calendar month from %s preserves billing filters", (from, cutoff) => {
      const params = new URLSearchParams("period=today&fleet=deleted-fleet");
      const next = previousBillingMonth(params, from);
      expect(next.get("period")).toBe("month"); expect(next.get("through_at")).toBe(cutoff);
      expect(next.get("fleet")).toBe("deleted-fleet");
      expect(params.get("period")).toBe("today");
    });
});
