import { describe, expect, it } from "vitest";
import type { EventDetail, EventRow } from "@/lib/api/events";
import { buildRunSummary } from "./run-summary";

const STATUS_ACTIVE = "active";
const NEXT_CURSOR = "cur_2";

function row(over: Partial<EventRow> = {}): EventRow {
  return {
    event_id: "evt_1",
    fleet_id: "agt_1",
    workspace_id: "ws_1",
    actor: "cron:*",
    event_type: "cron",
    status: "processed",
    tokens: 1500,
    wall_ms: 12_000,
    failure_label: null,
    failure_detail: null,
    checkpoint_id: null,
    resumes_event_id: null,
    cost_nanos: 40_000_000,
    created_at: 1_700_000_000_000,
    updated_at: 1_700_000_000_000,
    ...over,
  };
}

/** The thread's row is the list row plus the two bodies the strip ignores. */
function turn(over: Partial<EventRow> = {}): EventDetail {
  return { ...row(over), request_json: "{}", response_text: "done" };
}

describe("buildRunSummary", () => {
  it("the thread page and the newest-event read build the same summary", () => {
    const approvals = { items: [{}], next_cursor: null };
    const fromThread = buildRunSummary(STATUS_ACTIVE, { items: [turn(), turn({ event_id: "evt_0" })] }, approvals);
    const fromEvents = buildRunSummary(STATUS_ACTIVE, { items: [row()] }, approvals);
    // The bodies ride along on the thread row but never reach the strip.
    expect(fromThread.latest).toMatchObject(row());
    expect(fromEvents).toEqual({ ...fromThread, latest: row() });
    expect(fromEvents.pendingApprovals).toBe(1);
    expect(fromEvents.latestAvailable).toBe(true);
  });

  it("a failed read is unavailable, an empty read is empty — never the same thing", () => {
    const failed = buildRunSummary(STATUS_ACTIVE, null, null);
    expect(failed).toEqual({
      status: STATUS_ACTIVE,
      latest: null,
      latestAvailable: false,
      pendingApprovals: 0,
      pendingApprovalsHasMore: false,
      approvalsAvailable: false,
    });
    const empty = buildRunSummary(STATUS_ACTIVE, { items: [] }, { items: [], next_cursor: null });
    expect(empty.latest).toBeNull();
    expect(empty.latestAvailable).toBe(true);
    expect(empty.approvalsAvailable).toBe(true);
  });

  it("a continuation cursor on the approvals page marks the count as truncated", () => {
    const more = buildRunSummary(STATUS_ACTIVE, { items: [row()] }, { items: [{}, {}], next_cursor: NEXT_CURSOR });
    expect(more.pendingApprovals).toBe(2);
    expect(more.pendingApprovalsHasMore).toBe(true);
  });
});
