import { instantMs } from "@/lib/api/decode";
import type { LiveFrame } from "@/lib/api/turns";
import type { FRAME_KIND } from "@/lib/api/turns-types";
import { outcomeForStatus, roleFor } from "@/lib/turns/turn-summary";
import { AGENTSFLEET_TURN_STATUS, text, type FleetTurn } from "./fleet-stream-row";

// How a message waiting for a runner reaches the timeline, beside
// `fleet-stream-frames.ts`: `turn_admitted` opens a waiting row the moment the
// daemon accepts a message, and `turn_received` for the same turn starts it.
// The two can arrive in either order — a runner can run a send and announce
// it before the route publishes — so an admitted frame for a row already held
// fills only an empty text and never moves the row back to waiting.

type AdmittedFrame = Extract<LiveFrame, { kind: typeof FRAME_KIND.TURN_ADMITTED }>;
type ReceivedFrame = Extract<LiveFrame, { kind: typeof FRAME_KIND.TURN_RECEIVED }>;

export function applyTurnAdmitted(prev: FleetTurn[], frame: AdmittedFrame): FleetTurn[] {
  const message = text(frame.message);
  const index = prev.findIndex((turn) => turn.id === frame.turn_id);
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
export function receivedUpdate(existing: FleetTurn, frame: ReceivedFrame): FleetTurn | null {
  const message = text(frame.message);
  const fillsText = existing.text.length === 0 && message.length > 0;
  const starts = awaitsOwnRun(existing);
  if (!fillsText && !starts) return null;
  return {
    ...existing,
    ...(fillsText ? { text: message } : {}),
    ...(starts
      ? {
          status: AGENTSFLEET_TURN_STATUS.RECEIVED,
          outcome: outcomeForStatus(AGENTSFLEET_TURN_STATUS.RECEIVED),
        }
      : {}),
  };
}

/** A waiting row a recovered "received" history row starts: the received frame
 * fell in a gap. Any other row is returned as it is. */
export function startWaiting(turn: FleetTurn): FleetTurn {
  if (!awaitsOwnRun(turn)) return turn;
  return {
    ...turn,
    status: AGENTSFLEET_TURN_STATUS.RECEIVED,
    outcome: outcomeForStatus(AGENTSFLEET_TURN_STATUS.RECEIVED),
  };
}

// ── internals ────────────────────────────────────────────────────────────

// A row that has not begun its own run: queued, or labelled folded by a 202
// whose live run then ended without claiming it. The daemon runs such a
// message like any queued one, so its received frame must start it; left
// labelled, the row would read "Added to the current run" through its own run.
function awaitsOwnRun(turn: FleetTurn): boolean {
  return turn.status === AGENTSFLEET_TURN_STATUS.QUEUED || turn.status === AGENTSFLEET_TURN_STATUS.FOLDED;
}

function waitingRow(frame: AdmittedFrame, message: string): FleetTurn {
  const createdAt = instantMs(frame.created_at);
  return {
    id: frame.turn_id,
    role: roleFor(frame.actor),
    actor: frame.actor,
    text: message,
    reply: "",
    outcome: outcomeForStatus(AGENTSFLEET_TURN_STATUS.QUEUED),
    failureLabel: null,
    failureDetail: null,
    createdAt: createdAt === null ? new Date() : new Date(createdAt),
    status: AGENTSFLEET_TURN_STATUS.QUEUED,
  };
}

// The row at `index` re-stamped with the daemon's instant, or `prev` itself
// when the frame carried none or the row already has it.
export function adoptInstant(
  prev: FleetTurn[],
  index: number,
  existing: FleetTurn,
  createdAt: number | null,
): FleetTurn[] {
  if (createdAt === null) return prev;
  if (existing.createdAt.getTime() === createdAt && !existing.clientTimestamp) return prev;
  const updated = [...prev];
  updated[index] = { ...existing, createdAt: new Date(createdAt), clientTimestamp: false };
  return updated;
}
