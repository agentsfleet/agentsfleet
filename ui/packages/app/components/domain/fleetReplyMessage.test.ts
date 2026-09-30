import { describe, expect, it } from "vitest";
import type { MessageState, ThreadMessageLike } from "@assistant-ui/react";

import { REASONING_SPAN, replyParts, reportsOwnRun, toReplyMessage } from "./fleetReplyMessage";
import { readReasoningSpan } from "./fleetMessageReaders";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";
import { ACTOR } from "@/lib/events/event-summary";
import { AGENTSFLEET_EVENT_STATUS, type FleetEvent, type FleetEventStatus } from "@/lib/streaming/fleet-stream-row";

const STARTED = 1_000;
const DONE_AFTER_MS = 900;
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
        { name: "search_repo", startedAtMs: STARTED, ms: DONE_AFTER_MS, done: true },
        { name: "read_file", startedAtMs: STARTED + DONE_AFTER_MS, ms: null, done: false },
      ],
    });
    const message = toReplyMessage(BASE, row);
    expect(message.role).toBe("assistant");
    expect(message.status).toEqual({ type: "running" });
    expect(message.content).toEqual([
      { type: "reasoning", text: "Weighing it.", status: { type: "complete" } },
      {
        type: "tool-call", toolCallId: "e1:reply:tool:0", toolName: "search_repo", args: {},
        result: null, timing: { startedAt: STARTED, completedAt: STARTED + DONE_AFTER_MS },
      },
      {
        type: "tool-call", toolCallId: "e1:reply:tool:1", toolName: "read_file", args: {},
        timing: { startedAt: STARTED + DONE_AFTER_MS },
      },
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
      type: "tool-call", toolCallId: "e1:reply:tool:0", toolName: "late", args: {},
      result: null, timing: { startedAt: STARTED },
    });
  });

  it("test_malformed_reply_metadata_reads_absent", () => {
    // A row with nothing yet has no parts, not empty ones.
    expect(replyParts(evt({ reply: "   ", reasoning: "", tools: undefined }))).toEqual([]);
    // A live reasoning part runs on its own status.
    expect(replyParts(evt({ reasoning: "Still thinking", thinking: true }))[0]).toMatchObject({ status: { type: "running" } });
    const malformed = { metadata: { custom: { [REASONING_SPAN.STARTED]: "1000", [REASONING_SPAN.ENDED]: Number.NaN } } };
    expect(readReasoningSpan(malformed as unknown as MessageState)).toEqual({ startedAtMs: null, endedAtMs: null });
    const missing = { metadata: { custom: {} } };
    expect(readReasoningSpan(missing as unknown as MessageState)).toEqual({ startedAtMs: null, endedAtMs: null });
  });
});

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
