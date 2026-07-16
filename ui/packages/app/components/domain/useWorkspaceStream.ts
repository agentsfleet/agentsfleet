"use client";

import type { ReactNode } from "react";
import type { EventRow, WorkspaceControlFrame, WorkspaceLiveFrame } from "@/lib/api/events";
import type {
  ConnectionStatus,
  FleetEvent,
} from "@/lib/streaming/fleet-stream-registry";
import type { WorkspaceConnectionStatus } from "@/lib/streaming/workspace-stream";

import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useSyncExternalStore,
} from "react";
import { FRAME_KIND } from "@/lib/api/events";
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

type Listener = () => void;

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

const WorkspaceStreamContext = createContext<WorkspaceStore | null>(null);

export function WorkspaceStreamProvider({
  workspaceId,
  fleetIds,
  children,
}: {
  workspaceId: string;
  fleetIds: string[];
  children: ReactNode;
}) {
  const store = useMemo(() => new WorkspaceStore(workspaceId), [workspaceId]);
  const ids = useMemo(() => [...new Set(fleetIds)].sort(), [fleetIds]);

  useEffect(() => {
    return store.connect(ids);
  }, [store, ids]);

  return React.createElement(WorkspaceStreamContext.Provider, { value: store }, children);
}

export function useWorkspaceFleetStream(fleetId: string): WorkspaceTileSnapshot {
  const store = useContext(WorkspaceStreamContext);
  const subscribe = useCallback(
    (listener: Listener) => store?.subscribe(fleetId, listener) ?? (() => {}),
    [store, fleetId],
  );
  const getSnapshot = useCallback(() => store?.snapshot(fleetId) ?? EMPTY_TILE, [store, fleetId]);
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

class WorkspaceStore {
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

  #notifySoon(fleetId?: string) {
    if (fleetId === undefined) {
      this.#snapshots.clear();
      this.#notifyAll = true;
    } else {
      this.#snapshots.delete(fleetId);
      this.#dirtyFleetIds.add(fleetId);
    }
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
    this.#notifyAll = false;
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
