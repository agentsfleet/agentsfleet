import { describe, expect, it } from "vitest";
import {
  BILLING_ALL, BILLING_QUERY, BILLING_SETUP, activityKey, changedFilter, detailTrail, first,
  nextDetailPage, previousBillingMonth, reportOptions, selectedActivity,
} from "@/app/(dashboard)/settings/billing/lib/report-query";

describe("billing report navigation", () => {
  it("the work kind is part of a disclosure's identity", () => {
    expect(activityKey({ id: "shared", activity_type: "runtime" })).not.toBe(activityKey({ id: "shared", activity_type: "setup" }));
  });
  it("starts with a bounded month report and no continuation", () => {
    expect(reportOptions({})).toEqual({ period: "month", limit: 25, starting_after: undefined,
      fleet_id: undefined, activity_type: undefined, model: undefined, through_at: undefined });
    expect(first(["today", "month"])).toBe("today");
  });

  it.each([BILLING_ALL, BILLING_SETUP, "fleet-id"])("maps fleet choice %s without widening explicit input", fleet => {
    const result = reportOptions({ fleet, model: "exact-model", through_at: "cutoff", period: "today", ps: "50", cps: "50", c: ["first", "last"] });
    expect(result).toMatchObject({ period: "today", model: "exact-model", through_at: "cutoff", limit: 50, starting_after: "last" });
    expect(result.fleet_id).toBe(fleet === "fleet-id" ? fleet : undefined);
    expect(result.activity_type).toBe(fleet === BILLING_SETUP ? "setup" : undefined);
  });

  it("drops stale continuations while passing explicit invalid filters for server validation", () => {
    expect(reportOptions({ c: "old", cps: "25", ps: "50", period: "year", fleet: "bad-id" }))
      .toMatchObject({ limit: 50, starting_after: undefined, period: "year", fleet_id: "bad-id" });
  });

  it("changing a filter resets both pagers and disclosure while retaining the selected cutoff", () => {
    const original = new URLSearchParams("period=month&fleet=fleet&model=old&c=cursor&cps=50&ps=50&through_at=cutoff&activity=work&dc=detail");
    const next = changedFilter(original, BILLING_QUERY.model, "new/model");
    expect(next.get(BILLING_QUERY.model)).toBe("new/model");
    expect(next.get("ps")).toBe("50");
    for (const key of ["c", "cps", "activity", "dc"]) expect(next.has(key)).toBe(false);
    expect(next.get("through_at")).toBe("cutoff");
    expect(changedFilter(original, BILLING_QUERY.period, "today").has("through_at")).toBe(false);
    expect(original.get("c")).toBe("cursor");
    expect(changedFilter(original, BILLING_QUERY.fleet, BILLING_ALL).has(BILLING_QUERY.fleet)).toBe(false);
  });

  it.each([["2026-11-01T00:00:00Z", "2026-10-31T23:59:59.999Z"],
    ["2026-01-01T00:00:00Z", "2025-12-31T23:59:59.999Z"], ["2024-03-01T00:00:00Z", "2024-02-29T23:59:59.999Z"]])(
    "previous calendar month from %s preserves billing filters", (from, cutoff) => {
      const params = new URLSearchParams("period=today&fleet=deleted-fleet&model=recorded-model&c=old&activity=old&dc=old");
      const next = previousBillingMonth(params, from);
      expect(next.get("period")).toBe("month"); expect(next.get("through_at")).toBe(cutoff);
      expect(next.get("fleet")).toBe("deleted-fleet"); expect(next.get("model")).toBe("recorded-model");
      for (const key of ["c", "activity", "dc"]) expect(next.has(key)).toBe(false);
      expect(params.get("period")).toBe("today");
    });

  it("opening and closing details retain the report cutoff and main page", () => {
    const params = new URLSearchParams("c=page&cps=25&dc=old");
    const opened = selectedActivity(params, "work", "cutoff");
    expect(opened.get(BILLING_QUERY.activity)).toBe("work");
    expect(opened.get(BILLING_QUERY.through)).toBe("cutoff");
    expect(opened.get("c")).toBe("page");
    expect(opened.has(BILLING_QUERY.detailCursor)).toBe(false);
    expect(selectedActivity(opened, "", "cutoff").has(BILLING_QUERY.activity)).toBe(false);
  });

  it("detail paging replaces rows, remembers the way back and ignores stale jumps", () => {
    const params = new URLSearchParams("activity=work&through_at=cutoff");
    const next = nextDetailPage(params, 2, "second");
    expect(next?.getAll(BILLING_QUERY.detailCursor)).toEqual(["second"]);
    expect(next?.get(BILLING_QUERY.activity)).toBe("work");
    expect(nextDetailPage(new URLSearchParams(next ?? undefined), 1, null)?.getAll(BILLING_QUERY.detailCursor)).toEqual([]);
    expect(nextDetailPage(params, 2, null)).toBeNull();
    expect(nextDetailPage(params, 4, "second")).toBeNull();
    expect(nextDetailPage(params, 0, "second")).toBeNull();
    expect(detailTrail(undefined)).toEqual([]);
    expect(detailTrail("first")).toEqual(["first"]);
    expect(detailTrail(["first", "", "second"])).toEqual(["first", "second"]);
  });
});
