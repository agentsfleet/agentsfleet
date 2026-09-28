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
//
// Every send ends: one with no answer by `SEND_TIMEOUT_MS` ends unknown and
// frees the fleet's queue. A reused id the daemon refuses as another message's
// ends in `conflict`, and its text goes out again only under a new id.

// Placeholder actor on an optimistic row until the stream's matching
// `EVENT_RECEIVED` lands and reconciliation replaces it with the real
// authenticated principal.
const OPTIMISTIC_ACTOR = "steer:pending";
const UTF8 = new TextEncoder();
// UTF-8 spends one to three bytes per UTF-16 unit (a surrogate pair's four
// bytes are two per unit), so most drafts are settled by their length alone.
const MAX_UTF8_BYTES_PER_UNIT = 3;
const settledEitherWay = (): void => undefined;
/** How long a send waits for its Server Action: above the server's own 20 s
 * retry deadline (`lib/api/retry-config.ts`), so a slow send that is still
 * alive is never abandoned. */
export const SEND_TIMEOUT_MS = 30_000;
/** The daemon's refusal of an operation id that already names another message. */
const OPERATION_CONFLICT_CODE = "UZ-AGT-016";

// One queue per fleet, in module state, so a composer that remounts mid-send
// queues behind the send still out instead of overtaking it.
const DELIVERY_TAILS = new Map<string, Promise<void>>();

type SteerResult = Awaited<ReturnType<typeof steerFleetAction>>;
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

/** A failed send's text and id, the ledger it belongs to, and what the
 * composer has done with it since: shown it, and emptied it after. */
type Restored = { operationId: string; text: string; ledger: PendingSendWriters; shown: boolean; cleared: boolean };

export type MessageDelivery = {
  /** assistant-ui's `onNew`: a message the composer submitted. */
  onNew: (msg: AppendMessage) => Promise<void>;
  /** A ledger entry sent again: under its own operation id, or under a new
   * one when the daemon refused that id as another message's. The send
   * reports through the ledger, so a click has nothing to await. */
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
  const { take, forget, remember, noteDraft } = useRestoredDraft(writers);
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
      const back = take();
      const operationId = back?.text === text && back.ledger === writers ? back.operationId : mintOrNull();
      if (operationId === null) throw new MessageNotSentError();
      if (await deliver(operationId, text)) return;
      // A draft assistant-ui did not return is not this send's to reuse: the
      // same words typed later are a new message.
      if (sends.current === send) remember(operationId, text);
      throw new MessageNotSentError();
    },
    [deliver, writers, take, remember],
  );

  const resend = useCallback(
    (operationId: string) => {
      const entry = writers.find(operationId);
      if (entry === undefined) return;
      // Resent from the notice: the composer's copy is no longer a recovery.
      forget(operationId);
      // A conflict's id is spent: it is dismissed, and the text goes out as new.
      const spent = entry.state === PENDING_SEND_STATE.CONFLICT;
      const sendAs = spent ? mintOrNull() : operationId;
      if (sendAs === null) return;
      if (spent) writers.dismiss(operationId);
      void deliver(sendAs, entry.text);
    },
    [deliver, writers, forget],
  );

  return useMemo(
    () => ({ onNew, resend, noteRestored: remember, noteDraft }),
    [onNew, resend, remember, noteDraft],
  );
}

// The draft a failure put back, and the id it was sent under. Only that draft,
// sent unchanged, reuses the id: the same words typed later are a new message,
// and an old id would replay an old admission and run nothing.
function useRestoredDraft(writers: PendingSendWriters) {
  const restored = useRef<Restored | null>(null);

  /** The recovery, if any, handed to the send that reads it — and ended. */
  const take = useCallback((): Restored | null => {
    const back = restored.current;
    restored.current = null;
    return back;
  }, []);

  const forget = useCallback((operationId: string) => {
    if (restored.current?.operationId === operationId) restored.current = null;
  }, []);

  // A conflict's id already names another message, so its draft is never
  // remembered under it.
  const remember = useCallback((operationId: string, text: string) => {
    const spent = writers.find(operationId)?.state === PENDING_SEND_STATE.CONFLICT;
    restored.current = spent ? null : { operationId, text, ledger: writers, shown: false, cleared: false };
  }, [writers]);

  // An edit ends the recovery: the words the operator puts in the composer
  // are theirs, even the same words put back after a clear. An empty composer
  // ends nothing by itself — a Send empties it before `onNew` reads this, and
  // before the restore lands it is simply empty.
  const noteDraft = useCallback((text: string) => {
    const back = restored.current;
    if (back === null) return;
    if (text.length === 0) back.cleared = back.shown;
    else if (text === back.text && !back.cleared) back.shown = true;
    else restored.current = null;
  }, []);

  return { take, forget, remember, noteDraft };
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
        const result = await steerWithin(workspaceId, fleetId, text, operationId, writers);
        if (result === null) return ended(PENDING_SEND_STATE.UNKNOWN);
        if (!result.ok) return ended(outcomeOf(result));
        // Settled before the cosmetic reconcile: the daemon holds it, whatever
        // the painting does next.
        writers.settle(operationId);
        reconcileOptimistic(tempId, result.data.event_id, result.data.replayed);
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

// The Server Action's answer, or null when nothing answered: its transport
// failed, or `SEND_TIMEOUT_MS` passed first. Either way the daemon may or may
// not hold the message. An answer after the timeout only settles the entry —
// its row is gone, and the stream shows the message if it landed.
async function steerWithin(
  workspaceId: string, fleetId: string, text: string, operationId: string, writers: PendingSendWriters,
): Promise<SteerResult | null> {
  const action = steerFleetAction(workspaceId, fleetId, text, operationId);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<null>((resolve) => {
    timer = setTimeout(() => resolve(null), SEND_TIMEOUT_MS);
  });
  const result = await Promise.race([action, timeout]).catch(() => null).finally(() => clearTimeout(timer));
  if (result === null) action.then((late) => (late.ok ? writers.settle(operationId) : undefined), settledEitherWay);
  return result;
}

// A 401 asks for a sign-in. A reused id is refused for good. Any other client
// refusal but a timeout means the server saw the request and said no. Anything
// else — a timeout, a 5xx after the row committed, no status at all — leaves
// delivery unconfirmed.
function outcomeOf({ status, errorCode }: { status?: number; errorCode?: string }): PendingSendOutcome {
  if (status === HTTP_STATUS_UNAUTHORIZED) return PENDING_SEND_STATE.SESSION;
  if (errorCode === OPERATION_CONFLICT_CODE) return PENDING_SEND_STATE.CONFLICT;
  return isDefiniteRefusal(status) ? PENDING_SEND_STATE.REFUSED : PENDING_SEND_STATE.UNKNOWN;
}

// A fresh operation id, or null on a platform with no generator: the send is
// then not made.
function mintOrNull(): string | null {
  try {
    return mintOperationId();
  } catch {
    return null;
  }
}

function extractMessageText(msg: AppendMessage): string {
  for (const part of msg.content) {
    if (part.type === "text") return part.text;
  }
  return "";
}
