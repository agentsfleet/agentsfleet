import type { LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import type { FleetEvent, FleetToolCall } from "./fleet-stream-row";
import { readToolArgs, readToolOutcome, type ToolArgs, type ToolOutcome } from "./fleet-stream-tool-trace";

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
// Only a call's opening, its completion, and a repeated start that adds
// arguments change the timeline. The repeat merges its arguments into the open
// call and keeps the first start's clock; progress on an open call returns it
// unchanged: nothing renders a running call's elapsed, and each new array
// re-renders the thread. Arguments and outcome are read field by field
// (`fleet-stream-tool-trace`), so a malformed one is left off the call.

type ToolFrame = Extract<
  LiveFrame,
  { kind: typeof FRAME_KIND.TOOL_CALL_STARTED | typeof FRAME_KIND.TOOL_CALL_PROGRESS | typeof FRAME_KIND.TOOL_CALL_COMPLETED }
>;

type ToolStep = Omit<FleetToolCall, "startedAtMs"> & { opens: boolean };

// The fields a step adds to the call it lands on.
type StepFields = Pick<ToolStep, "args"> & ToolOutcome;

/** `prev` with `frame` folded in, or `prev` itself when the frame changes nothing. */
export function applyToolFrame(prev: FleetEvent[], frame: ToolFrame, nowMs: number): FleetEvent[] {
  const step = readToolStep(frame);
  return step === null ? prev : applyToolStep(prev, frame.event_id, step, nowMs);
}

function readToolStep(frame: ToolFrame): ToolStep | null {
  const { name, call_id } = frame as { name: unknown; call_id?: unknown };
  if (typeof name !== "string" || name.length === 0) return null;
  // A call id that is not a non-empty string names nothing; the frame pairs by timing.
  const named = typeof call_id === "string" && call_id.length > 0 ? { name, callId: call_id } : { name };
  switch (frame.kind) {
    case FRAME_KIND.TOOL_CALL_STARTED:
      return { ...named, ms: null, done: false, opens: true, ...argsOf(readToolArgs(frame.args_redacted)) };
    case FRAME_KIND.TOOL_CALL_PROGRESS:
      return timed(named, frame.elapsed_ms, false, {});
    default:
      return timed(named, frame.ms, true, readToolOutcome(frame));
  }
}

function argsOf(args: ToolArgs | undefined): Pick<ToolStep, "args"> {
  return args === undefined ? {} : { args };
}

// A timing that is absent reads as none — a completion with no figure must not
// erase an elapsed the call already holds. One that is present and not a
// finite, non-negative number is a malformed frame.
function timed(named: Pick<ToolStep, "name" | "callId">, value: unknown, done: boolean, outcome: ToolOutcome): ToolStep | null {
  if (value === undefined || value === null) return { ...named, ms: null, done, opens: false, ...outcome };
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) return null;
  return { ...named, ms: value, done, opens: false, ...outcome };
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
  return call.done ? tools : moved(tools, at, call, step);
}

function byTiming(tools: FleetToolCall[], step: ToolStep, nowMs: number): FleetToolCall[] {
  const open = tools.findIndex((t) => t.name === step.name && !t.done);
  const call = tools[open];
  if (call !== undefined) return moved(tools, open, call, step);
  if (!step.opens && restatesFinished(tools, step.name, step.ms, step.done)) return tools;
  return [...tools, called(step, nowMs)];
}

function called({ opens: _opens, ...call }: ToolStep, nowMs: number): FleetToolCall {
  return { ...call, startedAtMs: nowMs };
}

// Whether a frame could have come from a finished call of this name. A call
// that finished without a figure cannot be told from a new one, so it is.
function restatesFinished(tools: FleetToolCall[], name: string, ms: number | null, done: boolean): boolean {
  return tools.some((t) => t.name === name && (ms === null || t.ms === null || (done ? ms === t.ms : ms <= t.ms)));
}

// A completion moves an open call and lands its outcome; a repeated start lands
// arguments the first one lacked. Progress moves nothing.
function moved(tools: FleetToolCall[], open: number, call: FleetToolCall, step: ToolStep): FleetToolCall[] {
  if (step.opens) return withArgs(tools, open, call, step.args);
  if (!step.done) return tools;
  const { name: _name, callId: _callId, ms, done, opens: _opens, ...fields } = step;
  return replaced(tools, open, { ...call, ...(fields satisfies StepFields), ms: ms ?? call.ms, done });
}

function withArgs(tools: FleetToolCall[], open: number, call: FleetToolCall, args: ToolArgs | undefined): FleetToolCall[] {
  if (args === undefined) return tools;
  const merged = { ...call.args, ...args };
  return JSON.stringify(merged) === JSON.stringify(call.args) ? tools : replaced(tools, open, { ...call, args: merged });
}

function replaced(tools: FleetToolCall[], at: number, call: FleetToolCall): FleetToolCall[] {
  return tools.map((t, i) => (i === at ? call : t));
}
