import * as v from "valibot";

import type { FleetEvent, FleetToolCall } from "./fleet-stream-row";

// A tool call's arguments and outcome, narrowed where they cross into the
// browser: off a live frame, and off an event's saved trace. Each field
// narrows on its own, so a malformed one reads as absent and the call still
// renders without it — the frame's name and timing are what make a call, and
// `fleet-stream-tool-frames` reads those. Also where a settled turn's calls
// are decided: the saved trace when it has one, else the live rows with any
// still open closed. Valibot rather than zod: this module rides every fleet
// page, and zod's core cost each of them 79 kB gzipped against a 100 KiB route
// budget (`.size-limit.mjs`).

/** How a call ended, as `afd_wire::tool_trace::ToolCallStatus` spells it. */
export const TOOL_CALL_STATUS = {
  SUCCEEDED: "succeeded",
  FAILED: "failed",
  INTERRUPTED: "interrupted",
} as const;

export type ToolCallStatus = (typeof TOOL_CALL_STATUS)[keyof typeof TOOL_CALL_STATUS];

/** A call's status, narrowed off the wire. */
export const TOOL_CALL_STATUS_SCHEMA = v.picklist(Object.values(TOOL_CALL_STATUS));

/** A JSON value as `JSON.parse` hands it back, so arguments can reach the
 * library's tool-call part without a cast. */
export type JsonValue = string | number | boolean | null | readonly JsonValue[] | { readonly [key: string]: JsonValue };

export type ToolArgs = { readonly [key: string]: JsonValue };

/** What a finished call reported. Each field is absent when the runner sent
 * none, as an older runner does. */
export type ToolOutcome = {
  status?: ToolCallStatus;
  outputHead?: string;
  outputTail?: string;
  outputLineCount?: number;
  exitCode?: number;
};

const LINE_COUNT = v.pipe(v.number(), v.safeInteger(), v.minValue(0));
const EXIT_CODE = v.pipe(v.number(), v.safeInteger());
const NAMED = v.pipe(v.string(), v.nonEmpty());

// Valibot's `record` reads an array as an object keyed "0", "1", …, so a JSON
// array is tried first and never reaches it.
const JSON_VALUE: v.GenericSchema<JsonValue> = v.lazy(() =>
  v.union([v.string(), v.pipe(v.number(), v.finite()), v.boolean(), v.null(), v.array(JSON_VALUE), v.record(v.string(), JSON_VALUE)]),
);

// An object of JSON values; an array, a scalar or `{}` names no arguments.
const ARGS = v.pipe(
  v.unknown(),
  v.check((value) => !Array.isArray(value)),
  v.record(v.string(), JSON_VALUE),
  v.check((args) => Object.keys(args).length > 0),
);

const OUTCOME = v.object({
  status: v.fallback(v.optional(TOOL_CALL_STATUS_SCHEMA), undefined),
  output_head: v.fallback(v.optional(v.string()), undefined),
  output_tail: v.fallback(v.optional(v.string()), undefined),
  output_line_count: v.fallback(v.optional(LINE_COUNT), undefined),
  exit_code: v.fallback(v.optional(EXIT_CODE), undefined),
});

const SAVED_CALL = v.object({
  ...OUTCOME.entries,
  call_id: NAMED,
  name: NAMED,
  arguments: v.optional(v.unknown()),
  duration_ms: v.pipe(v.number(), v.finite(), v.minValue(0)),
});

const SAVED_TRACE = v.object({
  calls: v.array(v.unknown()),
  omitted_call_count: v.fallback(LINE_COUNT, 0),
});

export type SavedTrace = { calls: FleetToolCall[]; omitted: number };

/** A frame's or a saved call's arguments, or undefined when there are none to show. */
export function readToolArgs(value: unknown): ToolArgs | undefined {
  const parsed = v.safeParse(ARGS, value);
  return parsed.success ? parsed.output : undefined;
}

/** The outcome fields a completion or a saved call carries, each one absent
 * when missing or malformed. */
export function readToolOutcome(value: unknown): ToolOutcome {
  const parsed = v.safeParse(OUTCOME, value);
  return parsed.success ? outcomeOf(parsed.output) : {};
}

function outcomeOf({ status, output_head, output_tail, output_line_count, exit_code }: v.InferOutput<typeof OUTCOME>): ToolOutcome {
  return {
    ...(status === undefined ? {} : { status }),
    ...(output_head === undefined ? {} : { outputHead: output_head }),
    ...(output_tail === undefined ? {} : { outputTail: output_tail }),
    ...(output_line_count === undefined ? {} : { outputLineCount: output_line_count }),
    ...(exit_code === undefined ? {} : { exitCode: exit_code }),
  };
}

/**
 * An event's saved trace as finished calls, or null when it carries none.
 * Only the difference between a call's start and its end renders, so every
 * call starts at `startedAtMs` — the event's own instant. A call too
 * malformed to render counts with the ones the runner left out, so the thread
 * still says something is missing.
 */
export function readSavedTrace(value: unknown, startedAtMs: number): SavedTrace | null {
  const trace = v.safeParse(SAVED_TRACE, value);
  if (!trace.success) return null;
  const calls: FleetToolCall[] = [];
  let omitted = trace.output.omitted_call_count;
  for (const raw of trace.output.calls) {
    const call = v.safeParse(SAVED_CALL, raw);
    if (!call.success) {
      omitted += 1;
      continue;
    }
    const args = readToolArgs(call.output.arguments);
    calls.push({
      name: call.output.name,
      callId: call.output.call_id,
      startedAtMs,
      ms: call.output.duration_ms,
      done: true,
      ...(args === undefined ? {} : { args }),
      ...outcomeOf(call.output),
    });
  }
  return { calls, omitted };
}

/** Every call still open, closed as interrupted: a turn that ended ended them.
 * The same event back when none was open. */
export function interruptOpenCalls(event: FleetEvent): FleetEvent {
  const tools = event.tools;
  if (tools === undefined || tools.every((call) => call.done)) return event;
  return {
    ...event,
    tools: tools.map((call) => (call.done ? call : { ...call, done: true, status: TOOL_CALL_STATUS.INTERRUPTED, closedAtSettle: true })),
  };
}

/** Whether the turn closed any call itself rather than hearing how it ended. */
export function closedAnyAtSettle(events: readonly FleetEvent[], eventId: string): boolean {
  return events.find((event) => event.id === eventId)?.tools?.some((call) => call.closedAtSettle === true) ?? false;
}

/**
 * The calls a settled row keeps. A saved trace replaces the live rows; with
 * none, the live rows stay and any still open are interrupted. A trace equal
 * to the one already shown keeps its array, so a page restating a settled turn
 * re-renders nothing.
 */
export function settleTools(row: FleetEvent, live: FleetEvent): FleetEvent {
  if (row.tools === undefined) {
    if (live.tools === undefined) return row;
    const omitted = live.omittedCallCount === undefined ? {} : { omittedCallCount: live.omittedCallCount };
    return interruptOpenCalls({ ...row, tools: live.tools, ...omitted });
  }
  return live.tools !== undefined && sameCalls(row.tools, live.tools) ? { ...row, tools: live.tools } : row;
}

// Both lists come out of `readSavedTrace`, field for field in one order, so
// their JSON text is equal exactly when the calls are. A live list never
// matches a saved one: its clocks are the browser's.
function sameCalls(a: readonly FleetToolCall[], b: readonly FleetToolCall[]): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}
