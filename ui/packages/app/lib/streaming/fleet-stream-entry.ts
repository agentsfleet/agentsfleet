// The entry model behind the fleet-stream registry: one Entry per fleet, its
// published snapshot shape, and the factory that seeds a fresh entry from
// server-rendered rows. Data and construction only — the registry owns the
// map, the EventSource lifecycle, and every mutation. Split out for the same
// reason as `fleet-stream-frames.ts`: the registry's lifecycle file stays
// under the length gate and the model stays readable on its own.

import type { EventRow } from "@/lib/api/events";
import {
  maxServerCreatedAt,
  mergeBackfill,
  type FleetEvent,
} from "./fleet-stream-frames";
import type { InstallStepId } from "./install-steps";

export const CONNECTION_STATUS = {
  CONNECTING: "connecting",
  LIVE: "live",
  RECONNECTING: "reconnecting",
  OFFLINE: "offline",
} as const;
export type ConnectionStatus =
  (typeof CONNECTION_STATUS)[keyof typeof CONNECTION_STATUS];

export type FleetStreamSnapshot = {
  events: FleetEvent[];
  connectionStatus: ConnectionStatus;
  // The latest install step advanced by an `install:*` frame, or null when no
  // install frame has arrived (a non-installing fleet, or pre-first-frame). The
  // InstallStates surface reads this to advance its rendered step and to detect
  // the installing→active flip; the chat path ignores it.
  installStep: InstallStepId | null;
};

export type Listener = () => void;

export type Entry = {
  workspaceId: string;
  snapshot: FleetStreamSnapshot;
  listeners: Set<Listener>;
  refCount: number;
  eventSource: EventSource | null;
  reconnectTimer: ReturnType<typeof setTimeout> | null;
  reconnectAttempts: number;
  idleTimer: ReturnType<typeof setTimeout> | null;
  // Whether this entry's EventSource has ever reached onopen. Distinguishes
  // the initial (SSR-seeded) connect from a reconnect — only the latter
  // backfills. Lives on the mutable Entry, never a passed primitive.
  hasConnectedOnce: boolean;
  // An outage before the first successful open still creates a history gap.
  // The next open backfills even though `hasConnectedOnce` is still false.
  hadConnectionError: boolean;
  // Newest server-confirmed created_at (epoch ms) — advanced only by the SSR
  // seed and successful backfill pages, never by client-clock-stamped live or
  // optimistic rows, and never by a failed backfill (so a failure cannot seal
  // the gap it left).
  serverSinceMs: number | null;
  backfillInFlight: boolean;
  // Detaches the tab-visible / network-online recovery listeners. Held on the
  // entry so teardown can remove exactly what subscribe attached.
  detachRecovery: (() => void) | null;
};

export const EMPTY_SNAPSHOT: FleetStreamSnapshot = Object.freeze({
  events: [],
  connectionStatus: CONNECTION_STATUS.CONNECTING,
  installStep: null,
}) as FleetStreamSnapshot;

export function createEntry(workspaceId: string, initial: EventRow[]): Entry {
  return {
    workspaceId,
    snapshot: {
      events: mergeBackfill([], initial),
      connectionStatus: CONNECTION_STATUS.CONNECTING,
      installStep: null,
    },
    listeners: new Set(),
    refCount: 0,
    eventSource: null,
    reconnectTimer: null,
    reconnectAttempts: 0,
    idleTimer: null,
    hasConnectedOnce: false,
    hadConnectionError: false,
    serverSinceMs: maxServerCreatedAt(null, initial),
    backfillInFlight: false,
    detachRecovery: null,
  };
}
