"use client";

import { useCallback, useRef, useSyncExternalStore } from "react";
import type { ThreadMessageLike } from "@assistant-ui/react";
import type { EventRow } from "@/lib/api/events";
import {
  appendOptimistic as registryAppendOptimistic,
  CONNECTION_STATUS,
  getSnapshot,
  markOptimisticFailed as registryMarkOptimisticFailed,
  reconcileOptimistic as registryReconcileOptimistic,
  subscribe,
  type ConnectionStatus,
  type FleetEvent,
  type FleetEventStatus,
} from "@/lib/streaming/fleet-stream-registry";

// Public re-exports so existing consumers keep their import surface.
export {
  CONNECTION_STATUS,
  type ConnectionStatus,
  type FleetEvent,
  type FleetEventStatus,
};

export type UseFleetEventStreamResult = {
  events: FleetEvent[];
  connectionStatus: ConnectionStatus;
  isRunning: boolean;
  appendOptimistic: (text: string, actor: string) => string;
  reconcileOptimistic: (tempId: string, realEventId: string) => void;
  markOptimisticFailed: (tempId: string) => void;
  convertEvent: (event: FleetEvent) => ThreadMessageLike;
};

/**
 * React boundary over the module-level fleet-stream registry. Multiple
 * mounts of this hook for the same `fleetId` share one EventSource — and
 * the connection survives a /dashboard ↔ /fleets/[id] round-trip up to
 * the registry's idle release window.
 *
 * `initial` seeds the first subscriber's event list from server-rendered
 * data; the browser holds no token. Live updates arrive over the
 * cookie-authed SSE route handler. Later re-renders do not re-seed an
 * existing subscription (that would clobber live frames with stale data).
 */
export function useFleetEventStream(
  workspaceId: string,
  fleetId: string,
  initial: EventRow[],
): UseFleetEventStreamResult {
  // Hold the latest `initial` without making it a `subscribe` dependency:
  // a fresh array identity each render must not resubscribe. Updating the
  // ref every render (rather than capturing first-render only) keeps the
  // value current if `fleetId` changes within a live instance — the
  // registry ignores `initial` for an existing entry, so seed-once holds.
  const initialRef = useRef(initial);
  initialRef.current = initial;
  const subscribeFn = useCallback(
    (listener: () => void) =>
      subscribe(workspaceId, fleetId, initialRef.current, listener),
    [workspaceId, fleetId],
  );
  const snapshotFn = useCallback(() => getSnapshot(fleetId), [fleetId]);
  const snapshot = useSyncExternalStore(subscribeFn, snapshotFn, snapshotFn);

  const appendOptimistic = useCallback(
    (text: string, actor: string) =>
      registryAppendOptimistic(fleetId, text, actor),
    [fleetId],
  );
  const reconcileOptimistic = useCallback(
    (tempId: string, realEventId: string) =>
      registryReconcileOptimistic(fleetId, tempId, realEventId),
    [fleetId],
  );
  const markOptimisticFailed = useCallback(
    (tempId: string) => registryMarkOptimisticFailed(fleetId, tempId),
    [fleetId],
  );

  return {
    events: snapshot.events,
    connectionStatus: snapshot.connectionStatus,
    isRunning: snapshot.events.some((ev) => ev.status === "received"),
    appendOptimistic,
    reconcileOptimistic,
    markOptimisticFailed,
    convertEvent,
  };
}

function convertEvent(event: FleetEvent): ThreadMessageLike {
  return {
    role: event.role,
    id: event.id,
    createdAt: event.createdAt,
    content: [{ type: "text", text: event.text }],
    metadata: {
      custom: {
        actor: event.actor,
        requestJson: event.custom?.requestJson,
        reason: event.custom?.reason,
        status: event.status,
      },
    },
  };
}
