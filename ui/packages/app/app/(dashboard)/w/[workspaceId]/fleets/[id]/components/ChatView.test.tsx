import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { type EventDetail, type LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import type { FleetRunSummary } from "@/lib/events/run-summary";
import { __resetRegistryForTests, reconcileServerRows } from "@/lib/streaming/fleet-stream-registry";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import { METRICS_UNAVAILABLE } from "./console-copy";

const { refreshMock } = vi.hoisted(() => ({ refreshMock: vi.fn() }));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: refreshMock }),
}));
// The thread is a sibling view over the same stream and plays no part in the
// strip; the strip's own subscription is what these tests drive.
vi.mock("@/components/domain/FleetThreadDynamic", () => ({ default: () => null }));

import { ChatView } from "./ChatView";

const STATUS_ACTIVE = "active";
const STATUS_PAUSED = "paused";
const WORKSPACE_ID = "ws_1";
const FLEET_ID = "agt_1";
const APPROVALS_HREF = "/w/ws_1/approvals?fleetId=agt_1";
const SEED_AT = 1_700_000_000_000;
const INITIAL_TOKENS = 1500;
const COMPLETED_TOKENS = 1200;
const RERENDERED_TOKENS = 900;

function turn(over: Partial<EventDetail> = {}): EventDetail {
  return {
    event_id: "evt_seed",
    fleet_id: FLEET_ID,
    workspace_id: WORKSPACE_ID,
    actor: "cron:*",
    event_type: "cron",
    status: "processed",
    request_json: "{}",
    response_text: "done",
    tokens: INITIAL_TOKENS,
    wall_ms: 12_000,
    failure_label: null,
    failure_detail: null,
    checkpoint_id: null,
    resumes_event_id: null,
    cost_nanos: 40_000_000,
    created_at: SEED_AT,
    updated_at: SEED_AT,
    ...over,
  };
}

function summary(over: Partial<FleetRunSummary> = {}): FleetRunSummary {
  return {
    status: STATUS_ACTIVE,
    latest: {
      status: "processed",
      failure_label: null,
      failure_detail: null,
      created_at: SEED_AT,
      tokens: INITIAL_TOKENS,
      wall_ms: 12_000,
      cost_nanos: 40_000_000,
    },
    latestAvailable: true,
    pendingApprovals: 0,
    ...over,
  };
}

/** The daemon's closing bracket: the whole row plus the two fleet facts. */
function completion(over: Partial<Extract<LiveFrame, { kind: "event_complete" }>> = {}): LiveFrame {
  return {
    kind: FRAME_KIND.EVENT_COMPLETE,
    ...turn({ event_id: "evt_live", tokens: COMPLETED_TOKENS, created_at: SEED_AT + 60_000 }),
    fleet_status: STATUS_ACTIVE,
    pending_approvals: 0,
    ...over,
  };
}

function view(initialSummary: FleetRunSummary, initial: EventDetail[] = [turn()]) {
  return React.createElement(ChatView, {
    workspaceId: WORKSPACE_ID,
    fleetId: FLEET_ID,
    fleetName: "Agent Finch",
    initial,
    initialSummary,
    approvalsHref: APPROVALS_HREF,
  });
}

function emit(frame: LiveFrame) {
  act(() => {
    FakeEventSource.instances[0]!.emit(frame);
  });
}

// Nothing the strip does may reach the network: the one connection is the
// fake EventSource, and any fetch — a Server Action, a proxy read — is a
// regression of the round-trip discipline this surface is graded on.
const fetchSpy = vi.fn(() => Promise.reject(new Error("no read is allowed here")));

beforeEach(() => {
  refreshMock.mockReset();
  fetchSpy.mockClear();
  vi.stubGlobal("fetch", fetchSpy);
  FakeEventSource.install();
  __resetRegistryForTests();
});
afterEach(() => {
  cleanup();
  __resetRegistryForTests();
  FakeEventSource.uninstall();
  vi.unstubAllGlobals();
});

describe("ChatView — the run summary as a view over the stream", () => {
  it("the strip shows the new run figures when the completion frame arrives, with no read", async () => {
    render(view(summary()));
    expect(screen.getByText("1,500")).toBeTruthy();

    emit({
      kind: FRAME_KIND.EVENT_RECEIVED,
      event_id: "evt_live",
      actor: "cron:*",
      event_type: "cron",
      created_at: SEED_AT + 60_000,
    });
    emit(completion({ pending_approvals: 1 }));

    expect(await screen.findByText("1,200")).toBeTruthy();
    expect(screen.getByRole("link", { name: /1 approval waiting/i })).toBeTruthy();
    // Same status as the server rendered: the server tree is left alone.
    expect(refreshMock).not.toHaveBeenCalled();
    // One connection, and nothing else on the wire — no Server Action, no fetch.
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("a revisit whose cached status the server has overtaken does not refresh", async () => {
    // First visit: a completion pauses the fleet, and the page refreshes once.
    const first = render(view(summary()));
    emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "evt_live", actor: "cron:*" });
    emit(completion({ fleet_status: STATUS_PAUSED }));
    expect(await screen.findByText(STATUS_PAUSED)).toBeTruthy();
    expect(refreshMock).toHaveBeenCalledTimes(1);
    first.unmount();
    refreshMock.mockReset();

    // Within the idle window the entry is still cached, holding "paused". The
    // operator resumed the fleet from the CLI and comes back: the server
    // renders "active", which is newer than the cached frame — and no refresh
    // is owed for a status the server itself just rendered.
    render(view(summary({ status: STATUS_ACTIVE })));
    await act(async () => {});
    expect(screen.getByText(STATUS_ACTIVE)).toBeTruthy();
    expect(refreshMock).not.toHaveBeenCalled();
  });

  it("a gate frame moves the pending count and touches nothing else", async () => {
    render(view(summary()));
    expect(screen.queryByRole("link", { name: /approval/i })).toBeNull();

    emit({ kind: FRAME_KIND.GATE_OPENED, gate_id: "g1", event_id: "evt_seed", pending_approvals: 2 });
    expect(await screen.findByRole("link", { name: /2 approvals waiting/i })).toBeTruthy();
    expect(screen.getByText("1,500")).toBeTruthy();

    emit({
      kind: FRAME_KIND.GATE_RESOLVED,
      gate_id: "g1",
      event_id: "evt_seed",
      status: "approved",
      resolved_by: "human:x",
      pending_approvals: 0,
    });
    await act(async () => {});
    expect(screen.queryByRole("link", { name: /approval/i })).toBeNull();
    expect(refreshMock).not.toHaveBeenCalled();
  });

  it("only a status change refreshes the server-rendered controls, once", async () => {
    render(view(summary()));

    emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "evt_live", actor: "cron:*" });
    emit(completion({ fleet_status: STATUS_PAUSED }));

    // The header's lifecycle controls render from the fleet status on the
    // server, so a run that paused the fleet re-runs that tree exactly once;
    // the strip itself shows the daemon's word at once.
    expect(await screen.findByText(STATUS_PAUSED)).toBeTruthy();
    expect(refreshMock).toHaveBeenCalledTimes(1);

    // A second completion with the same status is not a second refresh.
    emit(completion({ event_id: "evt_next", fleet_status: STATUS_PAUSED }));
    await act(async () => {});
    expect(refreshMock).toHaveBeenCalledTimes(1);
  });

  it("a server render resets the facts to its own — a kill from the header is not shadowed by an older frame", async () => {
    const { rerender } = render(view(summary()));
    emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "evt_live", actor: "cron:*" });
    emit(completion());
    expect(await screen.findByText("1,200")).toBeTruthy();

    // The kill switch refreshed the page: the server says killed, and the
    // figures it rendered are newer than the frame's.
    rerender(view(summary({ status: "killed", latest: { ...summary().latest!, tokens: RERENDERED_TOKENS } })));
    await act(async () => {});
    expect(screen.getByText("killed")).toBeTruthy();
    // No refresh: the server tree is what just rendered.
    expect(refreshMock).not.toHaveBeenCalled();
  });

  it("a thread read that failed leaves the strip unavailable until the stream delivers a row", async () => {
    render(view(summary({ latest: null, latestAvailable: false }), []));
    expect(screen.getByText(METRICS_UNAVAILABLE)).toBeTruthy();

    emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "evt_live", actor: "cron:*" });
    emit(completion());
    expect(await screen.findByText("1,200")).toBeTruthy();
    expect(screen.queryByText(METRICS_UNAVAILABLE)).toBeNull();
  });

  it("a reconnect backfill's terminal row moves the strip like a frame would", async () => {
    render(view(summary()));
    act(() => {
      reconcileServerRows(FLEET_ID, [turn({ event_id: "evt_missed", tokens: COMPLETED_TOKENS, created_at: SEED_AT + 1 })]);
    });
    expect(await screen.findByText("1,200")).toBeTruthy();
  });

  it("an operator's message still in flight is not the fleet's latest run", async () => {
    render(view(summary()));
    // The registry's own optimistic row: the composer's vocabulary, never a
    // server status — the strip keeps showing the newest server row.
    const { appendOptimistic } = await import("@/lib/streaming/fleet-stream-registry");
    act(() => {
      appendOptimistic(FLEET_ID, "deploy it", "steer:me");
    });
    await act(async () => {});
    expect(screen.getByText("1,500")).toBeTruthy();
  });
});
