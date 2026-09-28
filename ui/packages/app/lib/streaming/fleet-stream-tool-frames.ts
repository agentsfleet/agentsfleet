import type { LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import type { FleetEvent, FleetToolCall } from "./fleet-stream-row";

// The three tool frames folded onto their event, split out of
// `fleet-stream-frames` because a tool frame needs more than the others do: it
// is untrusted at the field level, and it can repeat.
//
// `parseLiveFrame` checks only that `kind` is a string, and the tool row renders
// `name` as a React child — an object there throws and takes the thread down.
// So the fields are read here, and a frame whose name is not a string, or whose
// timing is not a number, changes nothing.
//
// The wire carries no call identity, so a call is keyed by (event, name) and
// matched to the open call of that name. A started frame opens a call; progress
// and completion move the open one. With no open call, a frame for a name this
// event already finished is weighed by its timing: one that could restate the
// finished call — its figure, no figure, or progress no further along than it
// ended — changes nothing, since a fabricated second call is worse than a
// missed update. One past that is a second call whose start this subscriber
// missed after a reconnect, and it shows. A name never seen here means the
// start was missed (a subscriber that joined mid-call), and the call opens.

type ToolFrame = Extract<
  LiveFrame,
  { kind: typeof FRAME_KIND.TOOL_CALL_STARTED | typeof FRAME_KIND.TOOL_CALL_PROGRESS | typeof FRAME_KIND.TOOL_CALL_COMPLETED }
>;

type ToolStep = Omit<FleetToolCall, "startedAtMs"> & { opens: boolean };

/** `prev` with `frame` folded in, or `prev` itself when the frame changes nothing. */
export function applyToolFrame(prev: FleetEvent[], frame: ToolFrame, nowMs: number): FleetEvent[] {
  const step = readToolStep(frame);
  return step === null ? prev : applyToolStep(prev, frame.event_id, step, nowMs);
}

function readToolStep(frame: ToolFrame): ToolStep | null {
  const { name } = frame as { name: unknown };
  if (typeof name !== "string" || name.length === 0) return null;
  switch (frame.kind) {
    case FRAME_KIND.TOOL_CALL_STARTED:
      return { name, ms: null, done: false, opens: true };
    case FRAME_KIND.TOOL_CALL_PROGRESS:
      return timed(name, frame.elapsed_ms, false);
    default:
      return timed(name, frame.ms, true);
  }
}

// A timing that is absent reads as none — a completion with no figure must not
// erase the elapsed a progress frame reported. One that is present and not a
// finite, non-negative number is a malformed frame.
function timed(name: string, value: unknown, done: boolean): ToolStep | null {
  if (value === undefined || value === null) return { name, ms: null, done, opens: false };
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) return null;
  return { name, ms: value, done, opens: false };
}

/// A frame whose event has not arrived is dropped: `event_received` always
/// precedes its tool calls, and an invented event would put a message in the
/// thread the backfill duplicates. The first frame of a call stamps its start.
function applyToolStep(
  prev: FleetEvent[],
  eventId: string,
  { name, ms, done, opens }: ToolStep,
  nowMs: number,
): FleetEvent[] {
  const index = prev.findIndex((e) => e.id === eventId);
  const event = prev[index];
  // Narrowed, not asserted: `index === -1` and `event === undefined` are the same
  // fact, and letting the type system see it is cheaper than promising it.
  if (event === undefined) return prev;

  const tools = event.tools ?? [];
  const open = tools.findIndex((t) => t.name === name && !t.done);
  const merged = open === -1
    ? opened(tools, { name, ms, done, opens }, nowMs)
    : moved(tools, open, ms, done);
  if (merged === tools) return prev;

  const updated = [...prev];
  updated[index] = { ...event, tools: merged };
  return updated;
}

function opened(tools: FleetToolCall[], { name, ms, done, opens }: ToolStep, nowMs: number): FleetToolCall[] {
  if (!opens && restatesFinished(tools, name, ms, done)) return tools;
  return [...tools, { name, startedAtMs: nowMs, ms, done }];
}

// Whether a frame could have come from a finished call of this name. A call
// that finished without a figure cannot be told from a new one, so it is.
function restatesFinished(tools: FleetToolCall[], name: string, ms: number | null, done: boolean): boolean {
  return tools.some((t) => t.name === name && (ms === null || t.ms === null || (done ? ms === t.ms : ms <= t.ms)));
}

function moved(tools: FleetToolCall[], open: number, ms: number | null, done: boolean): FleetToolCall[] {
  const call = tools[open];
  const nextMs = ms ?? call?.ms ?? null;
  // A repeated start, or a progress frame restating the elapsed, moves nothing.
  if (call === undefined || (call.ms === nextMs && call.done === done)) return tools;
  return tools.map((t, i) => (i === open ? { ...t, ms: nextMs, done } : t));
}
