import type { ProfilerOnRenderCallback } from "react";

import React, { Profiler } from "react";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, vi } from "vitest";
import {
  WorkspaceStreamProvider,
  useWorkspaceFleetStream,
} from "@/components/domain/useWorkspaceStream";
import { FRAME_KIND } from "@/lib/api/events-types";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { __resetWorkspaceRegistryForTests } from "@/lib/streaming/workspace-stream";
import { deriveTileLiveness } from "@/lib/wall/tile-liveness";

// The wall provider driven through its real workspace stream: a fake
// EventSource, a manual animation frame, and a probe that prints what a tile
// reads. Split from the wall suite so the suite keeps to its behaviour.

export const WORKSPACE_ID = "ws_wall";
export const EVENT_CREATED_AT_MS = 1_700_000_000_000;
export const SPENT_NANOS = 2_000_000_000;
export const LIVE_LABEL = "live";
export const LAST_KNOWN_LABEL = "last known";
export const CURRENT_LABEL = "current";
export const CATCHING_UP_LABEL = "catching up";
const PROMISE_SETTLE_TURNS = 10;
const FLEET_ACTOR = "fleet";
const SETTLED = "processed";
// What the probe prints for a figure or a feed line the stream has not given.
const ABSENT = "none";
const FEED_SHOWN = "shown";
/** The probe's words for a tile that has, and has not, a feed line to show. */
export const FEED_SHOWN_LABEL = `feed:${FEED_SHOWN}`;
export const NO_FEED_LABEL = `feed:${ABSENT}`;

let animationFrameId = 0;
let animationFrames = new Map<number, FrameRequestCallback>();

export class FakeEventSource {
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

export function setupWallTests(): void {
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
}

export function FleetProbe({ fleetId }: { fleetId: string }) {
  const state = useWorkspaceFleetStream(fleetId);
  const live = state.isLive ? LIVE_LABEL : LAST_KNOWN_LABEL;
  const recovery = state.catchingUp ? CATCHING_UP_LABEL : CURRENT_LABEL;
  const kind = deriveTileLiveness(AGENTSFLEET_STATUS.ACTIVE, state.connectionStatus).kind;
  const processed = state.counters?.eventsProcessed ?? ABSENT;
  return React.createElement(
    "output",
    { "data-testid": fleetId },
    `feed:${state.feed === undefined ? ABSENT : FEED_SHOWN} ${live} ${recovery} kind:${kind} processed:${processed}`,
  );
}

export function renderWall(fleetIds: string[], onRender?: ProfilerOnRenderCallback) {
  const provider = (
    <WorkspaceStreamProvider workspaceId={WORKSPACE_ID} fleetIds={fleetIds}>
      {fleetIds.map((fleetId) => (
        <FleetProbe key={fleetId} fleetId={fleetId} />
      ))}
    </WorkspaceStreamProvider>
  );
  return render(onRender ? <Profiler id="fleet-wall" onRender={onRender}>{provider}</Profiler> : provider);
}

export function activityFrame(fleetId: string) {
  return {
    fleet_id: fleetId,
    kind: FRAME_KIND.EVENT_RECEIVED,
    event_id: `event_${fleetId}`,
    actor: FLEET_ACTOR,
  };
}

export function completionFrame(fleetId: string, eventId: string, eventsProcessed: number) {
  return {
    fleet_id: fleetId,
    kind: FRAME_KIND.EVENT_COMPLETE,
    event_id: eventId,
    status: SETTLED,
    created_at: EVENT_CREATED_AT_MS,
    updated_at: EVENT_CREATED_AT_MS,
    events_processed: eventsProcessed,
    budget_used_nanos: SPENT_NANOS,
  };
}

export function eventRow(fleetId: string, eventId: string) {
  return {
    event_id: eventId,
    fleet_id: fleetId,
    actor: FLEET_ACTOR,
    response_text: "recovered",
    request_json: "{}",
    status: SETTLED,
    created_at: EVENT_CREATED_AT_MS,
  };
}

export function onlyEventSource(): FakeEventSource {
  const source = FakeEventSource.instances[0];
  if (!source) throw new Error("workspace stream was not opened");
  return source;
}

export function flushAnimationFrame() {
  const callbacks = [...animationFrames.values()];
  animationFrames.clear();
  act(() => {
    for (const callback of callbacks) callback(0);
  });
}

export async function settlePromises() {
  for (let index = 0; index < PROMISE_SETTLE_TURNS; index += 1) await Promise.resolve();
}
