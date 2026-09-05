import type { EventDetail, EventRow } from "@/lib/api/events";
import {
  EVENT_STATUS,
  outcomeFor,
  replyBodyFor,
  roleFor,
  triggerBodyFor,
} from "@/lib/events/event-summary";

// The row model the live timeline is made of, the one conversion from a
// durable row into it, and the two readers every wire value passes through.
// Split from the frame reducers so the reducers, the merge and the fleet
// facts can all build on it without importing one another: the model is the
// bottom of the streaming vocabulary.

/** A figure off the wire, or null for anything that is not a number. SSE
 * payloads are untrusted; a missing or malformed figure renders as unknown,
 * never as a fabricated zero. */
export function figure(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

/** A string off the wire, trimmed, or empty for anything that is not one — so
 * a malformed field renders as absent instead of throwing inside the
 * EventSource listener and losing the frame. */
export function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

// The server's durable statuses plus the two the browser owns: a submission
// awaiting its server identifier, and one the server refused.
export const AGENTSFLEET_EVENT_STATUS = {
  RECEIVED: EVENT_STATUS.RECEIVED,
  PROCESSED: EVENT_STATUS.PROCESSED,
  AGENT_ERROR: EVENT_STATUS.FLEET_ERROR,
  GATE_BLOCKED: EVENT_STATUS.GATE_BLOCKED,
  OPTIMISTIC: "optimistic",
  FAILED: "failed",
} as const;

export type FleetEventStatus =
  (typeof AGENTSFLEET_EVENT_STATUS)[keyof typeof AGENTSFLEET_EVENT_STATUS];

// One tool the fleet called while working an event, as the three tool frames
// describe it — started with no timing yet, progressing with elapsed time,
// completed with the final wall time.
export type FleetToolCall = {
  name: string;
  /** Wall time so far (from a progress frame) or final (from a completion). */
  ms: number | null;
  done: boolean;
};

export type FleetEvent = {
  id: string;
  role: "user" | "assistant" | "system";
  actor: string;
  /**
   * The trigger body — what woke the fleet (an operator's steer, a webhook
   * headline). Fixed at creation from the actor + request payload; the fleet's
   * reply never overwrites it. Empty for a row that is itself a reply.
   */
  text: string;
  /**
   * The fleet's reply on this same durable row (`response_text`), accumulated
   * from CHUNK frames while streaming. Empty until the fleet answers; the row
   * then renders `outcome` in the reply's place.
   */
  reply: string;
  /**
   * What the reply bubble says when `reply` is empty — the honest floor that
   * keeps a completed turn from rendering blank. Recomputed on status change.
   */
  outcome: string;
  /**
   * The runner's failure class for a failed turn (`startup_posture`, …), kept
   * beside the rendered `outcome` sentence because remediation guidance is
   * chosen by the CLASS, not by the sentence. Null on a clean or in-flight
   * turn — a row that has not failed has no class to carry.
   */
  failureLabel: string | null;
  /**
   * The recorded cause line for a failed turn, kept beside the class so a
   * summary above the thread can name WHICH check failed without re-parsing
   * it back out of the rendered outcome sentence.
   */
  failureDetail: string | null;
  createdAt: Date;
  status: FleetEventStatus;
  /**
   * The run's figures, as the daemon reported them: tokens spent, wall time,
   * and the summed telemetry cost. Absent on a row the browser assembled from
   * an opening frame or a chunk — the run has not reported — and null where
   * the daemon reported none; the strip renders both as unknown, never zero.
   */
  tokens?: number | null;
  wallMs?: number | null;
  costNanos?: number | null;
  /** Tools called while working this event, in first-seen order. */
  tools?: FleetToolCall[];
  custom?: { requestJson?: string | null };
};

/// The payload stand-in for a turn whose body is not on hand — a live frame
/// carries none, and a list row no longer does either.
export const EMPTY_PAYLOAD = "{}";

// A durable row becomes a rendered turn: the trigger (from the actor + request
// payload) and the fleet's reply (from response_text on the same row). Neither
// clobbers the other, so an operator's own message survives reload and the
// fleet's answer is never dropped or attributed to the operator.
export function rowToEvent(row: EventRow | EventDetail): FleetEvent {
  // A backfill row may or may not carry bodies. The events LIST carries none —
  // it is kept off oversized-attribute storage — so a turn reconstructed from
  // it renders its header and outcome, and its text arrives from the live
  // stream or from the single-event read. A caller that already holds details
  // passes them and nothing is lost.
  const bodies = row as Partial<EventDetail>;
  const request_json = bodies.request_json ?? EMPTY_PAYLOAD;
  return {
    id: row.event_id,
    role: roleFor(row.actor),
    actor: row.actor,
    text: triggerBodyFor({ actor: row.actor, event_type: row.event_type, request_json }),
    reply: replyBodyFor({ response_text: bodies.response_text ?? null }),
    outcome: outcomeFor(row),
    failureLabel: row.failure_label ?? null,
    failureDetail: row.failure_detail ?? null,
    createdAt: new Date(row.created_at),
    status: row.status as FleetEventStatus,
    tokens: figure(row.tokens),
    wallMs: figure(row.wall_ms),
    costNanos: figure(row.cost_nanos),
    custom: { requestJson: request_json },
  };
}
