"use client";

import { useCallback, useRef } from "react";
import type { AppendMessage } from "@assistant-ui/react";

import type { FailedDelivery } from "./useFleetDeliveryFailure";
import type { useFleetEventStream } from "./useFleetEventStream";
import { steerFleetAction } from "@/app/(dashboard)/w/[workspaceId]/fleets/actions";
import { requestOnboardingRefresh } from "@/lib/onboarding-refresh";

// The tail of the steer delivery chain, lifted out of `FleetThread` at its
// length cap. Self-contained: optimistic append, the serialized POST, and the
// two failure paths that put a `failed` row back on screen.

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
  markOptimisticFailed: StreamApi["markOptimisticFailed"];
  onFailure: (failure: FailedDelivery) => void;
};

export function useNewMessageHandler({
  workspaceId,
  fleetId,
  appendOptimistic,
  reconcileOptimistic,
  markOptimisticFailed,
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
      const send = async () => {
        try {
          const result = await steerFleetAction(workspaceId, fleetId, text);
          if (result.ok) {
            reconcileOptimistic(tempId, result.data.event_id);
            requestOnboardingRefresh(workspaceId);
            return;
          }
          markOptimisticFailed(tempId);
          onFailure({ message: msg, tempId, kind: result.status === 401 ? "session" : "send" });
        } catch {
          // The Server Action's Remote Procedure Call (RPC) transport failed (offline, or the
          // action invocation errored) — surface the same `failed` row the
          // ok:false path produces so the user knows the steer didn't land.
          markOptimisticFailed(tempId);
          onFailure({ message: msg, tempId, kind: "send" });
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
      markOptimisticFailed,
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
