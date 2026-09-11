import type { ProfilerOnRenderCallback } from "react";

import React, { Profiler } from "react";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  WorkspaceStreamProvider,
  useWorkspaceFleetStream,
} from "@/components/domain/useWorkspaceStream";
import { FRAME_KIND } from "@/lib/api/events-types";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { __resetWorkspaceRegistryForTests } from "@/lib/streaming/workspace-stream";
import { deriveTileLiveness } from "@/lib/wall/tile-liveness";

const WORKSPACE_ID = "ws_wall";
const FLEET_A = "fleet_a";
const FLEET_B = "fleet_b";
const BURST_FLEET_COUNT = 60;
const RECONNECT_DELAY_MS = 2_000;
const BACKFILL_PAGE_LIMIT = 200;
const EVENT_CREATED_AT_MS = 1_700_000_000_000;
const PROMISE_SETTLE_TURNS = 10;
const LIVE_LABEL = "live";
const LAST_KNOWN_LABEL = "last known";
const CURRENT_LABEL = "current";
const CATCHING_UP_LABEL = "catching up";
const FLEET_ACTOR = "fleet";
const ONE_EVENT_LABEL = "events:1";
const SNAPSHOT_KIND_LABEL = "kind:snapshot";
const LIVE_KIND_LABEL = "kind:live";
const COUNT_BEFORE_THE_DROP = 5;
const COUNT_AFTER_THE_DROP = 7;
const SPENT_NANOS = 2_000_000_000;

class FakeEventSource {
  static instances: FakeEventSource[] = [];
  onopen: ((this: EventSource, ev: Event) => unknown) | null = null;
  onmessage: ((this: EventSource, ev: MessageEvent) => unknown) | null = null;
  onerror: ((this: EventSource, ev: Event) => unknown) | null = null;
  listeners = new Map<string, Set<(event: Event) => unknown>>();
  closed = false;

  constructor(readonly url: string) {
    FakeEventSource.instances.push(this);
  }

  close() {
    this.closed = true;
  }

  addEventListener(name: string, listener: (event: Event) => unknown) {
    const handlers = this.listeners.get(name) ?? new Set();
    handlers.add(listener);
    this.listeners.set(name, handlers);
  }

  open() {
    this.onopen?.call(this as unknown as EventSource, {} as Event);
  }

  emit(payload: unknown, eventName?: string) {
    const parsed = typeof payload === "string" ? JSON.parse(payload) as unknown : payload;
    const resolvedEventName = eventName ?? (typeof parsed === "object" && parsed !== null && "kind" in parsed
      ? String((parsed as { kind: unknown }).kind)
      : "");
    const data = JSON.stringify(payload);
    const event = { data } as MessageEvent;
    for (const listener of this.listeners.get(resolvedEventName) ?? []) listener.call(this, event);
    if (!resolvedEventName) this.onmessage?.call(this as unknown as EventSource, event);
  }

  fail() {
    this.onerror?.call(this as unknown as EventSource, {} as Event);
  }
}

let animationFrameId = 0;
let animationFrames = new Map<number, FrameRequestCallback>();

beforeEach(() => {
  FakeEventSource.instances = [];
  animationFrameId = 0;
  animationFrames = new Map();
  vi.stubGlobal("EventSource", FakeEventSource);
  vi.stubGlobal(
    "requestAnimationFrame",
    vi.fn((callback: FrameRequestCallback) => {
      animationFrameId += 1;
      animationFrames.set(animationFrameId, callback);
      return animationFrameId;
    }),
  );
  vi.stubGlobal(
    "cancelAnimationFrame",
    vi.fn((id: number) => {
      animationFrames.delete(id);
    }),
  );
  __resetWorkspaceRegistryForTests();
});

afterEach(() => {
  cleanup();
  __resetWorkspaceRegistryForTests();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("workspace fleet wall provider", () => {
  it("opens one workspace stream and routes a tagged frame only to its fleet", () => {
    const view = renderWall([FLEET_A, FLEET_B]);
    const source = onlyEventSource();

    source.open();
    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A, FLEET_B] });
    flushAnimationFrame();
    source.emit(activityFrame(FLEET_A));
    flushAnimationFrame();

    expect(FakeEventSource.instances).toHaveLength(1);
    expect(source.url).toBe(`/live/v1/workspaces/${WORKSPACE_ID}/events/stream`);
    expect(view.getByTestId(FLEET_A).textContent).toContain(ONE_EVENT_LABEL);
    expect(view.getByTestId(FLEET_B).textContent).toContain("events:0");
  });

  it("coalesces a 60-fleet burst into one animation callback and one React commit", () => {
    const fleetIds = Array.from({ length: BURST_FLEET_COUNT }, (_, index) => `fleet_${index}`);
    const onRender = vi.fn<ProfilerOnRenderCallback>();
    renderWall(fleetIds, onRender);
    const source = onlyEventSource();

    source.open();
    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: fleetIds });
    flushAnimationFrame();
    const commitsBeforeBurst = onRender.mock.calls.length;
    vi.mocked(requestAnimationFrame).mockClear();

    for (const fleetId of fleetIds) source.emit(activityFrame(fleetId));
    expect(requestAnimationFrame).toHaveBeenCalledTimes(1);
    flushAnimationFrame();
    expect(onRender.mock.calls).toHaveLength(commitsBeforeBurst + 1);
  });

  it("marks fleet liveness from the server hello frame", () => {
    const view = renderWall([FLEET_A, FLEET_B]);
    const source = onlyEventSource();

    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    flushAnimationFrame();

    expect(view.getByTestId(FLEET_A).textContent).toContain(LIVE_LABEL);
    expect(view.getByTestId(FLEET_B).textContent).toContain(LAST_KNOWN_LABEL);
  });

  it("updates fleet liveness only when a later server hello announces the changed set", () => {
    const view = renderWall([FLEET_A, FLEET_B]);
    const source = onlyEventSource();

    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    flushAnimationFrame();
    expect(view.getByTestId(FLEET_B).textContent).toContain(LAST_KNOWN_LABEL);

    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A, FLEET_B] });
    flushAnimationFrame();
    expect(view.getByTestId(FLEET_B).textContent).toContain(LIVE_LABEL);
  });

  it("surfaces catching up until the server drop gap is backfilled", async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ items: [], next_cursor: null }),
    });
    vi.stubGlobal("fetch", fetchMock);
    const view = renderWall([FLEET_A]);
    const source = onlyEventSource();

    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    flushAnimationFrame();
    source.emit({ kind: FRAME_KIND.CATCHING_UP, dropped: 3 });
    source.emit({ kind: FRAME_KIND.CATCHING_UP, dropped: 4 });
    flushAnimationFrame();

    expect(view.getByTestId(FLEET_A).textContent).toContain(CATCHING_UP_LABEL);
    await act(async () => settlePromises());
    flushAnimationFrame();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(view.getByTestId(FLEET_A).textContent).toContain(CURRENT_LABEL);
  });

  it("keeps catching up visible when the server drop backfill fails", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 503 }));
    const view = renderWall([FLEET_A]);
    const source = onlyEventSource();

    source.emit({ kind: FRAME_KIND.CATCHING_UP, dropped: 4 });
    flushAnimationFrame();
    await act(async () => settlePromises());
    flushAnimationFrame();

    expect(view.getByTestId(FLEET_A).textContent).toContain(CATCHING_UP_LABEL);
    expect(warn).toHaveBeenCalledWith("fleet-stream backfill failed", "HTTP 503");
  });

  it("keeps catching up visible when the backfill request rejects", async () => {
    const failure = new Error("network failed");
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(failure));
    const view = renderWall([FLEET_A]);
    const source = onlyEventSource();

    source.emit({ kind: FRAME_KIND.CATCHING_UP, dropped: 5 });
    flushAnimationFrame();
    await act(async () => settlePromises());

    expect(view.getByTestId(FLEET_A).textContent).toContain(CATCHING_UP_LABEL);
    expect(warn).toHaveBeenCalledWith("fleet-stream backfill failed", failure);
  });

  it("recovers both fleets through one workspace backfill after reconnect", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        items: [
          eventRow(FLEET_A, "recovered_a"),
          eventRow(FLEET_B, "recovered_b"),
          eventRow("unsubscribed", "ignored"),
        ],
        next_cursor: null,
      }),
    });
    vi.stubGlobal("fetch", fetchMock);
    const view = renderWall([FLEET_A, FLEET_B]);
    const first = onlyEventSource();
    first.open();
    flushAnimationFrame();

    first.fail();
    await act(async () => vi.advanceTimersByTimeAsync(RECONNECT_DELAY_MS));
    const second = FakeEventSource.instances[1];
    if (!second) throw new Error("workspace stream did not reconnect");
    await act(async () => {
      second.open();
      await settlePromises();
    });
    flushAnimationFrame();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      `/live/v1/workspaces/${WORKSPACE_ID}/events?limit=${BACKFILL_PAGE_LIMIT}`,
    );
    expect(view.getByTestId(FLEET_A).textContent).toContain(ONE_EVENT_LABEL);
    expect(view.getByTestId(FLEET_B).textContent).toContain(ONE_EVENT_LABEL);
  });

  it("returns a stable empty state when a tile renders outside the provider", () => {
    const view = render(<FleetProbe fleetId={FLEET_A} />);

    expect(view.getByTestId(FLEET_A).textContent).toContain("events:0");
    expect(view.getByTestId(FLEET_A).textContent).toContain(CURRENT_LABEL);
    expect(FakeEventSource.instances).toHaveLength(0);
  });

  it("shares one cached fleet state across duplicate consumers", () => {
    const view = render(
      <WorkspaceStreamProvider workspaceId={WORKSPACE_ID} fleetIds={[FLEET_A]}>
        <FleetProbe fleetId={FLEET_A} />
        <FleetProbe fleetId={FLEET_A} />
      </WorkspaceStreamProvider>,
    );
    const source = onlyEventSource();

    source.emit(activityFrame(FLEET_A));
    flushAnimationFrame();

    expect(view.getAllByTestId(FLEET_A)).toHaveLength(2);
    expect(view.getAllByTestId(FLEET_A)[0]?.textContent).toContain(ONE_EVENT_LABEL);
  });

  it("skips a removed tile listener when its queued update flushes", () => {
    const fleetIds = [FLEET_A];
    const view = render(
      <WorkspaceStreamProvider workspaceId={WORKSPACE_ID} fleetIds={fleetIds}>
        <FleetProbe fleetId={FLEET_A} />
      </WorkspaceStreamProvider>,
    );
    const source = onlyEventSource();
    source.emit(activityFrame(FLEET_A));

    view.rerender(
      <WorkspaceStreamProvider workspaceId={WORKSPACE_ID} fleetIds={fleetIds}>
        {null}
      </WorkspaceStreamProvider>,
    );
    flushAnimationFrame();

    expect(view.queryByTestId(FLEET_A)).toBeNull();
  });

  it("a_dropped_frame_is_corrected_by_the_next_snapshot", () => {
    // Dimension 1.5. The frame carrying 6 never arrives; the one carrying 7
    // does, and the tile reads 7 — the database's figure — with no reload:
    // every frame carries the whole truth, so nothing owed the missing one.
    const view = renderWall([FLEET_A]);
    const source = onlyEventSource();
    source.open();
    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    source.emit(completionFrame(FLEET_A, "e5", COUNT_BEFORE_THE_DROP));
    flushAnimationFrame();
    expect(view.getByTestId(FLEET_A).textContent).toContain(`processed:${COUNT_BEFORE_THE_DROP}`);

    // e6's frame is the one transport lost.
    source.emit(completionFrame(FLEET_A, "e7", COUNT_AFTER_THE_DROP));
    flushAnimationFrame();

    expect(view.getByTestId(FLEET_A).textContent).toContain(`processed:${COUNT_AFTER_THE_DROP}`);
    expect(FakeEventSource.instances).toHaveLength(1);
  });

  it("a_capped_stream_degrades_to_a_snapshot_tile", () => {
    // Dimension 1.6, the client half. A stream the daemon refused at its
    // ceiling (`SSE_MAX_STREAMS`, `UZ-API-002` — the admission is pinned by
    // `afd_api/tests/fleet_streams.rs`) errors at the EventSource; the tile
    // must then say `snapshot`, never a stale `live`.
    const view = renderWall([FLEET_A]);
    const source = onlyEventSource();
    source.open();
    flushAnimationFrame();
    expect(view.getByTestId(FLEET_A).textContent).toContain(LIVE_KIND_LABEL);

    source.fail();
    flushAnimationFrame();

    expect(view.getByTestId(FLEET_A).textContent).toContain(SNAPSHOT_KIND_LABEL);
    expect(view.getByTestId(FLEET_A).textContent).not.toContain(LIVE_KIND_LABEL);
  });

  it("cancels a queued tile notification when the provider unmounts", () => {
    const view = renderWall([FLEET_A]);
    const source = onlyEventSource();
    source.emit(activityFrame(FLEET_A));

    view.unmount();

    expect(cancelAnimationFrame).toHaveBeenCalledTimes(1);
  });
});

function FleetProbe({ fleetId }: { fleetId: string }) {
  const state = useWorkspaceFleetStream(fleetId);
  const live = state.isLive ? LIVE_LABEL : LAST_KNOWN_LABEL;
  const recovery = state.catchingUp ? CATCHING_UP_LABEL : CURRENT_LABEL;
  const kind = deriveTileLiveness(AGENTSFLEET_STATUS.ACTIVE, state.connectionStatus).kind;
  const processed = state.counters?.eventsProcessed ?? "none";
  return React.createElement(
    "output",
    { "data-testid": fleetId },
    `events:${state.events.length} ${live} ${recovery} kind:${kind} processed:${processed}`,
  );
}

function renderWall(fleetIds: string[], onRender?: ProfilerOnRenderCallback) {
  const provider = (
    <WorkspaceStreamProvider workspaceId={WORKSPACE_ID} fleetIds={fleetIds}>
      {fleetIds.map((fleetId) => (
        <FleetProbe key={fleetId} fleetId={fleetId} />
      ))}
    </WorkspaceStreamProvider>
  );
  return render(onRender ? <Profiler id="fleet-wall" onRender={onRender}>{provider}</Profiler> : provider);
}

function activityFrame(fleetId: string) {
  return {
    fleet_id: fleetId,
    kind: FRAME_KIND.EVENT_RECEIVED,
    event_id: `event_${fleetId}`,
    actor: FLEET_ACTOR,
  };
}

function completionFrame(fleetId: string, eventId: string, eventsProcessed: number) {
  return {
    fleet_id: fleetId,
    kind: FRAME_KIND.EVENT_COMPLETE,
    event_id: eventId,
    status: "processed",
    created_at: EVENT_CREATED_AT_MS,
    updated_at: EVENT_CREATED_AT_MS,
    events_processed: eventsProcessed,
    budget_used_nanos: SPENT_NANOS,
  };
}

function eventRow(fleetId: string, eventId: string) {
  return {
    event_id: eventId,
    fleet_id: fleetId,
    actor: FLEET_ACTOR,
    response_text: "recovered",
    request_json: "{}",
    status: "processed",
    created_at: EVENT_CREATED_AT_MS,
  };
}

function onlyEventSource(): FakeEventSource {
  const source = FakeEventSource.instances[0];
  if (!source) throw new Error("workspace stream was not opened");
  return source;
}

function flushAnimationFrame() {
  const callbacks = [...animationFrames.values()];
  animationFrames.clear();
  act(() => {
    for (const callback of callbacks) callback(0);
  });
}

async function settlePromises() {
  for (let index = 0; index < PROMISE_SETTLE_TURNS; index += 1) await Promise.resolve();
}
