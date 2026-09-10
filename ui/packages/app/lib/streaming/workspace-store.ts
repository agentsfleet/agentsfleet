import type {
  EventRow,
  FleetCountersSnapshot,
  WorkspaceControlFrame,
  WorkspaceLiveFrame,
} from "@/lib/api/events";
import type { ConnectionStatus } from "@/lib/streaming/fleet-stream-registry";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import type { WorkspaceConnectionStatus } from "@/lib/streaming/workspace-stream";

import { FRAME_KIND } from "@/lib/api/events-types";
import { capEvents } from "@/lib/streaming/fleet-stream-cap";
import { CONNECTION_STATUS } from "@/lib/streaming/fleet-stream-registry";
import {
  runWorkspaceBackfill,
  warnBackfillFailure,
} from "@/lib/streaming/fleet-stream-backfill";
import { applyLiveFrame, mergeBackfill } from "@/lib/streaming/fleet-stream-frames";
import {
  noteServerFrameTime,
  subscribeFleet,
  subscribeStatus,
  subscribeWorkspaceFrames,
  WORKSPACE_CONNECTION_STATUS,
} from "@/lib/streaming/workspace-stream";

export type Listener = () => void;

/**
 * The tile footer: where the fleet stands, as the last frame said.
 *
 * Server truth, assigned from whichever frame arrived last and never added
 * to. `undefined` until the stream has said anything about the fleet, so the
 * tile keeps rendering the server-rendered figures rather than a zero.
 */
export type TileCounters = {
  spentNanos: number;
  eventsProcessed: number;
};

export type WorkspaceTileSnapshot = {
  events: FleetEvent[];
  connectionStatus: ConnectionStatus;
  helloReceived: boolean;
  isLive: boolean;
  catchingUp: boolean;
  counters: TileCounters | undefined;
};

const EMPTY_TILE: WorkspaceTileSnapshot = Object.freeze({
  events: [],
  connectionStatus: CONNECTION_STATUS.CONNECTING,
  helloReceived: false,
  isLive: true,
  catchingUp: false,
  counters: undefined,
});

export type WorkspaceStreamSnapshot = {
  connectionStatus: ConnectionStatus;
  helloReceived: boolean;
  catchingUp: boolean;
};

const EMPTY_WORKSPACE: WorkspaceStreamSnapshot = Object.freeze({
  connectionStatus: CONNECTION_STATUS.CONNECTING,
  helloReceived: false,
  catchingUp: false,
});

/**
 * The snapshot a frame carries, or nothing. Both fields or neither: the
 * daemon flattens an optional pair, so a frame with one number and not the
 * other is malformed and is treated as carrying none — the figures already
 * standing stay, and nothing is guessed.
 */
function readCounters(carried: FleetCountersSnapshot): TileCounters | undefined {
  const { events_processed: eventsProcessed, budget_used_nanos: spentNanos } = carried;
  if (typeof eventsProcessed !== "number" || typeof spentNanos !== "number") return undefined;
  return { eventsProcessed, spentNanos };
}

export { EMPTY_TILE, EMPTY_WORKSPACE };

export class WorkspaceStore {
  readonly #workspaceId: string;
  #status: ConnectionStatus = CONNECTION_STATUS.CONNECTING;
  #helloReceived = false;
  #catchingUp = false;
  #liveFleetIds = new Set<string>();
  // Bounded on both axes: a key only for a subscribed fleet, since frames and
  // backfill rows reach the store through the subscriptions `connect` opened
  // and nothing else; and `capEvents` on every write, so a tab left open on a
  // busy fleet keeps a window rather than a history.
  #eventsByFleet = new Map<string, FleetEvent[]>();
  #countersByFleet = new Map<string, TileCounters>();
  #snapshots = new Map<string, WorkspaceTileSnapshot>();
  #listenersByFleet = new Map<string, Set<Listener>>();
  #dirtyFleetIds = new Set<string>();
  #notifyFrame: number | null = null;
  #notifyAll = false;
  #generation = 0;
  #subscribedFleetIds = new Set<string>();
  #workspaceListeners = new Set<Listener>();
  #workspaceSnapshot: WorkspaceStreamSnapshot | null = null;
  #workspaceDirty = false;

  constructor(workspaceId: string) {
    this.#workspaceId = workspaceId;
  }

  connect(fleetIds: string[]) {
    const generation = ++this.#generation;
    this.#subscribedFleetIds = new Set(fleetIds);
    const backfill = (workspaceId: string, anchorMs: number | null) =>
      this.#backfill(workspaceId, anchorMs, generation);
    const unsubs = [
      subscribeStatus(this.#workspaceId, (next) => this.#setStatus(next), backfill),
      subscribeWorkspaceFrames(this.#workspaceId, (frame) => this.#applyWorkspaceFrame(frame)),
      ...fleetIds.map((fleetId) =>
        subscribeFleet(this.#workspaceId, fleetId, (frame) => this.#applyFleetFrame(fleetId, frame)),
      ),
    ];
    return () => {
      for (const unsub of unsubs) unsub();
      this.#generation += 1;
      this.#cancelNotification();
    };
  }

  subscribe(fleetId: string, listener: Listener) {
    let listeners = this.#listenersByFleet.get(fleetId);
    if (!listeners) {
      listeners = new Set();
      this.#listenersByFleet.set(fleetId, listeners);
    }
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
      if (listeners.size === 0) this.#listenersByFleet.delete(fleetId);
    };
  }

  subscribeWorkspace(listener: Listener) {
    this.#workspaceListeners.add(listener);
    return () => {
      this.#workspaceListeners.delete(listener);
    };
  }

  // Cached like `snapshot`, and for the same reason: `useSyncExternalStore`
  // re-renders on snapshot IDENTITY, so a fresh object per call would spin.
  workspaceSnapshot(): WorkspaceStreamSnapshot {
    const cached = this.#workspaceSnapshot;
    if (cached) return cached;
    const next: WorkspaceStreamSnapshot = {
      connectionStatus: this.#status,
      helloReceived: this.#helloReceived,
      catchingUp: this.#catchingUp,
    };
    this.#workspaceSnapshot = next;
    return next;
  }

  snapshot(fleetId: string): WorkspaceTileSnapshot {
    const cached = this.#snapshots.get(fleetId);
    if (cached) return cached;
    const next: WorkspaceTileSnapshot = {
      events: this.#eventsByFleet.get(fleetId) ?? [],
      connectionStatus: this.#status,
      helloReceived: this.#helloReceived,
      isLive: !this.#helloReceived || this.#liveFleetIds.has(fleetId),
      catchingUp: this.#catchingUp,
      counters: this.#countersByFleet.get(fleetId),
    };
    this.#snapshots.set(fleetId, next);
    return next;
  }

  #invalidateWorkspace() {
    this.#workspaceSnapshot = null;
    this.#workspaceDirty = true;
  }

  #notifySoon(fleetId?: string) {
    if (fleetId === undefined) {
      this.#snapshots.clear();
      this.#notifyAll = true;
      // A connection or hello/catching-up change is the wall's business too.
      this.#invalidateWorkspace();
    } else {
      this.#snapshots.delete(fleetId);
      this.#dirtyFleetIds.add(fleetId);
    }
    this.#scheduleFlush();
  }

  // One coalesced flush per animation frame, however many frames landed in it.
  #scheduleFlush() {
    if (this.#notifyFrame !== null) return;
    this.#notifyFrame = requestAnimationFrame(() => this.#flushNotifications());
  }

  #flushNotifications() {
    this.#notifyFrame = null;
    if (this.#notifyAll) {
      for (const listeners of this.#listenersByFleet.values()) {
        for (const listener of listeners) listener();
      }
    } else {
      for (const fleetId of this.#dirtyFleetIds) {
        for (const listener of this.#listenersByFleet.get(fleetId) ?? []) {
          listener();
        }
      }
    }
    if (this.#workspaceDirty) {
      for (const listener of this.#workspaceListeners) listener();
    }
    this.#notifyAll = false;
    this.#workspaceDirty = false;
    this.#dirtyFleetIds.clear();
  }

  #setStatus(next: WorkspaceConnectionStatus) {
    const status = toConnectionStatus(next);
    if (status === this.#status) return;
    this.#status = status;
    this.#notifySoon();
  }

  #applyWorkspaceFrame(frame: WorkspaceControlFrame) {
    if (frame.kind === FRAME_KIND.HELLO) {
      this.#helloReceived = true;
      this.#liveFleetIds = new Set(frame.fleet_ids);
      this.#catchingUp = false;
      // The set arrives with where each fleet stands, so a subscriber that
      // came late is right before its first event frame. A fleet the map
      // omits keeps whatever it had: the server chose silence over a guess.
      for (const [fleetId, carried] of Object.entries(frame.counters ?? {})) {
        this.#assignCounters(fleetId, carried);
      }
    } else {
      const catchingUp = frame.dropped > 0;
      if (catchingUp === this.#catchingUp) return;
      this.#catchingUp = catchingUp;
    }
    this.#notifySoon();
  }

  #applyFleetFrame(fleetId: string, frame: WorkspaceLiveFrame) {
    const events = capEvents(applyLiveFrame(this.#eventsByFleet.get(fleetId) ?? [], frame));
    this.#eventsByFleet.set(fleetId, events);
    // Only the four daemon-authored frames carry the snapshot; a runner's
    // mid-run frame says nothing about where the fleet stands.
    if ("events_processed" in frame || "budget_used_nanos" in frame) {
      this.#assignCounters(fleetId, frame);
    }
    this.#notifySoon(fleetId);
  }

  // ASSIGNED, never added to. Every daemon frame carries the whole truth, so
  // the same frame twice, or one arriving late, leaves the tile where the
  // newest snapshot put it — the reason no frame owns an increment.
  #assignCounters(fleetId: string, carried: FleetCountersSnapshot) {
    const counters = readCounters(carried);
    if (counters === undefined) return;
    this.#countersByFleet.set(fleetId, counters);
  }

  async #backfill(workspaceId: string, anchorMs: number | null, generation: number) {
    try {
      const outcome = await runWorkspaceBackfill({
        workspaceId,
        anchorMs,
        stillCurrent: () => this.#generation === generation,
        onPage: (rows) => this.#applyBackfillPage(rows),
      });
      if (outcome.ok && this.#generation === generation) {
        if (outcome.watermark !== null) noteServerFrameTime(workspaceId, outcome.watermark);
        if (this.#catchingUp) {
          this.#catchingUp = false;
          this.#notifySoon();
        }
      }
    } catch (error) {
      warnBackfillFailure(error);
    }
  }

  #applyBackfillPage(rows: EventRow[]) {
    const rowsByFleet = new Map<string, EventRow[]>();
    for (const row of rows) {
      if (!this.#subscribedFleetIds.has(row.fleet_id)) continue;
      const fleetRows = rowsByFleet.get(row.fleet_id) ?? [];
      fleetRows.push(row);
      rowsByFleet.set(row.fleet_id, fleetRows);
    }
    for (const [fleetId, fleetRows] of rowsByFleet) {
      const events = capEvents(mergeBackfill(this.#eventsByFleet.get(fleetId) ?? [], fleetRows));
      this.#eventsByFleet.set(fleetId, events);
      this.#notifySoon(fleetId);
    }
  }

  #cancelNotification() {
    if (this.#notifyFrame !== null) cancelAnimationFrame(this.#notifyFrame);
    this.#notifyFrame = null;
    this.#notifyAll = false;
    this.#dirtyFleetIds.clear();
  }
}

function toConnectionStatus(status: WorkspaceConnectionStatus): ConnectionStatus {
  switch (status) {
    case WORKSPACE_CONNECTION_STATUS.LIVE:
      return CONNECTION_STATUS.LIVE;
    case WORKSPACE_CONNECTION_STATUS.RECONNECTING:
      return CONNECTION_STATUS.RECONNECTING;
    case WORKSPACE_CONNECTION_STATUS.CONNECTING:
      return CONNECTION_STATUS.CONNECTING;
  }
}
