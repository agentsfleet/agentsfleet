import type {
  EventRow,
  WorkspaceControlFrame,
  WorkspaceLiveFrame,
} from "@/lib/api/events";
import type { ConnectionStatus } from "@/lib/streaming/fleet-stream-registry";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import type { WorkspaceConnectionStatus } from "@/lib/streaming/workspace-stream";
import type { TileCounters, WorkspaceTileSnapshot } from "@/lib/streaming/workspace-tile";

import { FRAME_KIND } from "@/lib/api/events-types";
import { capEvents } from "@/lib/streaming/fleet-stream-cap";
import { CONNECTION_STATUS } from "@/lib/streaming/fleet-stream-registry";
import {
  runWorkspaceBackfill,
  warnBackfillFailure,
} from "@/lib/streaming/fleet-stream-backfill";
import { applyLiveFrame, mergeBackfill } from "@/lib/streaming/fleet-stream-frames";
import {
  lastGreeting,
  noteServerFrameTime,
  subscribeFleet,
  subscribeStatus,
  subscribeWorkspaceFrames,
  WORKSPACE_CONNECTION_STATUS,
} from "@/lib/streaming/workspace-stream";
import { feedOf, isMidRunFrame, mergeCounters, sameTile } from "@/lib/streaming/workspace-tile";

export type Listener = () => void;
export type { TileCounters, WorkspaceTileSnapshot };

const EMPTY_TILE: WorkspaceTileSnapshot = Object.freeze({
  feed: undefined,
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

export { EMPTY_TILE, EMPTY_WORKSPACE };

export class WorkspaceStore {
  readonly #workspaceId: string;
  #status: ConnectionStatus = CONNECTION_STATUS.CONNECTING;
  #helloReceived = false;
  #catchingUp = false;
  // Every catching-up frame, counted. A walk clears the notice only when no gap
  // was reported after it started: a later gap queued one more walk, and the
  // notice is that walk's to clear.
  #gapsReported = 0;
  #liveFleetIds = new Set<string>();
  // Bounded on both axes: a key only for a subscribed fleet, since frames and
  // backfill rows reach the store through the subscriptions `connect` opened
  // and nothing else; and `capEvents` on every write, so a tab left open on a
  // busy fleet keeps a window rather than a history. The wall's own rows: they
  // order the feed and hold nothing a tile does not show, so a reply's chunks
  // and tool calls never land here once their row is open.
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
    // A wall that remounts inside the idle grace joins a connection whose
    // `hello` already went by, and takes the set it announced. A greeted store
    // is only re-subscribing for a changed fleet set and already has it.
    if (!this.#helloReceived) {
      const greeting = lastGreeting(this.#workspaceId);
      if (greeting) this.#applyWorkspaceFrame(greeting);
    }
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
    const next = this.#project(fleetId);
    this.#snapshots.set(fleetId, next);
    return next;
  }

  #project(fleetId: string): WorkspaceTileSnapshot {
    return {
      feed: feedOf(this.#eventsByFleet.get(fleetId)),
      connectionStatus: this.#status,
      helloReceived: this.#helloReceived,
      isLive: !this.#helloReceived || this.#liveFleetIds.has(fleetId),
      catchingUp: this.#catchingUp,
      counters: this.#countersByFleet.get(fleetId),
    };
  }

  // A tile is told only when what it shows moved; otherwise it keeps the
  // snapshot it holds, and `useSyncExternalStore` renders nothing.
  #retile(fleetId: string) {
    const cached = this.#snapshots.get(fleetId);
    if (cached !== undefined && sameTile(cached, this.#project(fleetId))) return;
    this.#notifySoon(fleetId);
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
      // Only subscribed fleets are kept — the map is bounded by the tiles on
      // screen, never by what a payload chose to name.
      const carried: unknown = frame.counters;
      if (typeof carried === "object" && carried !== null) {
        for (const [fleetId, snapshot] of Object.entries(carried)) {
          if (this.#subscribedFleetIds.has(fleetId)) this.#assignCounters(fleetId, snapshot);
        }
      }
    } else {
      // Every catching-up frame is a gap: frames dropped behind a slow tab, or
      // a subscription lost and re-established, whose missed count is
      // unknowable and arrives as 0. The backfill it starts, or the next
      // greeting, clears it.
      this.#gapsReported += 1;
      if (this.#catchingUp) return;
      this.#catchingUp = true;
    }
    this.#notifySoon();
  }

  #applyFleetFrame(fleetId: string, frame: WorkspaceLiveFrame) {
    // A tile changes when work starts: a message still waiting for a runner
    // moves nothing a tile shows, and its received frame follows.
    if (frame.kind === FRAME_KIND.EVENT_ADMITTED) return;
    const held = this.#eventsByFleet.get(fleetId) ?? [];
    // A chunk or a tool call for a row the wall already holds changes nothing
    // a tile shows. Anything else is folded as before: a chunk for a row the
    // wall has not seen opens it, so the feed line reads as it always did.
    if (isMidRunFrame(frame) && held.some((event) => event.id === frame.event_id)) return;
    this.#eventsByFleet.set(fleetId, capEvents(applyLiveFrame(held, frame)));
    // Only the four daemon-authored frames carry the snapshot; a runner's
    // mid-run frame says nothing about where the fleet stands.
    if ("events_processed" in frame || "budget_used_nanos" in frame) {
      this.#assignCounters(fleetId, frame);
    }
    this.#retile(fleetId);
  }

  #assignCounters(fleetId: string, carried: unknown) {
    const standing = this.#countersByFleet.get(fleetId);
    const merged = mergeCounters(standing, carried);
    if (merged !== undefined && merged !== standing) this.#countersByFleet.set(fleetId, merged);
  }

  async #backfill(workspaceId: string, anchorMs: number | null, generation: number) {
    const gapsAtStart = this.#gapsReported;
    try {
      const outcome = await runWorkspaceBackfill({
        workspaceId,
        anchorMs,
        stillCurrent: () => this.#generation === generation,
        onPage: (rows) => this.#applyBackfillRows(rows),
      });
      if (outcome.ok && this.#generation === generation) {
        if (outcome.watermark !== null) noteServerFrameTime(workspaceId, outcome.watermark);
        if (this.#catchingUp && this.#gapsReported === gapsAtStart) {
          this.#catchingUp = false;
          this.#notifySoon();
        }
      }
    } catch (error) {
      warnBackfillFailure(error);
    }
  }

  #applyBackfillRows(rows: EventRow[]) {
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
      this.#retile(fleetId);
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
    case WORKSPACE_CONNECTION_STATUS.REVOKED:
      return CONNECTION_STATUS.REVOKED;
  }
}
