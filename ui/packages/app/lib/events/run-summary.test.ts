import { describe, expect, it } from "vitest";
import type { EventDetail, EventRow } from "@/lib/api/events";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import { buildRunSummary, figuresOfRow, latestFigures, sameFigures } from "./run-summary";

const STATUS_ACTIVE = "active";
const PENDING = 3;
// Instants for the ordering cases: two rows share TIED_AT, so the tie-break on
// the stream entry id is what the test reaches.
const OLDER_AT = 1_000;
const TIED_AT = 2_000;
const OPTIMISTIC_AT = 9_000;
const FAILED_AT = 9_500;

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

function event(over: Partial<FleetEvent> = {}): FleetEvent {
  return {
    id: "evt_1",
    role: "system",
    actor: "cron:*",
    text: "",
    reply: "",
    outcome: "",
    failureLabel: null,
    failureDetail: null,
    createdAt: new Date(1_700_000_000_000),
    status: "processed",
    tokens: 1500,
    wallMs: 12_000,
    costNanos: 40_000_000,
    ...over,
  };
}

describe("buildRunSummary", () => {
  it("the thread page and a live row build the same figures", () => {
    const fromThread = buildRunSummary(STATUS_ACTIVE, { items: [turn(), turn({ event_id: "evt_0" })] }, PENDING);
    // The bodies ride along on the thread row but never reach the strip, and
    // the registry's view of the same row reads identically.
    expect(fromThread.latest).toEqual(figuresOfRow(row()));
    expect(fromThread.latest).toEqual(latestFigures([event()]));
    expect(fromThread.pendingApprovals).toBe(PENDING);
    expect(fromThread.latestAvailable).toBe(true);
  });

  it("a failed read is unavailable, an empty read is empty — never the same thing", () => {
    expect(buildRunSummary(STATUS_ACTIVE, null, 0)).toEqual({
      status: STATUS_ACTIVE,
      latest: null,
      latestAvailable: false,
      pendingApprovals: 0,
    });
    const empty = buildRunSummary(STATUS_ACTIVE, { items: [] }, 0);
    expect(empty.latest).toBeNull();
    expect(empty.latestAvailable).toBe(true);
  });
});

describe("latestFigures", () => {
  it("picks the newest server row by instant, then by stream entry id", () => {
    const older = event({ id: "1725000000000-0", createdAt: new Date(OLDER_AT), tokens: 1 });
    const tied = event({ id: "1725000000000-1", createdAt: new Date(TIED_AT), tokens: 2 });
    const newest = event({ id: "1725000000000-2", createdAt: new Date(TIED_AT), tokens: 3 });
    expect(latestFigures([newest, older, tied])?.tokens).toBe(3);
  });

  it("skips the rows the browser made: an optimistic steer or a refused send", () => {
    const server = event({ id: "srv", createdAt: new Date(OLDER_AT) });
    const optimistic = event({ id: "optim-1", status: "optimistic", createdAt: new Date(OPTIMISTIC_AT) });
    const failed = event({ id: "optim-2", status: "failed", createdAt: new Date(FAILED_AT) });
    expect(latestFigures([server, optimistic, failed])?.created_at).toBe(OLDER_AT);
    expect(latestFigures([optimistic, failed])).toBeNull();
    expect(latestFigures([])).toBeNull();
  });

  it("a row with no figures yet reads as unknown, never as zero", () => {
    const opened = event({ tokens: undefined, wallMs: undefined, costNanos: undefined, status: "received" });
    expect(latestFigures([opened])).toMatchObject({ tokens: null, wall_ms: null, cost_nanos: null });
  });
});

describe("sameFigures", () => {
  it("compares field by field, and null only equals null", () => {
    expect(sameFigures(figuresOfRow(row()), figuresOfRow(row()))).toBe(true);
    expect(sameFigures(figuresOfRow(row()), figuresOfRow(row({ tokens: 1 })))).toBe(false);
    expect(sameFigures(null, null)).toBe(true);
    expect(sameFigures(null, figuresOfRow(row()))).toBe(false);
  });
});
