import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";
import { FRAME_KIND, type EventRow, type LiveFrame } from "@/lib/api/events";
import type { FleetRunSummary } from "@/lib/events/run-summary";
import { __resetRegistryForTests } from "@/lib/streaming/fleet-stream-registry";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import { useFleetRunSummary } from "./useFleetRunSummary";

// The hook as a render budget: what it costs the component that calls it. The
// strip sits over a streaming reply, and every chunk of that reply rebuilds
// the registry's rows — the hook must not pass that on.

const { refreshMock } = vi.hoisted(() => ({ refreshMock: vi.fn() }));
vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: refreshMock }) }));

const WORKSPACE_ID = "ws_1";
const FLEET_ID = "agt_1";
const SEED_AT = Date.UTC(2026, 4, 15, 18, 30, 0);
const CHUNKS = 40;

function row(): EventRow {
  return {
    event_id: "evt_seed",
    fleet_id: FLEET_ID,
    workspace_id: WORKSPACE_ID,
    actor: "cron:*",
    event_type: "cron",
    status: "processed",
    tokens: 1500,
    wall_ms: 10,
    failure_label: null,
    failure_detail: null,
    checkpoint_id: null,
    resumes_event_id: null,
    cost_nanos: null,
    created_at: SEED_AT,
    updated_at: SEED_AT,
  };
}

const INITIAL: FleetRunSummary = {
  status: "active",
  latest: null,
  latestAvailable: true,
  pendingApprovals: 0,
};

let renders = 0;
let seen: FleetRunSummary | null = null;

// The summary is built once per render pass and handed down, the way a
// Server Component's props reach a client leaf: one identity until the next
// server render.
function Harness({ summary = INITIAL }: { summary?: FleetRunSummary }) {
  renders += 1;
  seen = useFleetRunSummary(WORKSPACE_ID, FLEET_ID, [row()], summary);
  return null;
}

function emit(frame: LiveFrame) {
  act(() => {
    FakeEventSource.instances[0]!.emit(frame);
  });
}

beforeEach(() => {
  renders = 0;
  seen = null;
  refreshMock.mockReset();
  FakeEventSource.install();
  __resetRegistryForTests();
});
afterEach(() => {
  cleanup();
  __resetRegistryForTests();
  FakeEventSource.uninstall();
});

describe("useFleetRunSummary — what a streaming reply costs the strip", () => {
  it("the chunks of a reply re-render nothing; the completion re-renders once", () => {
    render(React.createElement(Harness));
    emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "evt_live", actor: "cron:*", created_at: SEED_AT + 1 });
    const settled = renders;

    for (let i = 0; i < CHUNKS; i += 1) {
      emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_live", text: "word " });
    }
    expect(renders).toBe(settled);

    emit({
      kind: FRAME_KIND.EVENT_COMPLETE,
      ...row(),
      event_id: "evt_live",
      tokens: 1200,
      created_at: SEED_AT + 1,
      fleet_status: "active",
      pending_approvals: 0,
    });
    expect(renders).toBe(settled + 1);
    expect(seen?.latest).toMatchObject({ tokens: 1200 });
    expect(refreshMock).not.toHaveBeenCalled();
  });

  it("a render the strip asked for does not roll back a frame that landed during it", () => {
    const view = render(React.createElement(Harness));
    emit({
      kind: FRAME_KIND.EVENT_COMPLETE,
      ...row(),
      event_id: "evt_live",
      created_at: SEED_AT + 1,
      fleet_status: "paused",
      pending_approvals: 0,
    });
    expect(refreshMock).toHaveBeenCalledTimes(1);
    // A gate opens while the round trip is in flight …
    emit({ kind: FRAME_KIND.GATE_OPENED, gate_id: "g", event_id: "evt_live", pending_approvals: 1 });
    // … and the render lands carrying the count the server read before it.
    const landed: FleetRunSummary = { ...INITIAL, status: "paused", pendingApprovals: 0 };
    view.rerender(React.createElement(Harness, { summary: landed }));
    expect(seen?.pendingApprovals).toBe(1);
    expect(seen?.status).toBe("paused");
    expect(refreshMock).toHaveBeenCalledTimes(1);

    // The next server render — the kill switch's own, with nothing newer on
    // the tail — lands as the server wrote it.
    const killed: FleetRunSummary = { ...INITIAL, status: "killed", pendingApprovals: 0 };
    view.rerender(React.createElement(Harness, { summary: killed }));
    expect(seen?.pendingApprovals).toBe(0);
    expect(seen?.status).toBe("killed");
    expect(refreshMock).toHaveBeenCalledTimes(1);
  });

  it("a gate frame that restates the count re-renders nothing", () => {
    render(React.createElement(Harness));
    emit({ kind: FRAME_KIND.GATE_OPENED, gate_id: "g", event_id: "evt_seed", pending_approvals: 1 });
    const settled = renders;
    emit({ kind: FRAME_KIND.GATE_OPENED, gate_id: "g2", event_id: "evt_seed", pending_approvals: 1 });
    expect(renders).toBe(settled);
    expect(seen?.pendingApprovals).toBe(1);
  });
});
