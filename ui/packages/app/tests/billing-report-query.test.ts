import { describe, expect, it } from "vitest";
import {
  BILLING_ALL, BILLING_QUERY, BILLING_SETUP, activityKey, changedFilter, detailTrail, first,
  nextDetailPage, reportOptions, selectedActivity,
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

  it("changing a filter resets both pagers, cutoff and disclosure while retaining page size", () => {
    const original = new URLSearchParams("period=month&fleet=fleet&model=old&c=cursor&cps=50&ps=50&through_at=cutoff&activity=work&dc=detail");
    const next = changedFilter(original, BILLING_QUERY.model, "new/model");
    expect(next.get(BILLING_QUERY.model)).toBe("new/model");
    expect(next.get("ps")).toBe("50");
    for (const key of ["c", "cps", "through_at", "activity", "dc"]) expect(next.has(key)).toBe(false);
    expect(original.get("c")).toBe("cursor");
    expect(changedFilter(original, BILLING_QUERY.fleet, BILLING_ALL).has(BILLING_QUERY.fleet)).toBe(false);
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
