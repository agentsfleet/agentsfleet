"use client";

import { useCallback, useRef } from "react";
import type { AppendMessage } from "@assistant-ui/react";

import { DELIVERY_FAILURE, type DeliveryFailureKind, type FailedDelivery } from "./useFleetDeliveryFailure";
import type { useFleetEventStream } from "./useFleetEventStream";
import { steerFleetAction } from "@/app/(dashboard)/w/[workspaceId]/fleets/actions";
import { HTTP_STATUS_UNAUTHORIZED } from "@/lib/api/errors";
import { requestOnboardingRefresh } from "@/lib/onboarding-refresh";

// The tail of the steer delivery chain, lifted out of `FleetThread` at its
// length cap. Self-contained: optimistic append, the serialized POST, and the
// refusal path. A refused send leaves the thread and its text goes back to the
// composer, the way Claude.ai does it; nothing re-posts it without a click,
// because the POST carries no idempotency key.

// Placeholder actor on an optimistic row until the stream's matching
// `EVENT_RECEIVED` lands and reconciliation replaces it with the real
// authenticated principal.
const OPTIMISTIC_ACTOR = "steer:pending";

type StreamApi = ReturnType<typeof useFleetEventStream>;
type NewHandlerCtx = {
  workspaceId: string;
  fleetId: string;
  appendOptimistic: StreamApi["appendOptimistic"];
  reconcileOptimistic: StreamApi["reconcileOptimistic"];
  discardOptimistic: StreamApi["discardOptimistic"];
  onSubmitted: (tempId: string) => void;
  onFailure: (failure: FailedDelivery) => void;
};

export function useNewMessageHandler({
  workspaceId,
  fleetId,
  appendOptimistic,
  reconcileOptimistic,
  discardOptimistic,
  onSubmitted,
  onFailure,
}: NewHandlerCtx): (msg: AppendMessage) => Promise<void> {
  // The tail of the delivery chain. Removing the browser-side queue let two
  // rapid submissions race: their Server Action POSTs could reach the server
  // out of submission order, so "stop" could be assigned an earlier event id
  // than the "deploy" it was meant to follow. Optimistic rows still appear the
  // instant they are typed; only the POSTs are serialised, so the server
  // assigns event ids in the order the operator sent them.
  const deliveryTail = useRef<Promise<void>>(Promise.resolve());
  return useCallback(
    async (msg: AppendMessage) => {
      const text = extractMessageText(msg);
      if (text.length === 0) return;
      // Optimistic append is synchronous and in call order — the operator sees
      // both messages immediately, before any POST resolves.
      const tempId = appendOptimistic(text, OPTIMISTIC_ACTOR);
      if (tempId) onSubmitted(tempId);
      const refuse = (kind: DeliveryFailureKind) => {
        discardOptimistic(tempId);
        onFailure({ text, kind });
      };
      const send = async () => {
        try {
          const result = await steerFleetAction(workspaceId, fleetId, text);
          if (result.ok) {
            reconcileOptimistic(tempId, result.data.event_id);
            requestOnboardingRefresh(workspaceId);
            return;
          }
          refuse(result.status === HTTP_STATUS_UNAUTHORIZED ? DELIVERY_FAILURE.SESSION : DELIVERY_FAILURE.SEND);
        } catch {
          // The Server Action's Remote Procedure Call (RPC) transport failed
          // (offline, or the action invocation errored): the same refusal.
          refuse(DELIVERY_FAILURE.SEND);
        }
      };
      // Chain this POST after the previous one. `send` reports its own failure
      // and never rejects, so the tail never rejects — the next message always
      // gets its slot whether this one succeeded or failed.
      const slot = deliveryTail.current.then(send);
      deliveryTail.current = slot;
      await slot;
    },
    [
      workspaceId,
      fleetId,
      appendOptimistic,
      reconcileOptimistic,
      discardOptimistic,
      onSubmitted,
      onFailure,
    ],
  );
}

function extractMessageText(msg: AppendMessage): string {
  for (const part of msg.content) {
    if (part.type === "text") return part.text;
  }
  return "";
}
