import { type EventRow, type LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import {
  ACTOR,
  EVENT_STATUS,
  outcomeForCompletion,
  outcomeForStatus,
  roleFor,
  triggerBodyFor,
} from "@/lib/events/event-summary";
import {
  AGENTSFLEET_EVENT_STATUS,
  EMPTY_PAYLOAD,
  TERMINAL_STATUSES,
  closeReasoningSpan,
  figure,
  rowToEvent,
  sameEvent,
  text,
  type FleetEvent,
  type FleetEventStatus,
} from "./fleet-stream-row";
import { applyToolFrame } from "./fleet-stream-tool-frames";
import { interruptOpenCalls, settleTools } from "./fleet-stream-tool-trace";
import { applyEventAdmitted, receivedUpdate, startWaiting } from "./fleet-stream-admitted";

// Pure frame-transform helpers shared by the streaming registry: how each
// live frame folds into the timeline, and how a page of durable rows merges
// with it. Nothing here touches Map state, EventSource, or React. The row
// model itself lives in `fleet-stream-row.ts`.

// The statuses the server writes on a row; a completion naming any other
// spelling marks the turn done rather than leaving it working forever.
const SERVER_STATUSES: ReadonlySet<string> = new Set(Object.values(EVENT_STATUS));

export function parseLiveFrame(data: string): LiveFrame | null {
  try {
    const parsed: unknown = JSON.parse(data);
    if (!parsed || typeof parsed !== "object" || typeof (parsed as { kind?: unknown }).kind !== "string") return null;
    return parsed as LiveFrame;
  } catch {
    return null;
  }
}

/** `nowMs` is the browser clock the reducers stamp spans with; tests inject it. */
export function applyLiveFrame(
  prev: FleetEvent[],
  frame: LiveFrame,
  nowMs: number = Date.now(),
): FleetEvent[] {
  switch (frame.kind) {
    case FRAME_KIND.EVENT_ADMITTED:
      return applyEventAdmitted(prev, frame);
    case FRAME_KIND.EVENT_RECEIVED:
      return applyEventReceived(prev, frame);
    case FRAME_KIND.CHUNK:
      return applyChunk(prev, frame);
    case FRAME_KIND.EVENT_COMPLETE:
      return applyEventComplete(prev, frame, nowMs);
    case FRAME_KIND.TOOL_CALL_STARTED:
    case FRAME_KIND.TOOL_CALL_PROGRESS:
    case FRAME_KIND.TOOL_CALL_COMPLETED:
      return applyToolFrame(prev, frame, nowMs);
    default:
      // Install frames are forked off this path by the registry and never reach
      // the message list. Anything else is a frame the backend shipped ahead of
      // us — ignoring it is correct, but ONLY because it is genuinely unknown.
      // A frame we know about and drop here is the bug this switch just fixed.
      return prev;
  }
}

// ── internals ────────────────────────────────────────────────────────────

function applyEventReceived(
  prev: FleetEvent[],
  frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.EVENT_RECEIVED }>,
): FleetEvent[] {
  // The daemon stamps the frame with the row's own instant; a frame without
  // one (a malformed payload) falls back to now rather than to an invalid
  // date, so the row still sorts and renders.
  const createdAt = figure(frame.created_at);
  const index = prev.findIndex((e) => e.id === frame.event_id);
  const existing = prev[index];
  // A row the browser already holds — the operator's own steer, reconciled
  // to its identifier before the daemon opened it — keeps everything but its
  // instant: that was the client clock's guess, and the row's own is what the
  // strip orders the newest run by. A row waiting for a runner starts here.
  if (existing !== undefined) {
    const started = receivedUpdate(existing, frame);
    if (started === null) return adoptInstant(prev, index, existing, createdAt);
    const updated = [...prev];
    updated[index] = started;
    return adoptInstant(updated, index, started, createdAt);
  }
  const message = text(frame.message);
  return [
    ...prev,
    {
      id: frame.event_id,
      role: roleFor(frame.actor),
      actor: frame.actor,
      // A steer's frame carries what was typed. Any other frame carries no
      // payload, so the trigger comes from the actor and the event type the
      // daemon recorded: a webhook or cron trigger gets its own neutral
      // headline rather than a chat caption.
      text: message.length > 0 ? message : triggerBodyFor({
        actor: frame.actor,
        request_json: EMPTY_PAYLOAD,
        event_type: typeof frame.event_type === "string" ? frame.event_type : "",
      }),
      reply: "",
      outcome: outcomeForStatus(AGENTSFLEET_EVENT_STATUS.RECEIVED),
      failureLabel: null,
      failureDetail: null,
      createdAt: createdAt === null ? new Date() : new Date(createdAt),
      status: AGENTSFLEET_EVENT_STATUS.RECEIVED,
    },
  ];
}

// The row at `index` re-stamped with the daemon's instant, or `prev` itself
// when the frame carried none or the row already has it.
function adoptInstant(
  prev: FleetEvent[],
  index: number,
  existing: FleetEvent,
  createdAt: number | null,
): FleetEvent[] {
  if (createdAt === null) return prev;
  if (existing.createdAt.getTime() === createdAt && !existing.clientTimestamp) return prev;
  const updated = [...prev];
  updated[index] = { ...existing, createdAt: new Date(createdAt), clientTimestamp: false };
  return updated;
}

function applyChunk(
  prev: FleetEvent[],
  frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.CHUNK }>,
): FleetEvent[] {
  // Locate once and copy once, matching `applyToolStep` — a streaming reply
  // fires this per chunk, so a second full pass per frame is pure waste.
  const index = prev.findIndex((e) => e.id === frame.event_id);
  const existing = prev[index];
  if (existing === undefined) {
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
        failureLabel: null,
        failureDetail: null,
        createdAt: new Date(),
        status: AGENTSFLEET_EVENT_STATUS.RECEIVED,
      },
    ];
  }
  // Chunks are the fleet's reply — they accumulate into `reply`, never into the
  // trigger `text`, so the operator's own message is not overwritten by the
  // answer streaming back.
  const updated = [...prev];
  updated[index] = { ...existing, reply: existing.reply + frame.text };
  return updated;
}

function applyEventComplete(
  prev: FleetEvent[],
  frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.EVENT_COMPLETE }>,
  nowMs: number,
): FleetEvent[] {
  // Locate once, copy once — same shape as `applyToolStep` and `applyChunk`.
  const index = prev.findIndex((e) => e.id === frame.event_id);
  const existing = prev[index];
  // A completion for a row the timeline never opened — a subscriber that
  // connected after the opening, a continued run whose row the resolve wrote,
  // an opening frame the queue dropped — opens it here: the frame carries the
  // whole row, so the turn and the strip's figures land without a read. A
  // frame too malformed to be a row is dropped, never rendered as a blank.
  if (existing === undefined) return openFromCompletion(prev, frame);
  // SSE payloads are untrusted: a completion with no readable status still
  // marks the turn done rather than leaving it working forever.
  const status = terminalStatus(frame.status);
  // The outcome follows the status — and carries the failure cause the frame
  // ships, so a failed turn names its check live instead of a generic floor
  // until reload. The class rides alongside so guidance renders live too.
  const label = text(frame.failure_label);
  const detail = text(frame.failure_detail);
  const createdAt = figure(frame.created_at);
  const updated = [...prev];
  // A call still open when its turn ends was cut off, so it reads interrupted.
  updated[index] = closeReasoningSpan(interruptOpenCalls({
    ...existing,
    status,
    outcome: outcomeForCompletion(status, label, detail),
    failureLabel: label.length > 0 ? label : null,
    failureDetail: detail.length > 0 ? detail : null,
    // The row's own instant and figures ride the frame, so the strip orders
    // and moves without a read.
    createdAt: createdAt === null ? existing.createdAt : new Date(createdAt),
    clientTimestamp: createdAt === null ? existing.clientTimestamp : false,
    thinking: false,
    tokens: figure(frame.tokens),
    wallMs: figure(frame.wall_ms),
    costNanos: figure(frame.cost_nanos),
  }), nowMs);
  return updated;
}

// The completion's status as a row status: a server spelling as sent, and
// anything else — absent, malformed, unknown — as processed.
function terminalStatus(value: unknown): FleetEventStatus {
  return typeof value === "string" && SERVER_STATUSES.has(value)
    ? (value as FleetEventStatus)
    : AGENTSFLEET_EVENT_STATUS.PROCESSED;
}

// A completion carrying enough of a row to open one: the fields `rowToEvent`
// reads that have no fallback. Anything short of that is not a row.
function openFromCompletion(
  prev: FleetEvent[],
  frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.EVENT_COMPLETE }>,
): FleetEvent[] {
  const createdAt = figure(frame.created_at);
  if (typeof frame.actor !== "string" || createdAt === null) return prev;
  const row: EventRow = {
    fleet_id: "",
    workspace_id: "",
    event_id: frame.event_id,
    actor: frame.actor,
    event_type: text(frame.event_type),
    status: terminalStatus(frame.status),
    tokens: figure(frame.tokens),
    wall_ms: figure(frame.wall_ms),
    failure_label: text(frame.failure_label) || null,
    failure_detail: text(frame.failure_detail) || null,
    checkpoint_id: typeof frame.checkpoint_id === "string" ? frame.checkpoint_id : null,
    resumes_event_id: typeof frame.resumes_event_id === "string" ? frame.resumes_event_id : null,
    created_at: createdAt,
    updated_at: figure(frame.updated_at) ?? createdAt,
    cost_nanos: figure(frame.cost_nanos),
  };
  return [...prev, rowToEvent(row)];
}

// ── the merge ────────────────────────────────────────────────────────────

export function mergeBackfill(
  prev: FleetEvent[],
  rows: EventRow[],
  nowMs: number = Date.now(),
): FleetEvent[] {
  const seen = new Set(prev.map((e) => e.id));
  // A terminal backfill row is authoritative over a live row with the same
  // id for its STATUS and figures — an event that straddled an outage may sit
  // here as a partial chunk accumulation, and the durable row is the settled
  // one. It is NOT authoritative for the bodies: the events LIST carries
  // none (`afd_events` history/statement.rs asserts the select omits
  // `response_text`, and it omits `request_json` too), so taking the row
  // wholesale would blank a message the operator has already read. The live
  // text is kept for the same reason; the calls follow `settleTools`.
  // An in-progress backfill row ("received", or "queued" for a message still
  // waiting) never clobbers the live row: the stream is newer than the page.
  // The one exception moves forward only: a "received" row starts a live row
  // still waiting, whose received frame fell in the gap the page recovers.
  const authoritative = new Map<string, EventRow>();
  const started = new Set<string>();
  for (const r of rows) {
    if (!seen.has(r.event_id)) continue;
    if (TERMINAL_STATUSES.has(r.status)) authoritative.set(r.event_id, r);
    else if (r.status === AGENTSFLEET_EVENT_STATUS.RECEIVED) started.add(r.event_id);
  }
  const kept = prev.map((e) => {
    const replacement = authoritative.get(e.id);
    if (!replacement) return started.has(e.id) ? startWaiting(e) : e;
    const reconciled = rowToEvent(replacement);
    // The reasoning span is the browser's own measure — no row carries it — so
    // it is kept, and a span still open closes here once: a terminal row is
    // the end of the turn, whether or not the answer's first word was seen.
    const withBodies = closeReasoningSpan({
      ...reconciled,
      text: reconciled.text.length > 0 ? reconciled.text : e.text,
      reply: reconciled.reply.length > 0 ? reconciled.reply : e.reply,
      reasoning: e.reasoning,
      reasoningStartedAtMs: e.reasoningStartedAtMs,
      reasoningEndedAtMs: e.reasoningEndedAtMs,
      // Unset stays unset, so a page restating the row compares equal to it.
      thinking: e.thinking === undefined ? undefined : false,
      custom: reconciled.text.length > 0 ? reconciled.custom : e.custom,
    }, nowMs);
    const settled = settleTools(withBodies, e);
    // A page restating a row this thread already settled leaves its object alone.
    return sameEvent(settled, e) ? e : settled;
  });
  const fromBackfill = rows.filter((r) => !seen.has(r.event_id)).map(rowToEvent);
  // The client's clock can trail the server's. Keep locally submitted turns
  // at the visual tail until an opening or completion frame supplies the
  // server timestamp, even when the Server Action already acknowledged them.
  const merged = [...fromBackfill, ...kept].sort((a, b) => {
    if (a.clientTimestamp) {
      return b.clientTimestamp ? 0 : 1;
    }
    if (b.clientTimestamp) return -1;
    return a.createdAt.getTime() - b.createdAt.getTime();
  });
  // Nothing new, nothing moved: the same array, so no subscriber re-renders.
  return merged.length === prev.length && merged.every((event, at) => event === prev[at]) ? prev : merged;
}

// The newest server-confirmed `created_at` across the rows, folded into the
// running watermark. Only durable rows advance it — a live frame's instant is
// the daemon's, but the backfill that recovers a gap is keyed on rows the list
// has served, and the 2 s overlap it re-reads is cheaper than a watermark a
// dropped frame could push past the rows it never saw.
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
