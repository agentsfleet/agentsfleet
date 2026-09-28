"use client";

import { useCallback, useMemo, useRef } from "react";
import { MessageNotSentError, type AppendMessage } from "@assistant-ui/react";

import { PENDING_SEND_STATE, type PendingSendOutcome, type PendingSendWriters } from "./useFleetPendingSends";
import type { useFleetEventStream } from "./useFleetEventStream";
import { steerFleetAction } from "@/app/(dashboard)/w/[workspaceId]/fleets/actions";
import { HTTP_STATUS_UNAUTHORIZED, isDefiniteRefusal } from "@/lib/api/errors";
import { STEER_MESSAGE_MAX_BYTES } from "@/lib/api/fleets-types";
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
const UTF8 = new TextEncoder();
// UTF-8 spends one to three bytes per UTF-16 unit (a surrogate pair's four
// bytes are two per unit), so most drafts are settled by their length alone.
const MAX_UTF8_BYTES_PER_UNIT = 3;
const settledEitherWay = (): void => undefined;

// One queue per fleet, in module state, so a composer that remounts mid-send
// queues behind the send still out instead of overtaking it.
const DELIVERY_TAILS = new Map<string, Promise<void>>();

type StreamApi = ReturnType<typeof useFleetEventStream>;
type DeliveryCtx = {
  workspaceId: string;
  fleetId: string;
  appendOptimistic: StreamApi["appendOptimistic"];
  reconcileOptimistic: StreamApi["reconcileOptimistic"];
  discardOptimistic: StreamApi["discardOptimistic"];
  onSubmitted: (tempId: string) => void;
  writers: PendingSendWriters;
};

/** A failed send's text and id, and the ledger it belongs to. */
type Restored = { operationId: string; text: string; ledger: PendingSendWriters };

export type MessageDelivery = {
  /** assistant-ui's `onNew`: a message the composer submitted. */
  onNew: (msg: AppendMessage) => Promise<void>;
  /** A ledger entry sent again under its own operation id. The send reports
   * through the ledger, so a click has nothing to await. */
  resend: (operationId: string) => void;
  /** The composer put this send's text back on mount. */
  noteRestored: (operationId: string, text: string) => void;
  /** The composer's draft changed: whether a failed send's text came back. */
  noteDraft: (text: string) => void;
};

/** Whether `text` is longer than the daemon takes. */
export function exceedsSteerLimit(text: string): boolean {
  if (text.length > STEER_MESSAGE_MAX_BYTES) return true;
  if (text.length * MAX_UTF8_BYTES_PER_UNIT <= STEER_MESSAGE_MAX_BYTES) return false;
  return UTF8.encode(text).length > STEER_MESSAGE_MAX_BYTES;
}

export function useMessageDelivery(ctx: DeliveryCtx): MessageDelivery {
  const deliver = useSerializedDelivery(ctx);
  const { writers } = ctx;
  // The draft a failure put back, and the id it was sent under. Only that
  // draft, sent unchanged, reuses the id: the same words typed later are a new
  // message, and an old id would replay an old admission and run nothing.
  const restored = useRef<Restored | null>(null);
  // Every submit the composer made, counted the way assistant-ui counts them:
  // it returns a failed send's draft only when no newer send started since.
  const sends = useRef(0);

  const onNew = useCallback(
    async (msg: AppendMessage) => {
      const send = ++sends.current;
      const text = extractMessageText(msg);
      if (text.length === 0) return;
      // Refused before it is named or recorded: the daemon would refuse it, and
      // the composer keeps the draft while its hint says why.
      if (exceedsSteerLimit(text)) throw new MessageNotSentError();
      const back = restored.current;
      restored.current = null;
      let operationId: string;
      try {
        operationId = back?.text === text && back.ledger === writers ? back.operationId : mintOperationId();
      } catch {
        throw new MessageNotSentError();
      }
      if (await deliver(operationId, text)) return;
      // A draft assistant-ui did not return is not this send's to reuse: the
      // same words typed later are a new message.
      if (sends.current === send) restored.current = { operationId, text, ledger: writers };
      throw new MessageNotSentError();
    },
    [deliver, writers],
  );

  const resend = useCallback(
    (operationId: string) => {
      const entry = writers.find(operationId);
      if (entry === undefined) return;
      // Resent from the notice: the composer's copy is no longer a recovery.
      if (restored.current?.operationId === operationId) restored.current = null;
      void deliver(entry.operationId, entry.text);
    },
    [deliver, writers],
  );

  const noteRestored = useCallback((operationId: string, text: string) => {
    restored.current = { operationId, text, ledger: writers };
  }, [writers]);

  // An edit ends the recovery: the words the operator types next are theirs,
  // even when they come back round to the same text. An empty composer settles
  // nothing — it is the draft before a restore lands.
  const noteDraft = useCallback((text: string) => {
    if (text.length > 0 && restored.current !== null && restored.current.text !== text) restored.current = null;
  }, []);

  return useMemo(
    () => ({ onNew, resend, noteRestored, noteDraft }),
    [onNew, resend, noteRestored, noteDraft],
  );
}

// One send: recorded, painted, then POSTed behind the fleet's previous send.
// Removing the browser-side queue let two rapid submissions race: their Server
// Action POSTs could reach the server out of submission order, so "stop" could
// be assigned an earlier event id than the "deploy" it was meant to follow.
function useSerializedDelivery({
  workspaceId, fleetId, appendOptimistic, reconcileOptimistic, discardOptimistic, onSubmitted, writers,
}: DeliveryCtx): (operationId: string, text: string) => Promise<boolean> {
  return useCallback(
    (operationId: string, text: string): Promise<boolean> => {
      // The ledger entry is written first: a document that dies between here
      // and the acknowledgement leaves a record the next one can resend.
      writers.begin({ operationId, text, submittedAtMs: Date.now() });
      const tempId = appendOptimistic(text, OPTIMISTIC_ACTOR);
      if (tempId) onSubmitted(tempId);
      const ended = (outcome: PendingSendOutcome): boolean => {
        discardOptimistic(tempId);
        writers.fail(operationId, outcome);
        return false;
      };
      const send = async (): Promise<boolean> => {
        let result: Awaited<ReturnType<typeof steerFleetAction>>;
        try {
          result = await steerFleetAction(workspaceId, fleetId, text, operationId);
        } catch {
          // The Server Action's transport failed: nothing answered, so the
          // daemon may or may not hold the message.
          return ended(PENDING_SEND_STATE.UNKNOWN);
        }
        if (!result.ok) return ended(outcomeOf(result.status));
        // Settled before the cosmetic reconcile: the daemon holds it, whatever
        // the painting does next.
        writers.settle(operationId);
        reconcileOptimistic(tempId, result.data.event_id);
        requestOnboardingRefresh(workspaceId);
        return true;
      };
      const key = `${workspaceId}:${fleetId}`;
      // `send` reports its own failure through the ledger. The tail settles
      // either way, so a throw after the acknowledgement cannot stall every
      // later send on this fleet — the next message always gets its slot.
      const slot = (DELIVERY_TAILS.get(key) ?? Promise.resolve()).then(send);
      const tail = slot.then(settledEitherWay, settledEitherWay);
      DELIVERY_TAILS.set(key, tail);
      void tail.then(() => {
        if (DELIVERY_TAILS.get(key) === tail) DELIVERY_TAILS.delete(key);
      });
      return slot;
    },
    [workspaceId, fleetId, appendOptimistic, reconcileOptimistic, discardOptimistic, onSubmitted, writers],
  );
}

// A 401 asks for a sign-in. A client refusal other than a timeout means the
// server saw the request and said no. Anything else — a timeout, a 5xx after
// the row committed, no status at all — leaves delivery unconfirmed.
function outcomeOf(status: number | undefined): PendingSendOutcome {
  if (status === HTTP_STATUS_UNAUTHORIZED) return PENDING_SEND_STATE.SESSION;
  return isDefiniteRefusal(status) ? PENDING_SEND_STATE.REFUSED : PENDING_SEND_STATE.UNKNOWN;
}

function extractMessageText(msg: AppendMessage): string {
  for (const part of msg.content) {
    if (part.type === "text") return part.text;
  }
  return "";
}
