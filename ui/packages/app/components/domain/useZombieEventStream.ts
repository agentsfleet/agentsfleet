"use client";

import { useCallback, useSyncExternalStore } from "react";
import type { ThreadMessageLike } from "@assistant-ui/react";
import {
  appendOptimistic as registryAppendOptimistic,
  CONNECTION_STATUS,
  getSnapshot,
  reconcileOptimistic as registryReconcileOptimistic,
  subscribe,
  type ConnectionStatus,
  type RetryState,
  type ZombieEvent,
  type ZombieEventStatus,
} from "@/lib/streaming/zombie-stream-registry";

// Public re-exports so existing consumers keep their import surface.
export {
  CONNECTION_STATUS,
  type ConnectionStatus,
  type RetryState,
  type ZombieEvent,
  type ZombieEventStatus,
};

export type UseZombieEventStreamResult = {
  events: ZombieEvent[];
  connectionStatus: ConnectionStatus;
  isRunning: boolean;
  retryState: RetryState;
  appendOptimistic: (text: string, actor: string) => string;
  reconcileOptimistic: (tempId: string, realEventId: string) => void;
  convertEvent: (event: ZombieEvent) => ThreadMessageLike;
};

/**
 * React boundary over the module-level zombie-stream registry. Multiple
 * mounts of this hook for the same `zombieId` share one EventSource +
 * backfill — and the connection survives a /dashboard ↔ /zombies/[id]
 * round-trip up to the registry's idle release window.
 *
 * `token === null` returns an inert result (CONNECTING, no events); the
 * registry never opens a connection for an unauthenticated mount.
 */
export function useZombieEventStream(
  workspaceId: string,
  zombieId: string,
  token: string | null,
): UseZombieEventStreamResult {
  const subscribeFn = useCallback(
    (listener: () => void) => subscribe(workspaceId, zombieId, token, listener),
    [workspaceId, zombieId, token],
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

  return {
    events: snapshot.events,
    connectionStatus: snapshot.connectionStatus,
    isRunning: snapshot.events.some((ev) => ev.status === "received"),
    retryState: snapshot.retryState,
    appendOptimistic,
    reconcileOptimistic,
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
