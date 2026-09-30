"use client";

import { useCallback, useMemo, useRef } from "react";
import { MessageNotSentError, type AppendMessage } from "@assistant-ui/react";

import { PENDING_SEND_STATE, type PendingSendOutcome, type PendingSendWriters } from "./useFleetPendingSends";
import type { useFleetEventStream } from "./useFleetEventStream";
import { HTTP_STATUS_UNAUTHORIZED, isDefiniteRefusal } from "@/lib/api/errors";
import { postSteer, type SteerResult } from "@/lib/api/fleet-steer";
import { overSteerLimit, steerBytesNearLimit, type SteerAccepted } from "@/lib/api/fleets-types";
import { ERROR_CODE } from "@/lib/errors";
import { ACTOR } from "@/lib/events/event-summary";
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
// Every send ends within `SEND_TIMEOUT_MS` of Send. One still queued behind an
// earlier send by then never left the tab: it ends not sent, and its text comes
// back. One on the wire is aborted and ends unknown, with Resend under the same
// id. A reused id the daemon refuses as another message's ends in `conflict`,
// and its text goes out again only under a new id.

// Placeholder actor on an optimistic row until the stream's matching
// `EVENT_RECEIVED` lands and reconciliation replaces it with the real
// authenticated principal.
const OPTIMISTIC_ACTOR = ACTOR.PENDING_STEER;
const settledEitherWay = (): void => undefined;
/** How long a send has, from Send to its end. Longer than the steer route's
 * own worst case — its retry deadline plus one attempt's timeout, the ordering
 * a test pins — so a slow send that is still alive is not abandoned. */
export const SEND_TIMEOUT_MS = 30_000;

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
      if (overSteerLimit(steerBytesNearLimit(text))) throw new MessageNotSentError();
      const back = take();
      const operationId = back?.text === text && back.ledger === writers ? back.operationId : mintOrNull();
      if (operationId === null) throw new MessageNotSentError();
      dismissConflictsOf(writers, text);
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
// Removing the browser-side queue let two rapid submissions race: their POSTs
// could reach the server out of submission order, so "stop" could be assigned
// an earlier event id than the "deploy" it was meant to follow.
function useSerializedDelivery(ctx: DeliveryCtx): (operationId: string, text: string) => Promise<boolean> {
  const { workspaceId, fleetId, appendOptimistic, discardOptimistic, writers } = ctx;
  const acknowledged = useAcknowledgement(ctx);
  return useCallback(
    (operationId: string, text: string): Promise<boolean> => {
      // The ledger entry is written first: a document that dies between here
      // and the acknowledgement leaves a record the next one can resend.
      writers.begin({ operationId, text, submittedAtMs: Date.now() });
      const tempId = appendOptimistic(text, OPTIMISTIC_ACTOR);
      // The clock starts at Send, not at the send's turn in the queue: a send
      // behind a hung one ends when its own time is up, not a full clock later.
      const deadline = startDeadline();
      let dispatched = false;
      let over = false;
      const ended = (outcome: PendingSendOutcome): boolean => {
        over = true;
        discardOptimistic(tempId);
        writers.fail(operationId, outcome);
        return false;
      };
      // Out of time before its turn, it never left this tab and the daemon
      // cannot hold it: it ends not sent, once, and its text comes back, as a
      // refusal's does.
      const unsent = (): boolean => (over ? false : ended(PENDING_SEND_STATE.REFUSED));
      const send = async (): Promise<boolean> => {
        if (deadline.passed()) return unsent();
        dispatched = true;
        const result = await answerWithin(deadline, () => postSteer(workspaceId, fleetId, text, operationId, deadline.signal));
        if (result === null) return ended(PENDING_SEND_STATE.UNKNOWN);
        return result.ok ? acknowledged(operationId, tempId, result.data) : ended(outcomeOf(result));
      };
      const slot = enqueue(`${workspaceId}:${fleetId}`, send);
      return Promise.race([slot, deadline.expired.then(() => (dispatched ? slot : unsent()))]);
    },
    [workspaceId, fleetId, appendOptimistic, discardOptimistic, writers, acknowledged],
  );
}

// The daemon's 202. Settled before the cosmetic reconcile: the daemon holds
// it, whatever the painting does next.
function useAcknowledgement({ workspaceId, reconcileOptimistic, writers }: DeliveryCtx) {
  return useCallback(
    (operationId: string, tempId: string, accepted: SteerAccepted): boolean => {
      writers.settle(operationId);
      reconcileOptimistic(tempId, accepted.event_id, accepted.replayed);
      requestOnboardingRefresh(workspaceId);
      return true;
    },
    [workspaceId, reconcileOptimistic, writers],
  );
}

// `send` reports its own failure through the ledger. The tail settles either
// way, so a throw after the acknowledgement cannot stall every later send on
// this fleet — the next message always gets its slot.
function enqueue(key: string, send: () => Promise<boolean>): Promise<boolean> {
  const slot = (DELIVERY_TAILS.get(key) ?? Promise.resolve()).then(send);
  const tail = slot.then(settledEitherWay, settledEitherWay);
  DELIVERY_TAILS.set(key, tail);
  void tail.then(() => {
    if (DELIVERY_TAILS.get(key) === tail) DELIVERY_TAILS.delete(key);
  });
  return slot;
}

type Deadline = { signal: AbortSignal; expired: Promise<null>; passed: () => boolean; clear: () => void };

// A send's clock: when it runs out it aborts the request on the wire, and
// `expired` ends a send that is still waiting for its turn. `passed` reads the
// time as well as the abort: of two timers due in the same instant the second
// fires after the first one's work, which would otherwise dispatch a send
// whose time is already up.
function startDeadline(): Deadline {
  const controller = new AbortController();
  const endsAtMs = Date.now() + SEND_TIMEOUT_MS;
  const timer = setTimeout(() => controller.abort(), SEND_TIMEOUT_MS);
  const expired = new Promise<null>((resolve) => {
    controller.signal.addEventListener("abort", () => resolve(null), { once: true });
  });
  const passed = () => controller.signal.aborted || Date.now() >= endsAtMs;
  return { signal: controller.signal, expired, passed, clear: () => clearTimeout(timer) };
}

// The steer's answer, or null when none came before the clock ran out.
// Whatever the transport does with the abort, the send ends with it.
function answerWithin(deadline: Deadline, steer: () => Promise<SteerResult>): Promise<SteerResult | null> {
  return Promise.race([steer().catch(() => null), deadline.expired]).finally(deadline.clear);
}

// A conflict's text sent from the composer is that conflict's "Send as new":
// its entry goes, as the notice's own button dismisses it, so the button
// cannot send the same words a second time.
function dismissConflictsOf(writers: PendingSendWriters, text: string): void {
  for (const entry of writers.list()) {
    if (entry.state === PENDING_SEND_STATE.CONFLICT && entry.text === text) writers.dismiss(entry.operationId);
  }
}

// A 401 asks for a sign-in. A reused id is refused for good. Any other client
// refusal but a timeout means the server saw the request and said no. Anything
// else — a timeout, a 5xx after the row committed, no status at all — leaves
// delivery unconfirmed.
function outcomeOf({ status, errorCode }: Extract<SteerResult, { ok: false }>): PendingSendOutcome {
  if (status === HTTP_STATUS_UNAUTHORIZED) return PENDING_SEND_STATE.SESSION;
  if (errorCode === ERROR_CODE.AGENTSFLEET_OPERATION_CONFLICT) return PENDING_SEND_STATE.CONFLICT;
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
