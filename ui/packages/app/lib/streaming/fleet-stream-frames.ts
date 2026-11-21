import { applyTaskActivity } from "./fleet-stream-task-activity";
import { mergeTaskActivity, readTaskActivityHistory } from "../task-activity";
import { instantMs, isInstant } from "@/lib/api/decode";
import { type TurnRow, type LiveFrame } from "@/lib/api/turns";
import { FRAME_KIND } from "@/lib/api/turns-types";
import { readNanos } from "@/lib/api/nanos";
import {
  ACTOR,
  TURN_STATUS,
  outcomeForCompletion,
  outcomeForStatus,
  roleFor,
  triggerBodyFor,
} from "@/lib/turns/turn-summary";
import {
  AGENTSFLEET_TURN_STATUS,
  EMPTY_PAYLOAD,
  TERMINAL_STATUSES,
  closeReasoningSpan,
  figure,
  frameMessage,
  rowToTurn,
  sameTurn,
  text,
  type FleetTurn,
  type FleetTurnStatus,
} from "./fleet-stream-row";
import { applyToolFrame } from "./fleet-stream-tool-frames";
import { applyContextProgress } from "./fleet-stream-context";
import { interruptOpenCalls, settleTools } from "./fleet-stream-tool-trace";
import { adoptInstant, applyTurnAdmitted, receivedUpdate, startWaiting } from "./fleet-stream-admitted";

// Pure frame-transform helpers shared by the streaming registry: how each
// live frame folds into the timeline, and how a page of durable rows merges
// with it. Nothing here touches Map state, EventSource, or React. The row
// model itself lives in `fleet-stream-row.ts`.

// The statuses the server writes on a row; a completion naming any other
// spelling marks the turn done rather than leaving it working forever.
const SERVER_STATUSES: ReadonlySet<string> = new Set(Object.values(TURN_STATUS));

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
  prev: FleetTurn[],
  frame: LiveFrame,
  nowMs: number = Date.now(),
): FleetTurn[] {
  switch (frame.kind) {
    case FRAME_KIND.TASK_DECISION_STARTED:
    case FRAME_KIND.TASK_DECISION_COMPLETED:
    case FRAME_KIND.TASK_TRANSFER_PROGRESS:
      return applyTaskActivity(prev, frame);
    case FRAME_KIND.CONTEXT_PROGRESS:
      return applyContextProgress(prev, frame);
    case FRAME_KIND.TURN_ADMITTED:
      return applyTurnAdmitted(prev, frame);
    case FRAME_KIND.TURN_RECEIVED:
      return applyTurnReceived(prev, frame);
    case FRAME_KIND.CHUNK:
      return applyChunk(prev, frame);
    case FRAME_KIND.TURN_COMPLETE:
      return applyTurnComplete(prev, frame, nowMs);
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

function applyTurnReceived(
  prev: FleetTurn[],
  frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.TURN_RECEIVED }>,
): FleetTurn[] {
  // The daemon stamps the frame with the row's own instant; a frame without
  // one (a malformed payload) falls back to now rather than to an invalid
  // date, so the row still sorts and renders.
  const createdAt = instantMs(frame.created_at);
  const index = prev.findIndex((e) => e.id === frame.turn_id);
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
  const message = frameMessage(frame.message, frame.message_trimmed);
  return [
    ...prev,
    {
      id: frame.turn_id,
      role: roleFor(frame.actor),
      actor: frame.actor,
      // A steer's frame carries what was typed. Any other frame carries no
      // payload, so the trigger comes from the actor and the turn kind the
      // daemon recorded: a webhook or cron trigger gets its own neutral
      // headline rather than a chat caption.
      ...(message.text.length > 0 ? message : { text: triggerBodyFor({
        actor: frame.actor,
        request_json: EMPTY_PAYLOAD,
        event_type: typeof frame.event_type === "string" ? frame.event_type : "",
      }) }),
      reply: "",
      outcome: outcomeForStatus(AGENTSFLEET_TURN_STATUS.RECEIVED),
      failureLabel: null,
      failureDetail: null,
      createdAt: createdAt === null ? new Date() : new Date(createdAt),
      status: AGENTSFLEET_TURN_STATUS.RECEIVED,
    },
  ];
}

function applyChunk(
  prev: FleetTurn[],
  frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.CHUNK }>,
): FleetTurn[] {
  if (!["answer", "reasoning"].includes(frame.text_kind) || typeof frame.text !== "string") return prev;
  // Locate once and copy once, matching `applyToolStep` — a streaming reply
  // fires this per chunk, so a second full pass per frame is pure waste.
  const index = prev.findIndex((e) => e.id === frame.turn_id);
  const existing = prev[index];
  if (existing === undefined) {
    // A chunk with no prior trigger row: the fleet is replying to something the
    // client never saw the receipt for. The chunk text is the reply, and the
    // trigger stays empty rather than mislabelling the reply as the trigger.
    return [
      ...prev,
      {
        id: frame.turn_id,
        role: "assistant",
        actor: ACTOR.FLEET,
        text: "",
        reply: frame.text,
        outcome: outcomeForStatus(AGENTSFLEET_TURN_STATUS.RECEIVED),
        failureLabel: null,
        failureDetail: null,
        createdAt: new Date(),
        status: AGENTSFLEET_TURN_STATUS.RECEIVED,
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

function applyTurnComplete(
  prev: FleetTurn[],
  frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.TURN_COMPLETE }>,
  nowMs: number,
): FleetTurn[] {
  // Locate once, copy once — same shape as `applyToolStep` and `applyChunk`.
  const index = prev.findIndex((e) => e.id === frame.turn_id);
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
  const createdAt = instantMs(frame.created_at);
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
    contextProgress: undefined,
    tokens: figure(frame.tokens),
    wallMs: figure(frame.wall_ms),
    costNanos: readNanos(frame.cost_nanos),
  }), nowMs);
  return updated;
}

// The completion's status as a row status: a server spelling as sent, and
// anything else — absent, malformed, unknown — as processed.
function terminalStatus(value: unknown): FleetTurnStatus {
  return typeof value === "string" && SERVER_STATUSES.has(value)
    ? (value as FleetTurnStatus)
    : AGENTSFLEET_TURN_STATUS.PROCESSED;
}

// A completion carrying enough of a row to open one: the fields `rowToTurn`
// reads that have no fallback. Anything short of that is not a row.
function openFromCompletion(
  prev: FleetTurn[],
  frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.TURN_COMPLETE }>,
): FleetTurn[] {
  if (typeof frame.actor !== "string" || !isInstant(frame.created_at)) return prev;
  const row: TurnRow = {
    fleet_id: "",
    workspace_id: "",
    turn_id: frame.turn_id,
    actor: frame.actor,
    event_type: text(frame.event_type),
    status: terminalStatus(frame.status),
    tokens: figure(frame.tokens),
    wall_ms: figure(frame.wall_ms),
    failure_label: text(frame.failure_label) || null,
    failure_detail: text(frame.failure_detail) || null,
    resumes_turn_id: typeof frame.resumes_turn_id === "string" ? frame.resumes_turn_id : null,
    created_at: frame.created_at,
    updated_at: isInstant(frame.updated_at) ? frame.updated_at : frame.created_at,
    cost_nanos: readNanos(frame.cost_nanos),
  };
  return [...prev, rowToTurn(row)];
}

// ── the merge ────────────────────────────────────────────────────────────

export function mergeBackfill(
  prev: FleetTurn[],
  rows: TurnRow[],
  nowMs: number = Date.now(),
): FleetTurn[] {
  const seen = new Set(prev.map((e) => e.id));
  // A terminal backfill row is authoritative over a live row with the same
  // id for its STATUS and figures — a turn that straddled an outage may sit
  // here as a partial chunk accumulation, and the durable row is the settled
  // one. It is NOT authoritative for the bodies: the turns LIST carries
  // none (`afd_turns` history/statement/mod.rs asserts the select omits
  // `response_text`, and it omits `request_json` too), so taking the row
  // wholesale would blank a message the operator has already read. The live
  // text is kept for the same reason; the calls follow `settleTools`.
  // An in-progress backfill row ("received", or "queued" for a message still
  // waiting) never clobbers the live row: the stream is newer than the page.
  // The one exception moves forward only: a "received" row starts a live row
  // still waiting, whose received frame fell in the gap the page recovers.
  const authoritative = new Map<string, TurnRow>();
  const progress = new Map(rows.map(row => [row.turn_id, readTaskActivityHistory("task_activity" in row ? row.task_activity : undefined)]));
  const started = new Set<string>();
  for (const r of rows) {
    if (!seen.has(r.turn_id)) continue;
    if (TERMINAL_STATUSES.has(r.status)) authoritative.set(r.turn_id, r);
    else if (r.status === AGENTSFLEET_TURN_STATUS.RECEIVED) started.add(r.turn_id);
  }
  const kept = prev.map((e) => {
    const replacement = authoritative.get(e.id);
    if (!replacement) {
      const running = started.has(e.id) ? startWaiting(e) : e;
      const taskActivity = mergeTaskActivity(e.taskActivity, progress.get(e.id));
      return taskActivity === running.taskActivity ? running : {...running, taskActivity};
    }
    const reconciled = rowToTurn(replacement);
    // The reasoning span is the browser's own measure — no row carries it — so
    // it is kept, and a span still open closes here once: a terminal row is
    // the end of the turn, whether or not the answer's first word was seen.
    // A saved text replaces a frame's head of it; a list row has none, so the
    // head stays marked for the read that brings the whole.
    const withBodies = closeReasoningSpan({
      ...reconciled,
      ...(reconciled.text.length > 0 ? {} : { text: e.text, textTrimmed: e.textTrimmed }),
      reply: reconciled.reply.length > 0 ? reconciled.reply : e.reply,
      taskActivity: mergeTaskActivity(e.taskActivity, reconciled.taskActivity),
      runRestarted: e.runRestarted,
      reasoning: e.reasoning,
      reasoningStartedAtMs: e.reasoningStartedAtMs,
      reasoningEndedAtMs: e.reasoningEndedAtMs,
      // Unset stays unset, so a page restating the row compares equal to it.
      thinking: e.thinking === undefined ? undefined : false,
      custom: reconciled.text.length > 0 ? reconciled.custom : e.custom,
    }, nowMs);
    const settled = settleTools(withBodies, e, "tool_calls" in replacement);
    // A page restating a row this thread already settled leaves its object alone.
    return sameTurn(settled, e) ? e : settled;
  });
  const fromBackfill = rows.filter((r) => !seen.has(r.turn_id)).map(rowToTurn);
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
  return merged.length === prev.length && merged.every((turn, at) => turn === prev[at]) ? prev : merged;
}

// The newest server-confirmed `created_at` across the rows, folded into the
// running watermark. Only durable rows advance it — a live frame's instant is
// the daemon's, but the backfill that recovers a gap is keyed on rows the list
// has served, and the 2 s overlap it re-reads is cheaper than a watermark a
// dropped frame could push past the rows it never saw.
export function maxServerCreatedAt(
  current: number | null,
  rows: TurnRow[],
): number | null {
  let max = current;
  for (const r of rows) {
    const at = instantMs(r.created_at);
    if (at !== null && (max === null || at > max)) max = at;
  }
  return max;
}

// Epoch ms → the 20-char `YYYY-MM-DDTHH:MM:SSZ` shape the upstream `since`
// parser accepts (no fractional seconds).
export function rfc3339Seconds(ms: number): string {
  return `${new Date(Math.max(ms, 0)).toISOString().slice(0, 19)}Z`;
}
