import { describe, expect, it } from "vitest";
import type { MessageState, ThreadMessageLike } from "@assistant-ui/react";

import { REASONING_SPAN, replyParts, toReplyMessage } from "./fleetReplyMessage";
import { readReasoningSpan } from "./fleetMessageReaders";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";

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
