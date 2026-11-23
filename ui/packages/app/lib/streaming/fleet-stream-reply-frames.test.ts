import { describe, expect, it } from "vitest";
import { FRAME_KIND } from "@/lib/api/turns-types";
import { applyLiveFrame } from "./fleet-stream-frames";
import { OUTCOME } from "@/lib/turns/turn-summary";
import { applyFinalReplyText, applyReplyDelta, applyReplyGone, applyReplyRecovery } from "./fleet-stream-reply-frames";
import { AGENTSFLEET_TURN_STATUS } from "./fleet-stream-row";
import type { FleetTurn } from "./fleet-stream-row";
import type { ReplyDelta } from "./reply-stream-decoder";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";

// The reasoning span: when a reply started thinking, and when the answer,
// the completion or a recovery ended it. The row carries it, so a chat that
// remounts mid-thought reads the same start instead of restarting a clock.

const TURN = "e1";
const REASONING_AT = 1_000;
const MORE_REASONING_AT = 1_500;
const ANSWER_AT = 9_500;
const LATER_AT = 9_700;

function reasoning(text: string): ReplyDelta {
  return { answer: "", reasoning: text, thinking: true };
}

function answer(text: string): ReplyDelta {
  return { answer: text, reasoning: "", thinking: false };
}

function only(turns: FleetTurn[]): FleetTurn {
  const [turn] = turns;
  if (turn === undefined) throw new Error("expected one event");
  return turn;
}

describe("applyReplyDelta — reasoning span", () => {
  it("test_reasoning_span_stamped_once", () => {
    let rows = applyReplyDelta([evt({ id: TURN })], TURN, reasoning("Checking. "), REASONING_AT);
    rows = applyReplyDelta(rows, TURN, reasoning("Still checking."), MORE_REASONING_AT);
    expect(only(rows)).toMatchObject({ reasoningStartedAtMs: REASONING_AT });
    expect(only(rows).reasoningEndedAtMs).toBeUndefined();

    rows = applyReplyDelta(rows, TURN, answer("Signed."), ANSWER_AT);
    rows = applyReplyDelta(rows, TURN, answer(" Done."), LATER_AT);
    expect(only(rows)).toMatchObject({ reasoningStartedAtMs: REASONING_AT, reasoningEndedAtMs: ANSWER_AT });
  });

  // A short prompt: reasoning, answer, reasoning again, answer. The span
  // reopens, so the folded Thought counts to the last hand-off, and a run that
  // completes mid-thought closes it there.
  it("test_resumed_reasoning_reopens_the_span", () => {
    let rows = applyReplyDelta([evt({ id: TURN })], TURN, reasoning("Greeting. "), REASONING_AT);
    rows = applyReplyDelta(rows, TURN, answer("Hey!"), ANSWER_AT);
    expect(only(rows).reasoningEndedAtMs).toBe(ANSWER_AT);
    rows = applyReplyDelta(rows, TURN, reasoning("Anything else?"), ANSWER_AT + 1);
    expect(only(rows)).toMatchObject({ reasoningStartedAtMs: REASONING_AT, thinking: true });
    expect(only(rows).reasoningEndedAtMs).toBeUndefined();
    const answered = applyReplyDelta(rows, TURN, answer(" How can I help?"), LATER_AT);
    expect(only(answered)).toMatchObject({ reasoningStartedAtMs: REASONING_AT, reasoningEndedAtMs: LATER_AT });

    const completed = applyLiveFrame(rows, { kind: FRAME_KIND.TURN_COMPLETE, turn_id: TURN, status: "processed" }, LATER_AT);
    expect(only(completed).reasoningEndedAtMs).toBe(LATER_AT);
  });

  it("an answer-only reply opens no span", () => {
    const rows = applyReplyDelta([evt({ id: TURN })], TURN, answer("Straight answer."), ANSWER_AT);
    expect(only(rows).reasoningStartedAtMs).toBeUndefined();
    expect(only(rows).reasoningEndedAtMs).toBeUndefined();
  });

  it("stamps a reply the timeline opens from its first reasoning chunk", () => {
    const rows = applyReplyDelta([], TURN, reasoning("Opening thought."), REASONING_AT);
    expect(only(rows)).toMatchObject({ id: TURN, reasoningStartedAtMs: REASONING_AT, thinking: true });
  });

  it("the completion ends a span the answer never did", () => {
    const thinking = applyReplyDelta([evt({ id: TURN })], TURN, reasoning("Only thought."), REASONING_AT);
    const completed = applyLiveFrame(thinking, {
      kind: FRAME_KIND.TURN_COMPLETE, turn_id: TURN, status: "processed",
    }, ANSWER_AT);
    expect(only(completed)).toMatchObject({ reasoningEndedAtMs: ANSWER_AT, thinking: false });
    const saved = applyFinalReplyText(thinking, TURN, "Saved.", LATER_AT);
    expect(only(saved).reasoningEndedAtMs).toBe(LATER_AT);
  });

  it("a recovery ends the span; a row that never reasoned stays unstamped", () => {
    const thinking = applyReplyDelta([evt({ id: TURN })], TURN, reasoning("Lost mid-thought."), REASONING_AT);
    expect(only(applyReplyRecovery(thinking, TURN, false, ANSWER_AT)).reasoningEndedAtMs).toBe(ANSWER_AT);
    const plain = applyFinalReplyText([evt({ id: TURN })], TURN, "Saved.", ANSWER_AT);
    expect(only(plain).reasoningEndedAtMs).toBeUndefined();
  });
});

describe("applyReplyGone", () => {
  it("settles only its own row, with the gone line and its span closed", () => {
    const other = evt({ id: "e_other", reply: "Kept." });
    let rows = applyReplyDelta([evt({ id: TURN, status: AGENTSFLEET_TURN_STATUS.RECEIVED }), other], TURN, reasoning("Checking. "), REASONING_AT);
    rows = applyReplyGone(rows, TURN, LATER_AT);
    expect(rows[0]).toMatchObject({
      status: AGENTSFLEET_TURN_STATUS.AGENT_ERROR,
      reply: "",
      thinking: false,
      replyRecovering: false,
      outcome: OUTCOME.REPLY_GONE,
      reasoningEndedAtMs: LATER_AT,
    });
    expect(rows[1]).toBe(other);
  });
});
