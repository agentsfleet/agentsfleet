import { describe, expect, it } from "vitest";

import {
  groupEventRows,
  isZeroMetricOnFailure,
  MIN_ROW_GROUP,
} from "@/lib/events/event-row-grouping";
import type { EventRow } from "@/lib/api/events";

let seq = 0;

function row(over: Partial<EventRow> = {}): EventRow {
  seq += 1;
  return {
    event_id: over.event_id ?? `evt_${seq}`,
    fleet_id: "zomb_1",
    workspace_id: "ws_1",
    actor: over.actor ?? "webhook:github",
    event_type: "webhook",
    status: over.status ?? "fleet_error",
    request_json: "{}",
    response_text: over.response_text ?? null,
    tokens: over.tokens ?? 0,
    wall_ms: over.wall_ms ?? 0,
    cost_nanos: over.cost_nanos ?? 0,
    failure_label: "failure_label" in over ? over.failure_label ?? null : "startup_posture",
    failure_detail: "failure_detail" in over ? over.failure_detail ?? null : "no instructions configured",
    checkpoint_id: null,
    resumes_event_id: null,
    created_at: over.created_at ?? 1_700_000_000_000,
    updated_at: 1_700_000_000_000,
  };
}

function failures(count: number, over: Partial<EventRow> = {}): EventRow[] {
  return Array.from({ length: count }, () => row(over));
}

describe("groupEventRows", () => {
  it("collapses a run of identical failures while keeping every row reachable", () => {
    const entries = groupEventRows(failures(15));
    expect(entries).toHaveLength(1);
    expect(entries[0]?.rows).toHaveLength(15);
  });

  it("leaves a lone failure as its own entry", () => {
    expect(groupEventRows(failures(MIN_ROW_GROUP - 1))[0]?.rows).toHaveLength(1);
    expect(groupEventRows(failures(MIN_ROW_GROUP))[0]?.rows).toHaveLength(MIN_ROW_GROUP);
  });

  it("never collapses successes, however alike they look", () => {
    // Two successful runs are two pieces of work; their results differ even
    // when actor and status match, so merging them would hide what happened.
    const entries = groupEventRows([
      row({ status: "processed", response_text: "reviewed #1", failure_label: null, failure_detail: null }),
      row({ status: "processed", response_text: "reviewed #2", failure_label: null, failure_detail: null }),
    ]);
    expect(entries).toHaveLength(2);
  });

  it("breaks a run on a success between failures", () => {
    const entries = groupEventRows([
      ...failures(3),
      row({ status: "processed", failure_label: null, failure_detail: null }),
      ...failures(2),
    ]);
    expect(entries.map((entry) => entry.rows.length)).toEqual([3, 1, 2]);
  });

  it("keeps one class with two different causes apart", () => {
    const entries = groupEventRows([
      ...failures(2, { failure_detail: "no instructions configured" }),
      ...failures(2, { failure_detail: "no model configured" }),
    ]);
    expect(entries).toHaveLength(2);
  });

  it("keeps different actors apart", () => {
    const entries = groupEventRows([
      ...failures(2, { actor: "webhook:github" }),
      ...failures(2, { actor: "webhook:slack" }),
    ]);
    expect(entries).toHaveLength(2);
  });

  it("refuses to give unexplained failures a shared cause", () => {
    // No recorded label means nobody said why. Counting these together would
    // claim a common cause that was never established.
    const entries = groupEventRows(failures(3, { failure_label: null, failure_detail: null }));
    expect(entries).toHaveLength(3);
  });

  it("preserves order and loses nothing", () => {
    const rows = [...failures(3), row({ status: "processed", failure_label: null, failure_detail: null })];
    const flattened = groupEventRows(rows).flatMap((entry) => entry.rows);
    expect(flattened.map((r) => r.event_id)).toEqual(rows.map((r) => r.event_id));
  });

  it("survives an empty page", () => {
    expect(groupEventRows([])).toEqual([]);
  });
});

describe("isZeroMetricOnFailure", () => {
  it("dims a failed run's zero, which is absence rather than measurement", () => {
    expect(isZeroMetricOnFailure(row({ status: "fleet_error" }), 0)).toBe(true);
  });

  it("leaves a successful run's zero alone, and never touches a real figure", () => {
    expect(isZeroMetricOnFailure(row({ status: "processed" }), 0)).toBe(false);
    expect(isZeroMetricOnFailure(row({ status: "fleet_error" }), 42)).toBe(false);
    // An unknown figure already renders a dash; it is not a zero to dim.
    expect(isZeroMetricOnFailure(row({ status: "fleet_error" }), null)).toBe(false);
  });
});
