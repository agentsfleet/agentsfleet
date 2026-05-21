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
  type ZombieEvent,
  type ZombieEventStatus,
} from "@/lib/streaming/zombie-stream-registry";

// Public re-exports so existing consumers keep their import surface.
export {
  CONNECTION_STATUS,
  type ConnectionStatus,
  type ZombieEvent,
  type ZombieEventStatus,
};

export type UseZombieEventStreamResult = {
  events: ZombieEvent[];
  connectionStatus: ConnectionStatus;
  isRunning: boolean;
  appendOptimistic: (text: string, actor: string) => string;
  reconcileOptimistic: (tempId: string, realEventId: string) => void;
  markOptimisticFailed: (tempId: string) => void;
  convertEvent: (event: ZombieEvent) => ThreadMessageLike;
};

/**
 * React boundary over the module-level zombie-stream registry. Multiple
 * mounts of this hook for the same `zombieId` share one EventSource — and
 * the connection survives a /dashboard ↔ /zombies/[id] round-trip up to
 * the registry's idle release window.
 *
 * `initial` seeds the first subscriber's event list from server-rendered
 * data; the browser holds no token. Live updates arrive over the
 * cookie-authed SSE route handler. Later re-renders do not re-seed an
 * existing subscription (that would clobber live frames with stale data).
 */
export function useZombieEventStream(
  workspaceId: string,
  zombieId: string,
  initial: EventRow[],
): UseZombieEventStreamResult {
  // Hold the latest `initial` without making it a `subscribe` dependency:
  // a fresh array identity each render must not resubscribe. Updating the
  // ref every render (rather than capturing first-render only) keeps the
  // value current if `zombieId` changes within a live instance — the
  // registry ignores `initial` for an existing entry, so seed-once holds.
  const initialRef = useRef(initial);
  initialRef.current = initial;
  const subscribeFn = useCallback(
    (listener: () => void) =>
      subscribe(workspaceId, zombieId, initialRef.current, listener),
    [workspaceId, zombieId],
  );
  const snapshotFn = useCallback(() => getSnapshot(zombieId), [zombieId]);
  const snapshot = useSyncExternalStore(subscribeFn, snapshotFn, snapshotFn);

  const appendOptimistic = useCallback(
    (text: string, actor: string) =>
      registryAppendOptimistic(zombieId, text, actor),
    [zombieId],
  );
  const reconcileOptimistic = useCallback(
    (tempId: string, realEventId: string) =>
      registryReconcileOptimistic(zombieId, tempId, realEventId),
    [zombieId],
  );
  const markOptimisticFailed = useCallback(
    (tempId: string) => registryMarkOptimisticFailed(zombieId, tempId),
    [zombieId],
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

function convertEvent(event: ZombieEvent): ThreadMessageLike {
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
