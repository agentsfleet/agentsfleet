"use client";

import { useCallback, useEffect, useRef, useSyncExternalStore } from "react";
import type { ThreadMessageLike } from "@assistant-ui/react";
import type { EventRow } from "@/lib/api/events";
import {
  appendOptimistic as registryAppendOptimistic,
  CONNECTION_STATUS,
  discardOptimistic as registryDiscardOptimistic,
  getSnapshot,
  reconcileOptimistic as registryReconcileOptimistic,
  reconcileServerRows,
  retryConnection as registryRetryConnection,
  subscribe,
  type ConnectionStatus,
} from "@/lib/streaming/fleet-stream-registry";
import {
  AGENTSFLEET_EVENT_STATUS,
  type FleetEvent,
  type FleetEventStatus,
} from "@/lib/streaming/fleet-stream-row";
import type { InstallStepId } from "@/lib/streaming/install-steps";
import { setEventDetailReader } from "@/lib/streaming/fleet-stream-reply-registry";
import { readEventDetailRoute } from "@/lib/streaming/fleet-stream-detail-reader";

// The chat is the only surface that shows reply text, so it installs the read
// a reply uses when its stream lost the final words: the same-origin route, so
// a send in flight never holds the read up, nor the read a send.
setEventDetailReader(readEventDetailRoute);

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
  // The latest install step advanced by an `install:*` frame on the shared
  // stream, or null for a non-installing fleet. The InstallStates surface reads
  // this to advance its rendered step with no polling and to detect the
  // installing→active flip.
  installStep: InstallStepId | null;
  appendOptimistic: (text: string, actor: string, sentAs?: string) => string;
  reconcileOptimistic: (tempId: string, realEventId: string, replayed: boolean) => boolean;
  discardOptimistic: (tempId: string) => void;
  retryConnection: () => void;
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
 * cookie-authenticated Server-Sent Events route handler. Later snapshots
 * merge authoritative terminal rows without replacing newer live frames.
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
  // registry ignores `initial` while subscribing to an existing entry; the
  // reconciliation effect below handles later authoritative snapshots.
  const initialRef = useRef(initial);
  initialRef.current = initial;
  const subscribeFn = useCallback(
    (listener: () => void) =>
      subscribe(workspaceId, fleetId, initialRef.current, listener),
    [workspaceId, fleetId],
  );
  const snapshotFn = useCallback(() => getSnapshot(fleetId), [fleetId]);
  const snapshot = useSyncExternalStore(subscribeFn, snapshotFn, snapshotFn);

  useEffect(() => {
    reconcileServerRows(fleetId, initial);
  }, [fleetId, initial]);

  const appendOptimistic = useCallback(
    (text: string, actor: string, sentAs?: string) =>
      registryAppendOptimistic(fleetId, text, actor, sentAs),
    [fleetId],
  );
  const reconcileOptimistic = useCallback(
    (tempId: string, realEventId: string, replayed: boolean) =>
      registryReconcileOptimistic(fleetId, tempId, realEventId, replayed),
    [fleetId],
  );
  const discardOptimistic = useCallback(
    (tempId: string) => registryDiscardOptimistic(fleetId, tempId),
    [fleetId],
  );
  const retryConnection = useCallback(
    () => registryRetryConnection(fleetId),
    [fleetId],
  );

  // There is deliberately no aggregate "the fleet is working" flag. The one
  // that existed was true whenever ANY event sat unfinished, so a single
  // stranded run marked the fleet busy forever — and the composer, which read
  // it, held every message from then on. Work is now reported per event, on
  // the row it belongs to, where a strand can only misreport itself.
  return {
    events: snapshot.events,
    connectionStatus: snapshot.connectionStatus,
    installStep: snapshot.installStep,
    appendOptimistic,
    reconcileOptimistic,
    discardOptimistic,
    retryConnection,
    convertEvent,
  };
}

/** The trigger message a row renders as. The thread compares its output to
 * decide whether a trigger changed, so a field read here is compared too. */
export function convertEvent(event: FleetEvent): ThreadMessageLike {
  return {
    role: event.role,
    id: event.id,
    createdAt: event.createdAt,
    // Content carries the TRIGGER — the operator's message or the integration
    // headline. The fleet's answer, reasoning and tool calls become parts of
    // the reply message (`toReplyMessage`), so a reply never appears as
    // operator speech.
    content: [{ type: "text", text: event.text }],
    metadata: {
      custom: {
        actor: event.actor,
        requestJson: event.custom?.requestJson,
        status: event.status,
        // Waiting for a runner: this tab's own send before the daemon opened
        // it, or any sender's message the thread read or admitted frame says
        // is still on the queue.
        queued: event.clientTimestamp === true || event.status === AGENTSFLEET_EVENT_STATUS.QUEUED,
        submittedAtMs: event.submittedAtMs,
        // The reply itself is not here: it is the reply message's content
        // (`toReplyMessage`), and carrying it would change this message on
        // every streamed word. What is here is the sentence to show when there
        // is no reply (still working, blocked, failed).
        replyRecovering: event.replyRecovering,
        outcome: event.outcome,
        // The failure CLASS, not the sentence — the renderer picks remediation
        // guidance off it (a sentence cannot be matched against reliably).
        failureLabel: event.failureLabel,
        failureDetail: event.failureDetail,
      },
    },
  };
}
