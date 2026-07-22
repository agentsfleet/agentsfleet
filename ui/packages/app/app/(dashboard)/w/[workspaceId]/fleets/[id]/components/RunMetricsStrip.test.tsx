import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import RunMetricsStrip from "./RunMetricsStrip";
import type { EventRow } from "@/lib/api/events";
import { METRICS_EMPTY, METRICS_TIME_LABEL } from "./console-copy";
import { OUTCOME } from "@/lib/events/event-summary";
import { formatTimeClock } from "@agentsfleet/design-system";

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

  it("says a run is still working rather than naming its event type", () => {
    renderStrip(event({ status: "received", event_type: "webhook", response_text: null }));
    expect(screen.getByText(OUTCOME.WORKING)).toBeTruthy();
  });

  it.each([
    [{ status: "gate_blocked", response_text: null }, OUTCOME.WAITING_APPROVAL],
    [{ status: "fleet_error", event_type: "ticket", response_text: null }, OUTCOME.FAILED],
    [{ status: "processed", event_type: "ticket", response_text: null }, OUTCOME.NO_REPLY],
  ] as const)("derives every stored outcome fallback", (over, expected) => {
    renderStrip(event(over));
    expect(screen.getByText(expected)).toBeTruthy();
  });

  it("renders a runner failure as a sentence, never as its raw tag", () => {
    // The raw tag is what an operator saw here before: `startup_posture`.
    renderStrip(event({
      status: "fleet_error",
      failure_label: "startup_posture",
      response_text: null,
    }));
    expect(screen.getByText("Failed a startup safety check")).toBeTruthy();
    expect(screen.queryByText("startup_posture")).toBeNull();
  });

  it("omits the time rather than printing a broken one", () => {
    // A row whose stored timestamp does not read as a date still renders its
    // outcome; the strip drops the time instead of showing "Invalid Date".
    renderStrip(event({ created_at: Number.NaN, response_text: "review completed" }));
    expect(screen.getByText("review completed")).toBeTruthy();
    expect(screen.queryByText(/invalid/i)).toBeNull();
  });

  it("shows when the latest outcome happened", () => {
    const at = Date.UTC(2026, 6, 21, 10, 42, 17);
    renderStrip(event({ created_at: at, response_text: "Pull request review completed" }));
    expect(screen.getByText("Pull request review completed")).toBeTruthy();
    expect(screen.getByText(formatTimeClock(new Date(at)))).toBeTruthy();
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
