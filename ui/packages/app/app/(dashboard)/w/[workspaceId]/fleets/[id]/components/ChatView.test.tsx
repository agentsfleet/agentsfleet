import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { EventRow } from "@/lib/api/events";
import { METRICS_UNAVAILABLE } from "./console-copy";
import type { FleetRunSummary } from "./run-summary";

const { refreshMock, summaryActionMock } = vi.hoisted(() => ({
  refreshMock: vi.fn(),
  summaryActionMock: vi.fn(),
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: refreshMock }),
}));
vi.mock("../../actions", () => ({
  getFleetRunSummaryAction: summaryActionMock,
}));
// The thread is the completion SOURCE here, nothing more: the stub exposes the
// callback as a button so a test can complete a run without a stream.
vi.mock("@/components/domain/FleetThreadDynamic", () => ({
  default: ({ onRunCompleted }: { onRunCompleted: () => void }) =>
    React.createElement("button", { type: "button", onClick: onRunCompleted }, COMPLETE_LABEL),
}));

import { ChatView } from "./ChatView";

const COMPLETE_LABEL = "complete a run";
const STATUS_ACTIVE = "active";
const STATUS_PAUSED = "paused";
const WORKSPACE_ID = "ws_1";
const FLEET_ID = "agt_1";
const APPROVALS_HREF = "/w/ws_1/approvals?fleetId=agt_1";
const INITIAL_TOKENS = 1500;
const REFRESHED_TOKENS = 1200;
const RERENDERED_TOKENS = 900;

function row(tokens: number): EventRow {
  return {
    event_id: `evt_${tokens}`,
    fleet_id: FLEET_ID,
    workspace_id: WORKSPACE_ID,
    actor: "cron:*",
    event_type: "cron",
    status: "processed",
    tokens,
    wall_ms: 12_000,
    failure_label: null,
    failure_detail: null,
    checkpoint_id: null,
    resumes_event_id: null,
    cost_nanos: 40_000_000,
    created_at: 1_700_000_000_000,
    updated_at: 1_700_000_000_000,
  };
}

function summary(tokens: number, over: Partial<FleetRunSummary> = {}): FleetRunSummary {
  return {
    status: STATUS_ACTIVE,
    latest: row(tokens),
    latestAvailable: true,
    pendingApprovals: 0,
    pendingApprovalsHasMore: false,
    approvalsAvailable: true,
    ...over,
  };
}

function view(initialSummary: FleetRunSummary) {
  return React.createElement(ChatView, {
    workspaceId: WORKSPACE_ID,
    fleetId: FLEET_ID,
    fleetName: "Agent Finch",
    initial: [],
    initialSummary,
    approvalsHref: APPROVALS_HREF,
  });
}

function completeRun() {
  fireEvent.click(screen.getByRole("button", { name: COMPLETE_LABEL }));
}

beforeEach(() => {
  refreshMock.mockReset();
  summaryActionMock.mockReset();
});
afterEach(() => cleanup());

describe("ChatView — the run summary after a completion", () => {
  it("the strip shows the new run figures after the summary action resolves", async () => {
    summaryActionMock.mockResolvedValueOnce({
      ok: true,
      data: summary(REFRESHED_TOKENS, { pendingApprovals: 1 }),
    });
    render(view(summary(INITIAL_TOKENS)));
    expect(screen.getByText("1,500")).toBeTruthy();

    completeRun();

    await waitFor(() => expect(screen.getByText("1,200")).toBeTruthy());
    expect(screen.getByRole("link", { name: /1 approval waiting/i })).toBeTruthy();
    expect(summaryActionMock).toHaveBeenCalledWith(WORKSPACE_ID, FLEET_ID);
    // Same status as the server rendered: the server tree is left alone.
    expect(refreshMock).not.toHaveBeenCalled();
  });

  it("a failed summary read leaves the strip unchanged", async () => {
    summaryActionMock.mockResolvedValueOnce({ ok: false, error: "backend down", status: 503 });
    render(view(summary(INITIAL_TOKENS)));

    completeRun();

    await waitFor(() => expect(summaryActionMock).toHaveBeenCalledTimes(1));
    expect(screen.getByText("1,500")).toBeTruthy();
    expect(screen.queryByText(METRICS_UNAVAILABLE)).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(refreshMock).not.toHaveBeenCalled();
  });

  it("only a status change refreshes the server-rendered controls", async () => {
    summaryActionMock.mockResolvedValueOnce({
      ok: true,
      data: summary(REFRESHED_TOKENS, { status: STATUS_PAUSED }),
    });
    render(view(summary(INITIAL_TOKENS)));

    completeRun();

    // The header's lifecycle controls render from the fleet status on the
    // server, so a run that paused the fleet re-runs that tree exactly once.
    await waitFor(() => expect(refreshMock).toHaveBeenCalledTimes(1));
    // The strip's status stays what the server rendered until that refresh
    // hands down the new one — never a client-side guess.
    expect(screen.getByText(STATUS_ACTIVE)).toBeTruthy();
  });

  it("a server render resets the live figures to its own", async () => {
    summaryActionMock.mockResolvedValueOnce({ ok: true, data: summary(REFRESHED_TOKENS) });
    const { rerender } = render(view(summary(INITIAL_TOKENS)));
    completeRun();
    await waitFor(() => expect(screen.getByText("1,200")).toBeTruthy());

    // A router refresh arrives as a new initialSummary prop: server truth wins.
    rerender(view(summary(RERENDERED_TOKENS)));
    expect(screen.getByText("900")).toBeTruthy();
    expect(screen.queryByText("1,200")).toBeNull();
  });
});
