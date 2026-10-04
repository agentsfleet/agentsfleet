import { describe, expect, it } from "vitest";
import type { MessageState, ThreadMessageLike } from "@assistant-ui/react";

import { REASONING_SPAN, REPLY_FIGURE, readToolResult, replyParts, reportsOwnRun, toReplyMessage } from "./fleetReplyMessage";
import { readOmittedCallCount, readReasoningSpan, readReplyFigures } from "./fleetMessageReaders";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";
import { ACTOR } from "@/lib/events/event-summary";
import { AGENTSFLEET_EVENT_STATUS, type FleetEvent, type FleetEventStatus } from "@/lib/streaming/fleet-stream-row";
import { TOOL_CALL_STATUS, type ToolCallStatus } from "@/lib/streaming/fleet-stream-tool-trace";

const STARTED = 1_000;
const DONE_AFTER_MS = 900;
const { SUCCEEDED, FAILED, INTERRUPTED } = TOOL_CALL_STATUS;
const CALL_ID = "f1:0";
const ARGS = { query: "deploy window" } as const;
const OUTPUT = "3 matches";
const TOKENS = 12_400;
const WALL_MS = 41_000;
const OMITTED = 4;
const BASE: ThreadMessageLike = {
  role: "user",
  id: "e1:reply",
  content: [{ type: "text", text: "trigger text" }],
  metadata: { custom: { actor: "fleet", status: "received" } },
};

describe("toReplyMessage", () => {
  it("test_reply_parts_from_row", () => {
    const row = evt({
      id: "e1:reply",
      status: "received",
      reasoning: "Weighing it.",
      thinking: false,
      reply: "  Opened the PR.  ",
      reasoningStartedAtMs: STARTED,
      reasoningEndedAtMs: STARTED + DONE_AFTER_MS,
      tools: [
        {
          name: "search_repo", callId: CALL_ID, startedAtMs: STARTED, ms: DONE_AFTER_MS, done: true,
          args: ARGS, status: SUCCEEDED, outputHead: OUTPUT, outputLineCount: 1,
        },
        { name: "read_file", startedAtMs: STARTED + DONE_AFTER_MS, ms: null, done: false },
      ],
    });
    const message = toReplyMessage(BASE, row);
    expect(message.role).toBe("assistant");
    expect(message.status).toEqual({ type: "running" });
    expect(message.content).toEqual([
      { type: "reasoning", text: "Weighing it.", status: { type: "complete" } },
      {
        type: "tool-call", toolCallId: "e1:reply:tool:0", toolName: "search_repo", args: ARGS,
        result: { status: SUCCEEDED, outputHead: OUTPUT, outputLineCount: 1, callId: CALL_ID }, isError: false,
        timing: { startedAt: STARTED, completedAt: STARTED + DONE_AFTER_MS },
      },
      // A running call that named no arguments carries none, and no result.
      { type: "tool-call", toolCallId: "e1:reply:tool:1", toolName: "read_file", timing: { startedAt: STARTED + DONE_AFTER_MS } },
      { type: "text", text: "Opened the PR." },
    ]);
    // The row's custom bag rides through; the span joins it.
    expect(message.metadata?.custom).toMatchObject({
      actor: "fleet",
      [REASONING_SPAN.STARTED]: STARTED,
      [REASONING_SPAN.ENDED]: STARTED + DONE_AFTER_MS,
    });
    expect(toReplyMessage(BASE, { ...row, status: "processed" }).status).toEqual({ type: "complete", reason: "stop" });
  });

  it("should carry no completion instant when a done tool reported no wall time", () => {
    // Done without `ms`: the library reads a defined result as finished and a
    // missing completedAt as "duration unknown", so no clock claims a time.
    const [part] = replyParts(evt({ id: "e1:reply", tools: [{ name: "late", startedAtMs: STARTED, ms: null, done: true }] }));
    expect(part).toEqual({
      type: "tool-call", toolCallId: "e1:reply:tool:0", toolName: "late",
      result: {}, isError: false, timing: { startedAt: STARTED },
    });
  });

  it("test_failed_and_interrupted_calls_mark_error", () => {
    const done = (status: ToolCallStatus) => ({ name: "http_request", startedAtMs: STARTED, ms: DONE_AFTER_MS, done: true, status });
    const parts = replyParts(evt({ tools: [done(SUCCEEDED), done(FAILED), done(INTERRUPTED)] }));
    expect(parts.map((part) => (part.type === "tool-call" ? part.isError : undefined))).toEqual([false, true, true]);
  });

  it("test_malformed_tool_result_reads_absent", () => {
    // Running: no result to read.
    expect(readToolResult(undefined)).toBeUndefined();
    // Each field narrows on its own; a non-object reads as an empty outcome.
    expect(readToolResult({ status: "ok", outputHead: 7, outputLineCount: 2, exitCode: "1" })).toEqual({ outputLineCount: 2 });
    expect(readToolResult("x")).toEqual({});
    expect(readToolResult(null)).toEqual({});
  });

  it("test_reply_figures_ride_the_reply_bag", () => {
    const row = evt({ tokens: TOKENS, wallMs: WALL_MS, costNanos: null, omittedCallCount: OMITTED });
    const message = toReplyMessage(BASE, row);
    expect(message.metadata?.custom).toMatchObject({
      [REPLY_FIGURE.TOKENS]: TOKENS,
      [REPLY_FIGURE.WALL_MS]: WALL_MS,
      [REPLY_FIGURE.COST_NANOS]: null,
      [REPLY_FIGURE.OMITTED_CALLS]: OMITTED,
    });
    // An unreported figure reads null, never zero.
    expect(readReplyFigures(asState(message.metadata?.custom))).toEqual({ tokens: TOKENS, wallMs: WALL_MS, costNanos: null });
    expect(readOmittedCallCount(asState(message.metadata?.custom))).toBe(OMITTED);
  });

  it("test_malformed_reply_metadata_reads_absent", () => {
    // A row with nothing yet has no parts, not empty ones.
    expect(replyParts(evt({ reply: "   ", reasoning: "", tools: undefined }))).toEqual([]);
    // A live reasoning part runs on its own status.
    expect(replyParts(evt({ reasoning: "Still thinking", thinking: true }))[0]).toMatchObject({ status: { type: "running" } });
    const malformed = asState({ [REASONING_SPAN.STARTED]: "1000", [REASONING_SPAN.ENDED]: Number.NaN });
    expect(readReasoningSpan(malformed)).toEqual({ startedAtMs: null, endedAtMs: null });
    expect(readReasoningSpan(asState({}))).toEqual({ startedAtMs: null, endedAtMs: null });
    const wrongFigures = asState({ [REPLY_FIGURE.TOKENS]: "12", [REPLY_FIGURE.WALL_MS]: Number.POSITIVE_INFINITY, [REPLY_FIGURE.OMITTED_CALLS]: -3 });
    expect(readReplyFigures(wrongFigures)).toEqual({ tokens: null, wallMs: null, costNanos: null });
    expect(readOmittedCallCount(wrongFigures)).toBe(0);
    expect(readOmittedCallCount(asState({}))).toBe(0);
  });
});

// A message state carrying only the custom bag the readers look at.
function asState(custom: Record<string, unknown> | undefined): MessageState {
  return { metadata: { custom: custom ?? {} } } as unknown as MessageState;
}

const SUBJECT = "user_viewer";
const OWN = `${ACTOR.STEER_PREFIX}${SUBJECT}`;
const TEAMMATE = `${ACTOR.STEER_PREFIX}user_teammate`;
const SUBMITTED_AT_MS = 5_000;
const { OPTIMISTIC, RECEIVED, PROCESSED } = AGENTSFLEET_EVENT_STATUS;

// A turn this tab sent carries its submit clock; one that reached the tab by
// frame alone carries none.
function steer(id: string, actor: string, status: FleetEventStatus, submittedAtMs?: number): FleetEvent {
  return evt({ id, role: "user", actor, status, submittedAtMs });
}

describe("reportsOwnRun", () => {
  it("test_run_sent_from_this_tab_reports", () => {
    // Painted before the daemon names its sender.
    expect(reportsOwnRun([steer("optim-1", ACTOR.PENDING_STEER, OPTIMISTIC, SUBMITTED_AT_MS)], SUBJECT)).toBe(true);
    // Named by the 202: the submit clock rides the row.
    expect(reportsOwnRun([steer("evt_own", OWN, RECEIVED, SUBMITTED_AT_MS)], SUBJECT)).toBe(true);
  });

  it("test_run_sent_from_another_tab_stays_quiet", () => {
    // Same account, another tab: the top anchor must not pull this tab's reader to it.
    expect(reportsOwnRun([steer("evt_elsewhere", OWN, RECEIVED)], SUBJECT)).toBe(false);
  });

  it("test_unmarked_turn_stays_quiet_while_a_send_here_waits", () => {
    // Another tab's turn under the same account, newest while this tab's own
    // send awaits its 202: the waiting send never lends it this tab's run.
    const waiting = [steer("optim-1", ACTOR.PENDING_STEER, OPTIMISTIC, SUBMITTED_AT_MS), steer("evt_elsewhere", OWN, RECEIVED)];
    expect(reportsOwnRun(waiting, SUBJECT)).toBe(false);
  });

  it("test_newest_turn_decides", () => {
    // A teammate's turn landing under the viewer's own running reply ends the run.
    const under = [steer("evt_own", OWN, RECEIVED, SUBMITTED_AT_MS), steer("evt_mate", TEAMMATE, RECEIVED)];
    expect(reportsOwnRun(under, SUBJECT)).toBe(false);
  });

  it("test_settled_empty_or_signed_out_stays_quiet", () => {
    expect(reportsOwnRun([], SUBJECT)).toBe(false);
    expect(reportsOwnRun([steer("evt_own", OWN, PROCESSED, SUBMITTED_AT_MS)], SUBJECT)).toBe(false);
    // With no subject, no named steer is the viewer's.
    expect(reportsOwnRun([steer("evt_own", OWN, RECEIVED, SUBMITTED_AT_MS)], null)).toBe(false);
  });
});
