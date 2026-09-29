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
// A frame that names its call (`call_id`) pairs with that call: an id never
// seen here is a call whose start this subscriber missed, so it opens — even a
// second call of a tool that already finished — and a frame for a finished call
// changes nothing.
//
// A frame from a runner that names no call is keyed by (event, name) and
// matched to the open call of that name. A started frame opens a call and a
// completion moves the open one. With no open call, a frame for a name this
// event already finished is weighed by its timing: one that could restate the
// finished call — its figure, no figure, or progress no further along than it
// ended — changes nothing, since a fabricated second call is worse than a
// missed update. One past that is a second call whose start this subscriber
// missed after a reconnect, and it shows. A name never seen here means the
// start was missed (a subscriber that joined mid-call), and the call opens.
//
// Only a call's opening and its completion change the timeline. A repeated
// start, or progress on an open call, returns it unchanged: nothing renders a
// running call's elapsed, and each new array re-renders the thread.

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
  const { name, call_id } = frame as { name: unknown; call_id?: unknown };
  if (typeof name !== "string" || name.length === 0) return null;
  // A call id that is not a non-empty string names nothing; the frame pairs by timing.
  const callId = typeof call_id === "string" && call_id.length > 0 ? call_id : undefined;
  switch (frame.kind) {
    case FRAME_KIND.TOOL_CALL_STARTED:
      return { name, callId, ms: null, done: false, opens: true };
    case FRAME_KIND.TOOL_CALL_PROGRESS:
      return timed(name, callId, frame.elapsed_ms, false);
    default:
      return timed(name, callId, frame.ms, true);
  }
}

// A timing that is absent reads as none — a completion with no figure must not
// erase an elapsed the call already holds. One that is present and not a
// finite, non-negative number is a malformed frame.
function timed(name: string, callId: string | undefined, value: unknown, done: boolean): ToolStep | null {
  if (value === undefined || value === null) return { name, callId, ms: null, done, opens: false };
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) return null;
  return { name, callId, ms: value, done, opens: false };
}

/// A frame whose event has not arrived is dropped: `event_received` always
/// precedes its tool calls, and an invented event would put a message in the
/// thread the backfill duplicates. The first frame of a call stamps its start.
function applyToolStep(
  prev: FleetEvent[],
  eventId: string,
  step: ToolStep,
  nowMs: number,
): FleetEvent[] {
  const index = prev.findIndex((e) => e.id === eventId);
  const event = prev[index];
  // Narrowed, not asserted: `index === -1` and `event === undefined` are the same
  // fact, and letting the type system see it is cheaper than promising it.
  if (event === undefined) return prev;

  const tools = event.tools ?? [];
  const merged = step.callId === undefined ? byTiming(tools, step, nowMs) : byCallId(tools, step, nowMs);
  if (merged === tools) return prev;

  const updated = [...prev];
  updated[index] = { ...event, tools: merged };
  return updated;
}

function byCallId(tools: FleetToolCall[], step: ToolStep, nowMs: number): FleetToolCall[] {
  const at = tools.findIndex((t) => t.callId === step.callId);
  const call = tools[at];
  if (call === undefined) return [...tools, called(step, nowMs)];
  return call.done ? tools : moved(tools, at, step.ms, step.done);
}

function byTiming(tools: FleetToolCall[], step: ToolStep, nowMs: number): FleetToolCall[] {
  const open = tools.findIndex((t) => t.name === step.name && !t.done);
  if (open !== -1) return moved(tools, open, step.ms, step.done);
  if (!step.opens && restatesFinished(tools, step.name, step.ms, step.done)) return tools;
  return [...tools, called(step, nowMs)];
}

function called({ name, callId, ms, done }: ToolStep, nowMs: number): FleetToolCall {
  return callId === undefined ? { name, startedAtMs: nowMs, ms, done } : { name, callId, startedAtMs: nowMs, ms, done };
}

// Whether a frame could have come from a finished call of this name. A call
// that finished without a figure cannot be told from a new one, so it is.
function restatesFinished(tools: FleetToolCall[], name: string, ms: number | null, done: boolean): boolean {
  return tools.some((t) => t.name === name && (ms === null || t.ms === null || (done ? ms === t.ms : ms <= t.ms)));
}

// Only a completion moves an open call.
function moved(tools: FleetToolCall[], open: number, ms: number | null, done: boolean): FleetToolCall[] {
  const call = tools[open];
  if (call === undefined || !done) return tools;
  return tools.map((t, i) => (i === open ? { ...t, ms: ms ?? call.ms, done } : t));
}
