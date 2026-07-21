import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import RunMetricsStrip from "./RunMetricsStrip";
import type { EventRow } from "@/lib/api/events";
import { METRICS_EMPTY, METRICS_TIME_LABEL } from "./console-copy";

afterEach(() => cleanup());

function evt(over: Partial<EventRow> = {}): EventRow {
  return {
    event_id: "evt_1",
    fleet_id: "agt_1",
    workspace_id: "ws_1",
    actor: "cron:*",
    event_type: "cron",
    status: "processed",
    request_json: "{}",
    response_text: null,
    tokens: 1500,
    wall_ms: 12_000,
    failure_label: null,
    checkpoint_id: null,
    resumes_event_id: null,
    cost_nanos: 40_000_000,
    created_at: 1_700_000_000_000,
    updated_at: 1_700_000_000_000,
    ...over,
  };
}

describe("RunMetricsStrip", () => {
  it("test_metrics_strip_is_server_truth", () => {
    render(<RunMetricsStrip latest={evt()} />);
    // Every figure is the server field, rendered verbatim — tokens/time/cost.
    expect(screen.getByText("1,500")).toBeTruthy();
    expect(screen.getByText("12.0s")).toBeTruthy();
    expect(screen.getByText("$0.04")).toBeTruthy();
  });

  it("test_metrics_strip_labels_time", () => {
    render(<RunMetricsStrip latest={evt()} />);
    // The duration figure is labelled in plain words; "Wall" is jargon and
    // collides with the Live Wall page name.
    expect(screen.getByText(METRICS_TIME_LABEL)).toBeTruthy();
    expect(screen.queryByText("Wall")).toBeNull();
  });

  it("renders cost as — when the run has no telemetry (null cost, never a fabricated zero)", () => {
    render(<RunMetricsStrip latest={evt({ cost_nanos: null })} />);
    expect(screen.getByText("—")).toBeTruthy();
    // The dollar figure must not appear — a missing telemetry row is unknown,
    // not $0.00.
    expect(screen.queryByText("$0.00")).toBeNull();
  });

  it("renders missing token and wall metrics as unknown, not zero", () => {
    render(<RunMetricsStrip latest={evt({ tokens: null, wall_ms: null })} />);
    expect(screen.getAllByText("—")).toHaveLength(2);
    expect(screen.queryByText("0")).toBeNull();
    expect(screen.queryByText("0ms")).toBeNull();
  });

  it("renders the empty note when no run has been recorded", () => {
    render(<RunMetricsStrip latest={null} />);
    expect(screen.getByText(METRICS_EMPTY)).toBeTruthy();
  });
});
