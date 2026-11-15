import { describe, expect, it } from "vitest";
import {
  BILLING_ALL, BILLING_QUERY, BILLING_SETUP, changedFilter, first, selectedBillingMonth, reportOptions,
} from "@/app/(dashboard)/settings/billing/lib/report-query";
import { reportingMonths } from "@/app/(dashboard)/settings/billing/lib/report-months";

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

  it.each([["2026-11-01T00:00:00Z", "2026-10", "2026-10-31T23:59:59.999Z"],
    ["2026-01-01T00:00:00Z", "2025-12", "2025-12-31T23:59:59.999Z"],
    ["2024-03-01T00:00:00Z", "2024-02", "2024-02-29T23:59:59.999Z"]])(
    "selects a named reporting month from %s without changing the API", (from, expected, cutoff) => {
      const params = new URLSearchParams("period=today&fleet=deleted-fleet&model=recorded-model&c=old&through_at=old&activity=old&dc=old");
      const month = reportingMonths(new Date(from), from)[1];
      expect(month?.value).toBe(expected); expect(month?.through).toBe(cutoff);
      const next = selectedBillingMonth(params, month?.through);
      expect(next.get("period")).toBe("month"); expect(next.get("through_at")).toBe(cutoff);
      expect(next.has("month")).toBe(false); expect(next.has("timezone")).toBe(false);
      expect(next.get("fleet")).toBe("deleted-fleet"); expect(next.get("model")).toBe("recorded-model");
      for (const key of ["c", "activity", "dc"]) expect(next.has(key)).toBe(false);
      expect(params.get("period")).toBe("today");
    });

  it("keeps older history reachable with a bounded list and current month without a cutoff", () => {
    const months = reportingMonths(new Date("2026-11-15T12:00:00Z"), "2024-02-01T00:00:00Z");
    expect(months[0]).toMatchObject({value: "2026-11"});
    expect(months[0]?.through).toBeUndefined();
    expect(months.at(-2)?.value).toBe("2024-02"); expect(months.at(-1)?.value).toBe("2024-01");
    expect(months).toHaveLength(14);
    const edge = reportingMonths(new Date("2026-11-15T12:00:00Z"), "2025-12-01T00:00:00Z");
    expect(edge.at(-1)?.value).toBe("2025-11");
  });
});
