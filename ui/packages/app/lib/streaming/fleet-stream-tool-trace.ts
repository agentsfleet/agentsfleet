import { z } from "zod";

import type { FleetEvent, FleetToolCall } from "./fleet-stream-row";

// A tool call's arguments and outcome, narrowed where they cross into the
// browser: off a live frame, and off an event's saved trace. Each field
// narrows on its own, so a malformed one reads as absent and the call still
// renders without it — the frame's name and timing are what make a call, and
// `fleet-stream-tool-frames` reads those. Also where a settled turn's calls
// are decided: the saved trace when it has one, else the live rows with any
// still open closed.

/** How a call ended, as `afd_wire::tool_trace::ToolCallStatus` spells it. */
export const TOOL_CALL_STATUS = {
  SUCCEEDED: "succeeded",
  FAILED: "failed",
  INTERRUPTED: "interrupted",
} as const;

export type ToolCallStatus = (typeof TOOL_CALL_STATUS)[keyof typeof TOOL_CALL_STATUS];

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

const STATUS = z.enum([TOOL_CALL_STATUS.SUCCEEDED, TOOL_CALL_STATUS.FAILED, TOOL_CALL_STATUS.INTERRUPTED]);
const LINE_COUNT = z.number().int().nonnegative();
const EXIT_CODE = z.number().int();
// An object of JSON values; an array, a scalar or `{}` names no arguments.
const ARGS = z.record(z.string(), z.json()).refine((args) => Object.keys(args).length > 0);

const OUTCOME = z.object({
  status: STATUS.optional().catch(undefined),
  output_head: z.string().optional().catch(undefined),
  output_tail: z.string().optional().catch(undefined),
  output_line_count: LINE_COUNT.optional().catch(undefined),
  exit_code: EXIT_CODE.optional().catch(undefined),
});

const SAVED_CALL = OUTCOME.extend({
  call_id: z.string().min(1),
  name: z.string().min(1),
  arguments: z.unknown(),
  duration_ms: z.number().nonnegative(),
});

const SAVED_TRACE = z.object({
  calls: z.array(z.unknown()),
  omitted_call_count: LINE_COUNT.catch(0),
});

export type SavedTrace = { calls: FleetToolCall[]; omitted: number };

/** A frame's or a saved call's arguments, or undefined when there are none to show. */
export function readToolArgs(value: unknown): ToolArgs | undefined {
  const parsed = ARGS.safeParse(value);
  return parsed.success ? parsed.data : undefined;
}

/** The outcome fields a completion or a saved call carries, each one absent
 * when missing or malformed. */
export function readToolOutcome(value: unknown): ToolOutcome {
  return outcomeOf(OUTCOME.catch({}).parse(value));
}

function outcomeOf({ status, output_head, output_tail, output_line_count, exit_code }: z.infer<typeof OUTCOME>): ToolOutcome {
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
  const trace = SAVED_TRACE.safeParse(value);
  if (!trace.success) return null;
  const calls: FleetToolCall[] = [];
  let omitted = trace.data.omitted_call_count;
  for (const raw of trace.data.calls) {
    const call = SAVED_CALL.safeParse(raw);
    if (!call.success) {
      omitted += 1;
      continue;
    }
    const args = readToolArgs(call.data.arguments);
    calls.push({
      name: call.data.name,
      callId: call.data.call_id,
      startedAtMs,
      ms: call.data.duration_ms,
      done: true,
      ...(args === undefined ? {} : { args }),
      ...outcomeOf(call.data),
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
    tools: tools.map((call) => (call.done ? call : { ...call, done: true, status: TOOL_CALL_STATUS.INTERRUPTED })),
  };
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
