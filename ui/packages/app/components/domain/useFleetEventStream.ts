"use client";

import { useCallback, useEffect, useRef, useSyncExternalStore } from "react";
import type { ThreadMessageLike } from "@assistant-ui/react";
import type { EventRow } from "@/lib/api/events";
import {
  appendOptimistic as registryAppendOptimistic,
  CONNECTION_STATUS,
  discardOptimistic as registryDiscardOptimistic,
  getSnapshot,
  markOptimisticFailed as registryMarkOptimisticFailed,
  reconcileOptimistic as registryReconcileOptimistic,
  reconcileServerRows,
  retryConnection as registryRetryConnection,
  subscribe,
  type ConnectionStatus,
  type FleetEvent,
  type FleetEventStatus,
} from "@/lib/streaming/fleet-stream-registry";
import type { InstallStepId } from "@/lib/streaming/install-steps";

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
  appendOptimistic: (text: string, actor: string) => string;
  reconcileOptimistic: (tempId: string, realEventId: string) => boolean;
  markOptimisticFailed: (tempId: string) => void;
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
    markOptimisticFailed,
    discardOptimistic,
    retryConnection,
    convertEvent,
  };
}

function convertEvent(event: FleetEvent): ThreadMessageLike {
  return {
    role: event.role,
    id: event.id,
    createdAt: event.createdAt,
    // Content carries the TRIGGER — the operator's message or the integration
    // headline. The fleet's reply rides the custom bag; the renderer paints it
    // as its own bubble so a reply never appears as operator speech.
    content: [{ type: "text", text: event.text }],
    metadata: {
      custom: {
        actor: event.actor,
        requestJson: event.custom?.requestJson,
        status: event.status,
        // The fleet's reply on this same durable row, and the sentence to show
        // in its place when the reply is empty (still working, blocked, failed).
        reply: event.reply,
        outcome: event.outcome,
        // The failure CLASS, not the sentence — the renderer picks remediation
        // guidance off it (a sentence cannot be matched against reliably).
        failureLabel: event.failureLabel,
        failureDetail: event.failureDetail,
        // The tool calls the fleet made while working this event. They ride the
        // custom bag rather than assistant-ui's tool-call content parts: the
        // backend publishes them as sibling frames keyed by event_id, not as
        // structured parts of the assistant message, and reshaping them into
        // parts would invent a message boundary the wire does not have.
        tools: event.tools,
      },
    },
  };
}
