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

type ToolStep = Omit<FleetToolCall, "startedAtMs" | "callId"> & { callId: string; opens: boolean };

// The fields a step adds to the call it lands on.
type StepFields = Pick<ToolStep, "args"> & ToolOutcome;

/** `prev` with `frame` folded in, or `prev` itself when the frame changes nothing. */
export function applyToolFrame(prev: FleetEvent[], frame: ToolFrame, nowMs: number): FleetEvent[] {
  const step = readToolStep(frame);
  return step === null ? prev : applyToolStep(prev, frame.event_id, step, nowMs);
}

function readToolStep(frame: ToolFrame): ToolStep | null {
  const { name, call_id } = frame;
  if (typeof name !== "string" || name.length === 0 || typeof call_id !== "string" || call_id.length === 0) return null;
  const named = { name, callId: call_id };
  switch (frame.kind) {
    case FRAME_KIND.TOOL_CALL_STARTED:
      return { ...named, ms: null, done: false, opens: true, ...argsOf(readToolArgs(frame.args_redacted)) };
    case FRAME_KIND.TOOL_CALL_PROGRESS:
      return timed(named, frame.elapsed_ms, false, {});
    default: {
      const outcome = readToolOutcome(frame);
      return outcome === null ? null : timed(named, frame.ms, true, outcome);
    }
  }
}

function argsOf(args: ToolArgs | undefined): Pick<ToolStep, "args"> {
  return args === undefined ? {} : { args };
}

// Required timings must be finite and non-negative.
function timed(named: Pick<ToolStep, "name" | "callId">, value: unknown, done: boolean, outcome: ToolOutcome): ToolStep | null {
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
  const merged = byCallId(tools, step, nowMs);
  if (merged === tools) return prev;

  const updated = [...prev];
  updated[index] = { ...event, tools: merged };
  return updated;
}

function byCallId(tools: FleetToolCall[], step: ToolStep, nowMs: number): FleetToolCall[] {
  const at = tools.findIndex((t) => t.callId === step.callId);
  const call = tools[at];
  if (call === undefined) return [...tools, called(step, nowMs)];
  return call.done && call.closedAtSettle !== true ? tools : moved(tools, at, call, step);
}

function called({ opens: _opens, ...call }: ToolStep, nowMs: number): FleetToolCall {
  return { ...call, startedAtMs: nowMs };
}

// A completion moves an open call and lands its outcome; a repeated start lands
// arguments the first one lacked. Progress moves nothing.
function moved(tools: FleetToolCall[], open: number, call: FleetToolCall, step: ToolStep): FleetToolCall[] {
  if (step.opens) return withArgs(tools, open, call, step.args);
  if (!step.done) return tools;
  const { name: _name, callId: _callId, ms, done, opens: _opens, ...fields } = step;
  return replaced(tools, open, { ...heard(call), ...(fields satisfies StepFields), ms, done });
}

// A completion that lands after its turn ended is the truth the turn guessed
// at: the recorded status replaces the interrupted guess.
function heard(call: FleetToolCall): FleetToolCall {
  if (call.closedAtSettle !== true) return call;
  const { closedAtSettle: _guessed, status: _guessedStatus, ...rest } = call;
  return rest;
}

function withArgs(tools: FleetToolCall[], open: number, call: FleetToolCall, args: ToolArgs | undefined): FleetToolCall[] {
  if (args === undefined) return tools;
  const merged = { ...call.args, ...args };
  return JSON.stringify(merged) === JSON.stringify(call.args) ? tools : replaced(tools, open, { ...call, args: merged });
}

function replaced(tools: FleetToolCall[], at: number, call: FleetToolCall): FleetToolCall[] {
  return tools.map((t, i) => (i === at ? call : t));
}
