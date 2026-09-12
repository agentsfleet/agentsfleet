import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import FleetStatusLine from "./FleetStatusLine";
import type { EventDetail, EventRow } from "@/lib/api/events";
import {
  METRICS_COST_LABEL,
  METRICS_EMPTY,
  METRICS_OUTCOME_LABEL,
  METRICS_STATUS_LABEL,
  METRICS_TIME_LABEL,
  METRICS_TOKENS_LABEL,
} from "./console-copy";
import { OUTCOME } from "@/lib/events/event-summary";
import { formatTimeClock } from "@agentsfleet/design-system";

afterEach(() => cleanup());

function event(over: Partial<EventDetail> = {}): EventDetail {
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
    failure_detail: null,
    checkpoint_id: null,
    resumes_event_id: null,
    cost_nanos: 40_000_000,
    created_at: 1_700_000_000_000,
    updated_at: 1_700_000_000_000,
    ...over,
  };
}

function renderLine(latest: EventRow | null, pendingApprovals = 0, summaryAvailable = true, status = "active") {
  return render(
    <FleetStatusLine
      status={status}
      latest={latest}
      pendingApprovals={pendingApprovals}
      approvalsHref="/w/ws_1/approvals?fleetId=agt_1"
      summaryAvailable={summaryAvailable}
    />,
  );
}

const line = () => screen.getByLabelText("Fleet summary");
const toneOf = (text: string | RegExp) => screen.getByText(text).closest("[data-tone]")?.getAttribute("data-tone");

describe("FleetStatusLine", () => {
  it("shows status, durable outcome, tokens, spend, and duration on one line", () => {
    renderLine(event());
    expect(line().className).toContain("font-mono");
    expect(screen.getByText("active")).toBeTruthy();
    // The line names the outcome rather than quoting the answer: the list read
    // carries no reply text, so a completed run with nothing else to say reads
    // as exactly that.
    expect(screen.getByText(OUTCOME.COMPLETED)).toBeTruthy();
    expect(screen.getByText("1,500")).toBeTruthy();
    expect(screen.getByText("tok")).toBeTruthy();
    expect(screen.getByText("$0.04")).toBeTruthy();
    expect(screen.getByText("12.0s")).toBeTruthy();
  });

  it("keeps every figure's label for screen readers and for the acceptance walk", () => {
    // The glyph is what a sighted reader sees; the label stays in the text so
    // a screen reader hears "Tokens 1,500" and the acceptance walk can grep
    // `Tokens\s*(\S+)` off the line's innerText, as it did off the old strip.
    renderLine(event());
    for (const label of [
      METRICS_STATUS_LABEL,
      METRICS_OUTCOME_LABEL,
      METRICS_TOKENS_LABEL,
      METRICS_COST_LABEL,
      METRICS_TIME_LABEL,
    ]) {
      expect(screen.getByText(label).className).toContain("sr-only");
    }
    expect(line().textContent).toMatch(/Tokens\s*1,500/);
    expect(line().textContent).toMatch(/Duration\s*12\.0s/);
  });

  it("lights the status cell only while the fleet is active", () => {
    renderLine(event());
    expect(toneOf("active")).toBe("pulse");
    cleanup();
    renderLine(event(), 0, true, "paused");
    expect(toneOf("paused")).toBe("neutral");
  });

  it("says a run is still working rather than naming its event type, and keeps the glyph turning", () => {
    renderLine(event({ status: "received", event_type: "webhook", response_text: null }));
    expect(screen.getByText(OUTCOME.WORKING)).toBeTruthy();
    expect(toneOf(OUTCOME.WORKING)).toBe("foreground");
    expect(line().querySelector(".animate-spin")).not.toBeNull();
  });

  it.each([
    [{ status: "gate_blocked", response_text: null }, OUTCOME.WAITING_APPROVAL, "warning"],
    [{ status: "fleet_error", event_type: "ticket", response_text: null }, OUTCOME.FAILED, "danger"],
    [{ status: "processed", event_type: "ticket", response_text: null }, OUTCOME.COMPLETED, "success"],
  ] as const)("derives every stored outcome fallback with its tone", (over, expected, tone) => {
    renderLine(event(over));
    expect(screen.getByText(expected)).toBeTruthy();
    expect(toneOf(expected)).toBe(tone);
    expect(line().querySelector(".animate-spin")).toBeNull();
  });

  it("says a run completed rather than claiming no reply when the read carries no body", () => {
    // Every processed row the list read returns looks like this: `response_text`
    // is null because the page statement does not select it. The strip used to
    // read that null as "no reply recorded" — while the thread underneath
    // rendered the reply. The sentence may state completion and nothing more.
    renderLine(event({ status: "processed", response_text: null, failure_label: null }));
    expect(screen.getByText(OUTCOME.COMPLETED)).toBeTruthy();
    expect(screen.queryByText(/no reply/i)).toBeNull();
  });

  it("renders a runner failure as a sentence, never as its raw tag, in the danger tone", () => {
    // The raw tag is what an operator saw here before: `startup_posture`.
    renderLine(event({ status: "fleet_error", failure_label: "startup_posture", response_text: null }));
    expect(screen.getByText("Failed a startup safety check")).toBeTruthy();
    expect(screen.queryByText("startup_posture")).toBeNull();
    expect(toneOf("Failed a startup safety check")).toBe("danger");
  });

  it("reads a processed row that still names a failure as a failure", () => {
    // The row's own classification wins over its status column: a label is a
    // recorded fault whatever the status says.
    renderLine(event({ status: "processed", failure_label: "startup_posture" }));
    expect(toneOf("Failed a startup safety check")).toBe("danger");
  });

  it("omits the time rather than printing a broken one", () => {
    // A row whose stored timestamp does not read as a date still renders its
    // outcome; the line drops the time instead of showing "Invalid Date".
    renderLine(event({ created_at: Number.NaN }));
    expect(screen.getByText(OUTCOME.COMPLETED)).toBeTruthy();
    expect(screen.queryByText(/invalid/i)).toBeNull();
    expect(line().querySelector("time")).toBeNull();
  });

  it("shows when the latest outcome happened", () => {
    const at = Date.UTC(2026, 6, 21, 10, 42, 17);
    renderLine(event({ created_at: at }));
    expect(screen.getByText(OUTCOME.COMPLETED)).toBeTruthy();
    expect(screen.getByText(formatTimeClock(new Date(at)))).toBeTruthy();
  });

  it("renders missing telemetry as unknown, never fabricated zero, and never a unit beside a dash", () => {
    renderLine(event({ tokens: null, wall_ms: null, cost_nanos: null }));
    expect(screen.getAllByText("—")).toHaveLength(3);
    expect(screen.queryByText("$0.00")).toBeNull();
    expect(screen.queryByText("tok")).toBeNull();
  });

  it("links pending approvals to the fleet-filtered inbox", () => {
    renderLine(event(), 2);
    const link = screen.getByRole("link", { name: /2 approvals waiting/i });
    expect(link.getAttribute("href")).toBe("/w/ws_1/approvals?fleetId=agt_1");
    expect(link.closest("[data-tone]")?.getAttribute("data-tone")).toBe("warning");
  });

  it("names a single waiting approval in the singular", () => {
    renderLine(event(), 1);
    expect(screen.getByRole("link", { name: /^1 approval waiting/i })).toBeTruthy();
  });

  it("shows no approvals cell when none wait", () => {
    renderLine(event(), 0);
    expect(screen.queryByRole("link")).toBeNull();
  });

  it("renders the empty note when no outcome exists", () => {
    renderLine(null);
    expect(screen.getByText(METRICS_EMPTY)).toBeTruthy();
    expect(toneOf(METRICS_EMPTY)).toBe("neutral");
  });

  it("distinguishes an unavailable summary read from empty data", () => {
    renderLine(null, 0, false);
    expect(screen.getByText("Latest data unavailable.")).toBeTruthy();
    expect(screen.queryByText(METRICS_EMPTY)).toBeNull();
    // Nothing is known, so every figure is a dash.
    expect(screen.getAllByText("—")).toHaveLength(3);
  });
});
