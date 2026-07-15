import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import RunsLedger from "./RunsLedger";
import type { EventRow } from "@/lib/api/events";
import {
  LEDGER_COST_UNKNOWN,
  ROLLUP_FAILED_LABEL,
  ROLLUP_TOKENS_LABEL,
  ROLLUP_WAKES_LABEL,
  ROLLUP_WINDOW_LABEL,
  ROLLUP_WINDOW_UNAVAILABLE,
} from "./console-copy";

afterEach(() => cleanup());

// One cent in billing nanos ($0.01). The rollup arithmetic is asserted against
// dollar strings derived from multiples of this, so a single named unit keeps
// every fixture cost readable.
const CENT_NANOS = 10_000_000;

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
    tokens: 100,
    wall_ms: 5_000,
    failure_label: null,
    checkpoint_id: null,
    resumes_event_id: null,
    cost_nanos: CENT_NANOS,
    created_at: 1_700_000_000_000,
    updated_at: 1_700_000_000_000,
    ...over,
  };
}

// Reads a rollup stat's value by finding its label then the sibling value node.
function statValue(label: string): string {
  const rollup = screen.getByLabelText(ROLLUP_WINDOW_LABEL);
  const labelNode = within(rollup).getByText(label);
  return labelNode.parentElement?.querySelector("span:last-child")?.textContent ?? "";
}

const LIFETIME_NANOS = 1_000_000_000; // $1.00

describe("RunsLedger", () => {
  it("test_ledger_cost_is_server_truth", () => {
    render(
      <RunsLedger
        windowEvents={[evt({ event_id: "a", cost_nanos: 4_710_000 }), evt({ event_id: "b", cost_nanos: null })]}
        lifetimeBudgetNanos={LIFETIME_NANOS}
      />,
    );
    const costs = screen.getAllByTestId("ledger-cost").map((n) => n.textContent);
    // The server nanos rendered as dollars verbatim; a null cost renders "—",
    // never a token×rate estimate.
    expect(costs).toContain("$0.0047");
    expect(costs).toContain(LEDGER_COST_UNKNOWN);
  });

  it("test_rollup_aggregates_seven_day_window", () => {
    render(
      <RunsLedger
        windowEvents={[
          evt({ event_id: "a", tokens: 100, cost_nanos: CENT_NANOS, status: "processed" }),
          evt({ event_id: "b", tokens: 200, cost_nanos: 2 * CENT_NANOS, status: "fleet_error" }),
          evt({ event_id: "c", tokens: null, cost_nanos: null, status: "processed" }),
        ]}
        lifetimeBudgetNanos={LIFETIME_NANOS}
      />,
    );
    expect(statValue(ROLLUP_WAKES_LABEL)).toBe("3");
    expect(statValue(ROLLUP_TOKENS_LABEL)).toBe("300");
    expect(statValue(ROLLUP_FAILED_LABEL)).toBe("1");
    // spend = 10m + 20m = 30m nanos = $0.03; the null-cost run adds 0 spend.
    expect(screen.getByText("$0.03")).toBeTruthy();
  });

  it("test_rollup_spend_is_server_truth", () => {
    // A single null-cost event: one wake, zero spend — the missing telemetry
    // row never vanishes the run from the count. Lifetime is budget_used_nanos.
    render(
      <RunsLedger
        windowEvents={[evt({ tokens: 50, cost_nanos: null })]}
        lifetimeBudgetNanos={LIFETIME_NANOS}
      />,
    );
    expect(statValue(ROLLUP_WAKES_LABEL)).toBe("1");
    // Window spend is $0.00 (only a null-cost wake); lifetime is $1.00, shown
    // separately from the window sum.
    expect(screen.getByText("$0.00")).toBeTruthy();
    expect(screen.getByText("$1.00")).toBeTruthy();
  });

  it("renders an unknown row status and omits absent token and wall metrics", () => {
    render(
      <RunsLedger
        windowEvents={[evt({ status: "runner_restarted" as EventRow["status"], tokens: null, wall_ms: null })]}
        lifetimeBudgetNanos={LIFETIME_NANOS}
      />,
    );
    expect(screen.getByText("runner_restarted")).toBeTruthy();
    expect(screen.queryByText(/tok$/)).toBeNull();
    expect(screen.queryByText("5.0s")).toBeNull();
  });

  it("test_rollup_empty_window", () => {
    render(<RunsLedger windowEvents={[]} lifetimeBudgetNanos={0} />);
    // Zero events → the rollup renders zeros, not an absent or broken panel.
    expect(statValue(ROLLUP_WAKES_LABEL)).toBe("0");
    expect(screen.getByLabelText(ROLLUP_WINDOW_LABEL)).toBeTruthy();
  });

  it("test_rollup_degrades_on_window_fetch_failure", () => {
    // windowEvents === null models the 7-day fetch failing: the rollup degrades
    // to the lifetime figure + the "recent window unavailable" note, not a blank.
    render(<RunsLedger windowEvents={null} lifetimeBudgetNanos={LIFETIME_NANOS} />);
    expect(screen.getByText(ROLLUP_WINDOW_UNAVAILABLE)).toBeTruthy();
    expect(screen.getByText("$1.00")).toBeTruthy();
    // No per-run rollup counts are claimed when the window is unavailable.
    expect(screen.queryByText(ROLLUP_WAKES_LABEL)).toBeNull();
  });
});
