import type { EventRow, WorkspaceControlFrame, WorkspaceLiveFrame } from "@/lib/api/events";
import type { ConnectionStatus } from "@/lib/streaming/fleet-stream-registry";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import type { WorkspaceConnectionStatus } from "@/lib/streaming/workspace-stream";

import { FRAME_KIND } from "@/lib/api/events-types";
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

export type WorkspaceTileSnapshot = {
  events: FleetEvent[];
  connectionStatus: ConnectionStatus;
  helloReceived: boolean;
  isLive: boolean;
  catchingUp: boolean;
};

const EMPTY_TILE: WorkspaceTileSnapshot = Object.freeze({
  events: [],
  connectionStatus: CONNECTION_STATUS.CONNECTING,
  helloReceived: false,
  isLive: true,
  catchingUp: false,
});

export type WorkspaceStreamSnapshot = {
  connectionStatus: ConnectionStatus;
  helloReceived: boolean;
  catchingUp: boolean;
  countersStale: number;
};

const EMPTY_WORKSPACE: WorkspaceStreamSnapshot = Object.freeze({
  connectionStatus: CONNECTION_STATUS.CONNECTING,
  helloReceived: false,
  catchingUp: false,
  countersStale: 0,
});

export { EMPTY_TILE, EMPTY_WORKSPACE };

export class WorkspaceStore {
  readonly #workspaceId: string;
  #status: ConnectionStatus = CONNECTION_STATUS.CONNECTING;
  #helloReceived = false;
  #catchingUp = false;
  #liveFleetIds = new Set<string>();
  #eventsByFleet = new Map<string, FleetEvent[]>();
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
  #countersStale = 0;

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

  /**
   * The wall re-read these fleets, so the base it now renders already accounts
   * for every row streamed so far. Dropping them keeps the tile footer a sum
   * of base + rows-since-base instead of double-counting, and it is why no
   * timestamp watermark is needed: the hand-off is explicit.
   */
  absorb(fleetIds: readonly string[]) {
    for (const fleetId of fleetIds) {
      if (!this.#eventsByFleet.has(fleetId)) continue;
      this.#eventsByFleet.delete(fleetId);
      this.#snapshots.delete(fleetId);
      this.#dirtyFleetIds.add(fleetId);
    }
    this.#scheduleFlush();
  }

  /**
   * A tile could not price a settled row, so the frames alone cannot keep its
   * footer true. Bumping this asks the wall to re-read the server's counters
   * once; it is deliberately the only thing that does.
   */
  requestCounters() {
    this.#countersStale += 1;
    this.#invalidateWorkspace();
    this.#scheduleFlush();
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
      countersStale: this.#countersStale,
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
    } else {
      const catchingUp = frame.dropped > 0;
      if (catchingUp === this.#catchingUp) return;
      this.#catchingUp = catchingUp;
    }
    this.#notifySoon();
  }

  #applyFleetFrame(fleetId: string, frame: WorkspaceLiveFrame) {
    const events = applyLiveFrame(this.#eventsByFleet.get(fleetId) ?? [], frame);
    this.#eventsByFleet.set(fleetId, events);
    this.#notifySoon(fleetId);
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
      const events = mergeBackfill(this.#eventsByFleet.get(fleetId) ?? [], fleetRows);
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
