import { FRAME_KIND, type EventRow, type LiveFrame } from "@/lib/api/events";
import {
  ACTOR,
  EVENT_STATUS,
  outcomeFor,
  outcomeForStatus,
  replyBodyFor,
  roleFor,
  triggerBodyFor,
} from "@/lib/events/event-summary";

// Pure frame-transform helpers shared by the streaming registry.
// Nothing here touches Map state, EventSource, or React. Splitting
// these out keeps the registry's lifecycle file under the LENGTH GATE
// and the helpers unit-testable without spinning up a subscription.

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

// One tool the fleet called while working an event. The backend has always
// published `tool_call_started` / `_progress` / `_completed` frames; the reducer
// below dropped all three on the floor via a `default: return prev`, while the
// thread's own empty state promised "Tool calls, chunks, and completions appear
// here as the fleet runs." The frames were arriving and being discarded.
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
  createdAt: Date;
  status: FleetEventStatus;
  /** Tools called while working this event, in first-seen order. */
  tools?: FleetToolCall[];
  custom?: { requestJson?: string | null };
};

export function mergeBackfill(
  prev: FleetEvent[],
  rows: EventRow[],
): FleetEvent[] {
  const seen = new Set(prev.map((e) => e.id));
  // A terminal backfill row is authoritative over a live row with the same
  // id — an event that straddled an outage may sit here as a partial chunk
  // accumulation, and the durable row carries the full final text + status.
  // An in-progress ("received") backfill row never clobbers live chunks:
  // the live accumulation is newer than the list snapshot.
  const authoritative = new Map<string, EventRow>();
  for (const r of rows) {
    if (seen.has(r.event_id) && r.status !== AGENTSFLEET_EVENT_STATUS.RECEIVED) {
      authoritative.set(r.event_id, r);
    }
  }
  const kept = prev.map((e) => {
    const replacement = authoritative.get(e.id);
    if (!replacement) return e;
    const reconciled = rowToEvent(replacement);
    return e.tools ? { ...reconciled, tools: e.tools } : reconciled;
  });
  const fromBackfill = rows.filter((r) => !seen.has(r.event_id)).map(rowToEvent);
  return [...fromBackfill, ...kept].sort(
    (a, b) => a.createdAt.getTime() - b.createdAt.getTime(),
  );
}

// The newest server-confirmed `created_at` across the rows, folded into the
// running watermark. Live SSE frames are stamped with the CLIENT clock and
// must never advance this — a skewed client would push the backfill's lower
// bound into the server's future and silently recover nothing.
export function maxServerCreatedAt(
  current: number | null,
  rows: EventRow[],
): number | null {
  let max = current;
  for (const r of rows) {
    if (typeof r.created_at === "number" && (max === null || r.created_at > max)) {
      max = r.created_at;
    }
  }
  return max;
}

// Epoch ms → the 20-char `YYYY-MM-DDTHH:MM:SSZ` shape the upstream `since`
// parser accepts (no fractional seconds).
export function rfc3339Seconds(ms: number): string {
  return `${new Date(Math.max(ms, 0)).toISOString().slice(0, 19)}Z`;
}

export function applyLiveFrame(
  prev: FleetEvent[],
  frame: LiveFrame,
): FleetEvent[] {
  switch (frame.kind) {
    case FRAME_KIND.EVENT_RECEIVED:
      return applyEventReceived(prev, frame);
    case FRAME_KIND.CHUNK:
      return applyChunk(prev, frame);
    case FRAME_KIND.EVENT_COMPLETE:
      return applyEventComplete(prev, frame);
    case FRAME_KIND.TOOL_CALL_STARTED:
      return applyToolCall(prev, frame.event_id, frame.name, null, false);
    case FRAME_KIND.TOOL_CALL_PROGRESS:
      return applyToolCall(prev, frame.event_id, frame.name, frame.elapsed_ms, false);
    case FRAME_KIND.TOOL_CALL_COMPLETED:
      return applyToolCall(prev, frame.event_id, frame.name, frame.ms, true);
    default:
      // Install frames are forked off this path by the registry and never reach
      // the message list. Anything else is a frame the backend shipped ahead of
      // us — ignoring it is correct, but ONLY because it is genuinely unknown.
      // A frame we know about and drop here is the bug this switch just fixed.
      return prev;
  }
}

/// Fold one tool-call frame onto its event. The three frames are the same tool
/// seen at three moments, keyed by (event_id, name) — started has no timing yet,
/// progress carries elapsed, completed carries the final wall time. A frame whose
/// event has not arrived yet is dropped rather than synthesizing an orphan event:
/// `event_received` always precedes its tool calls on the wire, and inventing an
/// event here would put a message in the thread that the backfill would then
/// duplicate.
function applyToolCall(
  prev: FleetEvent[],
  eventId: string,
  name: string,
  ms: number | null,
  done: boolean,
): FleetEvent[] {
  const index = prev.findIndex((e) => e.id === eventId);
  const event = prev[index];
  // Narrowed, not asserted: `index === -1` and `event === undefined` are the same
  // fact, and letting the type system see it is cheaper than promising it.
  if (event === undefined) return prev;

  const tools = event.tools ?? [];
  const existing = tools.findIndex((t) => t.name === name && !t.done);

  const next: FleetToolCall = { name, ms, done };
  const merged =
    existing === -1
      ? [...tools, next]
      : tools.map((t, i) =>
          // A completion with no timing must not erase the elapsed a progress
          // frame already reported.
          i === existing ? { name, ms: ms ?? t.ms, done } : t,
        );

  const updated = [...prev];
  updated[index] = { ...event, tools: merged };
  return updated;
}

// ── internals ────────────────────────────────────────────────────────────

// A durable row becomes a rendered turn: the trigger (from the actor + request
// payload) and the fleet's reply (from response_text on the same row). Neither
// clobbers the other, so an operator's own message survives reload and the
// fleet's answer is never dropped or attributed to the operator.
function rowToEvent(row: EventRow): FleetEvent {
  return {
    id: row.event_id,
    role: roleFor(row.actor),
    actor: row.actor,
    text: triggerBodyFor(row),
    reply: replyBodyFor(row),
    outcome: outcomeFor(row),
    createdAt: new Date(row.created_at),
    status: row.status as FleetEventStatus,
    custom: { requestJson: row.request_json },
  };
}

function applyEventReceived(
  prev: FleetEvent[],
  frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.EVENT_RECEIVED }>,
): FleetEvent[] {
  if (prev.some((e) => e.id === frame.event_id)) return prev;
  return [
    ...prev,
    {
      id: frame.event_id,
      role: roleFor(frame.actor),
      actor: frame.actor,
      // The frame carries no payload and no event type, so the trigger comes
      // from the actor alone. A steer renders empty here until reconciliation
      // grafts the operator's text; anything else gets the neutral "Event
      // received" floor — fabricating `event_type: "chat"` would caption a
      // webhook or cron trigger as "chat received" until reload.
      text: triggerBodyFor({
        actor: frame.actor,
        request_json: "{}",
        event_type: "",
      }),
      reply: "",
      outcome: outcomeForStatus(AGENTSFLEET_EVENT_STATUS.RECEIVED),
      createdAt: new Date(),
      status: AGENTSFLEET_EVENT_STATUS.RECEIVED,
    },
  ];
}

function applyChunk(
  prev: FleetEvent[],
  frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.CHUNK }>,
): FleetEvent[] {
  const existing = prev.find((e) => e.id === frame.event_id);
  if (!existing) {
    // A chunk with no prior trigger row: the fleet is replying to something the
    // client never saw the receipt for. The chunk text is the reply, and the
    // trigger stays empty rather than mislabelling the reply as the trigger.
    return [
      ...prev,
      {
        id: frame.event_id,
        role: "assistant",
        actor: ACTOR.FLEET,
        text: "",
        reply: frame.text,
        outcome: outcomeForStatus(AGENTSFLEET_EVENT_STATUS.RECEIVED),
        createdAt: new Date(),
        status: AGENTSFLEET_EVENT_STATUS.RECEIVED,
      },
    ];
  }
  // Chunks are the fleet's reply — they accumulate into `reply`, never into the
  // trigger `text`, so the operator's own message is not overwritten by the
  // answer streaming back.
  return prev.map((e) =>
    e === existing ? { ...e, reply: e.reply + frame.text } : e,
  );
}

function applyEventComplete(
  prev: FleetEvent[],
  frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.EVENT_COMPLETE }>,
): FleetEvent[] {
  const existing = prev.find((e) => e.id === frame.event_id);
  if (!existing) return prev;
  const status = (frame.status ?? AGENTSFLEET_EVENT_STATUS.PROCESSED) as FleetEventStatus;
  // The outcome follows the status: an event that completes with no text must
  // stop saying it is still working.
  return prev.map((e) =>
    e === existing ? { ...e, status, outcome: outcomeForStatus(status) } : e,
  );
}
