import type { LiveFrame } from "@/lib/api/events";
import type { FRAME_KIND } from "@/lib/api/events-types";
import { outcomeForStatus, roleFor } from "@/lib/events/event-summary";
import { AGENTSFLEET_EVENT_STATUS, figure, text, type FleetEvent } from "./fleet-stream-row";

// How a message waiting for a runner reaches the timeline, beside
// `fleet-stream-frames.ts`: `event_admitted` opens a waiting row the moment the
// daemon accepts a message, and `event_received` for the same event starts it.
// The two can arrive in either order — a runner can lease a send and announce
// it before the route publishes — so an admitted frame for a row already held
// fills only an empty text and never moves the row back to waiting.

type AdmittedFrame = Extract<LiveFrame, { kind: typeof FRAME_KIND.EVENT_ADMITTED }>;
type ReceivedFrame = Extract<LiveFrame, { kind: typeof FRAME_KIND.EVENT_RECEIVED }>;

export function applyEventAdmitted(prev: FleetEvent[], frame: AdmittedFrame): FleetEvent[] {
  const message = text(frame.message);
  const index = prev.findIndex((event) => event.id === frame.event_id);
  const existing = prev[index];
  if (existing === undefined) return [...prev, waitingRow(frame, message)];
  if (existing.text.length > 0 || message.length === 0) return prev;
  const updated = [...prev];
  updated[index] = { ...existing, text: message };
  return updated;
}

/**
 * The row a received frame leaves behind for one the thread already holds: a
 * waiting row starts, and an empty text takes the frame's message. `null` when
 * the frame changes neither.
 */
export function receivedUpdate(existing: FleetEvent, frame: ReceivedFrame): FleetEvent | null {
  const message = text(frame.message);
  const fillsText = existing.text.length === 0 && message.length > 0;
  const starts = existing.status === AGENTSFLEET_EVENT_STATUS.QUEUED;
  if (!fillsText && !starts) return null;
  return {
    ...existing,
    ...(fillsText ? { text: message } : {}),
    ...(starts
      ? {
          status: AGENTSFLEET_EVENT_STATUS.RECEIVED,
          outcome: outcomeForStatus(AGENTSFLEET_EVENT_STATUS.RECEIVED),
        }
      : {}),
  };
}

/** A waiting row a recovered "received" history row starts: the received frame
 * fell in a gap. Any other row is returned as it is. */
export function startWaiting(event: FleetEvent): FleetEvent {
  if (event.status !== AGENTSFLEET_EVENT_STATUS.QUEUED) return event;
  return {
    ...event,
    status: AGENTSFLEET_EVENT_STATUS.RECEIVED,
    outcome: outcomeForStatus(AGENTSFLEET_EVENT_STATUS.RECEIVED),
  };
}

// ── internals ────────────────────────────────────────────────────────────

function waitingRow(frame: AdmittedFrame, message: string): FleetEvent {
  const createdAt = figure(frame.created_at);
  return {
    id: frame.event_id,
    role: roleFor(frame.actor),
    actor: frame.actor,
    text: message,
    reply: "",
    outcome: outcomeForStatus(AGENTSFLEET_EVENT_STATUS.QUEUED),
    failureLabel: null,
    failureDetail: null,
    createdAt: createdAt === null ? new Date() : new Date(createdAt),
    status: AGENTSFLEET_EVENT_STATUS.QUEUED,
  };
}
