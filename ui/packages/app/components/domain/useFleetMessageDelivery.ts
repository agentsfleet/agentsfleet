"use client";

import { useCallback, useMemo, useRef } from "react";
import { MessageNotSentError, type AppendMessage } from "@assistant-ui/react";

import { PENDING_SEND_STATE, type FleetPendingSends } from "./useFleetPendingSends";
import type { useFleetEventStream } from "./useFleetEventStream";
import { steerFleetAction } from "@/app/(dashboard)/w/[workspaceId]/fleets/actions";
import { HTTP_STATUS_UNAUTHORIZED } from "@/lib/api/errors";
import { requestOnboardingRefresh } from "@/lib/onboarding-refresh";
import { mintOperationId } from "@/lib/streaming/operation-id";

// The tail of the steer delivery chain, lifted out of `FleetThread` at its
// length cap. Every send is one operation: named before the optimistic append,
// held in the fleet's ledger until the daemon's 202 is reconciled, and sent
// again under the same name by Resend, so the daemon answers the first
// admission instead of running a second. A send that fails leaves the thread;
// its text goes back to the composer the way Claude.ai does it — the handler
// rejects with assistant-ui's `MessageNotSentError` and the composer restores
// the draft it cleared — and its ledger entry keeps the recovery.

// Placeholder actor on an optimistic row until the stream's matching
// `EVENT_RECEIVED` lands and reconciliation replaces it with the real
// authenticated principal.
const OPTIMISTIC_ACTOR = "steer:pending";

type StreamApi = ReturnType<typeof useFleetEventStream>;
type DeliveryCtx = {
  workspaceId: string;
  fleetId: string;
  appendOptimistic: StreamApi["appendOptimistic"];
  reconcileOptimistic: StreamApi["reconcileOptimistic"];
  discardOptimistic: StreamApi["discardOptimistic"];
  onSubmitted: (tempId: string) => void;
  ledger: FleetPendingSends;
};

export type MessageDelivery = {
  /** assistant-ui's `onNew`: a message the composer submitted. */
  onNew: (msg: AppendMessage) => Promise<void>;
  /** A ledger entry sent again under its own operation id. The send reports
   * through the ledger, so a click has nothing to await. */
  resend: (operationId: string) => void;
};

export function useMessageDelivery({
  workspaceId,
  fleetId,
  appendOptimistic,
  reconcileOptimistic,
  discardOptimistic,
  onSubmitted,
  ledger,
}: DeliveryCtx): MessageDelivery {
  // Removing the browser-side queue let two rapid submissions race: their
  // Server Action POSTs could reach the server out of submission order, so
  // "stop" could be assigned an earlier event id than the "deploy" it was
  // meant to follow. Optimistic rows still appear the instant they are typed;
  // only the POSTs are serialised, so the server assigns event ids in the
  // order the operator sent them.
  const deliveryTail = useRef<Promise<void>>(Promise.resolve());
  const deliver = useCallback(
    (operationId: string, text: string): Promise<boolean> => {
      // The ledger entry is written first: a document that dies between here
      // and the acknowledgement leaves a record the next one can resend.
      ledger.begin({ operationId, text, submittedAtMs: Date.now() });
      const tempId = appendOptimistic(text, OPTIMISTIC_ACTOR);
      if (tempId) onSubmitted(tempId);
      const send = async (): Promise<boolean> => {
        try {
          const result = await steerFleetAction(workspaceId, fleetId, text, operationId);
          if (result.ok) {
            reconcileOptimistic(tempId, result.data.event_id);
            ledger.settle(operationId);
            requestOnboardingRefresh(workspaceId);
            return true;
          }
          discardOptimistic(tempId);
          ledger.fail(
            operationId,
            result.status === HTTP_STATUS_UNAUTHORIZED ? PENDING_SEND_STATE.SESSION : PENDING_SEND_STATE.REFUSED,
          );
        } catch {
          // The Server Action's Remote Procedure Call (RPC) transport failed:
          // nothing answered, so the daemon may or may not hold the message.
          // Unknown, never refused — the notice says so, and Resend is safe
          // because it carries the same operation id.
          discardOptimistic(tempId);
          ledger.fail(operationId, PENDING_SEND_STATE.UNKNOWN);
        }
        return false;
      };
      // Chain this POST after the previous one. `send` reports its own failure
      // and never rejects, so the tail never rejects — the next message always
      // gets its slot whether this one succeeded or failed.
      const slot = deliveryTail.current.then(send);
      deliveryTail.current = slot.then(() => undefined);
      return slot;
    },
    [workspaceId, fleetId, appendOptimistic, reconcileOptimistic, discardOptimistic, onSubmitted, ledger],
  );

  const onNew = useCallback(
    async (msg: AppendMessage) => {
      const text = extractMessageText(msg);
      if (text.length === 0) return;
      // Named before anything else happens, and synchronously: a mint that
      // fails rejects before the optimistic row exists. A draft equal to an
      // unresolved send IS that send — the restored text pressed Send — so it
      // keeps the id the daemon may already hold instead of minting a second.
      let operationId: string;
      try {
        operationId = ledger.byText(text)?.operationId ?? mintOperationId();
      } catch {
        throw new MessageNotSentError();
      }
      if (!(await deliver(operationId, text))) throw new MessageNotSentError();
    },
    [deliver, ledger],
  );

  const resend = useCallback(
    (operationId: string) => {
      const entry = ledger.pending.find((held) => held.operationId === operationId);
      if (entry === undefined) return;
      void deliver(entry.operationId, entry.text);
    },
    [deliver, ledger],
  );

  return useMemo(() => ({ onNew, resend }), [onNew, resend]);
}

function extractMessageText(msg: AppendMessage): string {
  for (const part of msg.content) {
    if (part.type === "text") return part.text;
  }
  return "";
}
