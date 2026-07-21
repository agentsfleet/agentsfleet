import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import RunMetricsStrip from "./RunMetricsStrip";
import type { EventRow } from "@/lib/api/events";
import { METRICS_EMPTY, METRICS_TIME_LABEL } from "./console-copy";

afterEach(() => cleanup());

function event(over: Partial<EventRow> = {}): EventRow {
  return {
    event_id: "evt_1",
    fleet_id: "agt_1",
    workspace_id: "ws_1",
    actor: "cron:*",
    event_type: "cron",
    status: "processed",
    request_json: "{}",
    response_text: "Ticket triage completed",
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

function renderStrip(
  latest: EventRow | null,
  pendingApprovals = 0,
  pendingApprovalsHasMore = false,
  summaryAvailable = true,
  approvalsAvailable = true,
) {
  return render(
    <RunMetricsStrip
      status="active"
      latest={latest}
      pendingApprovals={pendingApprovals}
      pendingApprovalsHasMore={pendingApprovalsHasMore}
      approvalsHref="/w/ws_1/approvals?fleetId=agt_1"
      summaryAvailable={summaryAvailable}
      approvalsAvailable={approvalsAvailable}
    />,
  );
}

describe("RunMetricsStrip", () => {
  it("shows status, durable outcome, tokens, spend, and duration", () => {
    renderStrip(event());
    expect(screen.getByText("active")).toBeTruthy();
    expect(screen.getByText("Ticket triage completed")).toBeTruthy();
    expect(screen.getByText("1,500")).toBeTruthy();
    expect(screen.getByText("$0.04")).toBeTruthy();
    expect(screen.getByText("12.0s")).toBeTruthy();
    expect(screen.getByText(METRICS_TIME_LABEL)).toBeTruthy();
  });

  it("derives a received outcome only from stored status and event type", () => {
    renderStrip(event({ status: "received", event_type: "webhook", response_text: null }));
    expect(screen.getByText("Received webhook")).toBeTruthy();
  });

  it.each([
    [{ failure_label: "Provider quota exceeded", response_text: null }, "Provider quota exceeded"],
    [{ status: "gate_blocked", response_text: null }, "Waiting for approval"],
    [{ status: "fleet_error", event_type: "ticket", response_text: null }, "ticket failed"],
    [{ status: "processed", event_type: "ticket", response_text: null }, "ticket completed"],
  ] as const)("derives every stored outcome fallback", (over, expected) => {
    renderStrip(event(over));
    expect(screen.getByText(expected)).toBeTruthy();
  });

  it("renders missing telemetry as unknown, never fabricated zero", () => {
    renderStrip(event({ tokens: null, wall_ms: null, cost_nanos: null }));
    expect(screen.getAllByText("—")).toHaveLength(3);
    expect(screen.queryByText("$0.00")).toBeNull();
  });

  it("links pending approvals to the fleet-filtered inbox", () => {
    renderStrip(event(), 2);
    const link = screen.getByRole("link", { name: /2 approvals waiting/i });
    expect(link.getAttribute("href")).toBe("/w/ws_1/approvals?fleetId=agt_1");
  });

  it("marks a truncated singular approval count", () => {
    renderStrip(event(), 1, true);
    expect(screen.getByRole("link", { name: /1\+ approval waiting/i })).toBeTruthy();
  });

  it("renders the empty note when no outcome exists", () => {
    renderStrip(null);
    expect(screen.getByText(METRICS_EMPTY)).toBeTruthy();
  });

  it("distinguishes unavailable summary and approval reads from empty data", () => {
    renderStrip(null, 0, false, false, false);
    expect(screen.getByText("Latest data unavailable.")).toBeTruthy();
    expect(screen.getByText("Approvals unavailable")).toBeTruthy();
    expect(screen.queryByText(METRICS_EMPTY)).toBeNull();
  });
});
