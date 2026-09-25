import { afterEach, describe, expect, it, vi } from "vitest";
import { MAX_PENDING_CHARS, ReplyStreamDecoder, type ReplyDelta } from "./reply-stream-decoder";

const BURST_WRITES = 1_000;

function collect(): { decoder: ReplyStreamDecoder; deltas: ReplyDelta[] } {
  const deltas: ReplyDelta[] = [];
  return { decoder: new ReplyStreamDecoder((delta) => deltas.push(delta)), deltas };
}

describe("ReplyStreamDecoder", () => {
  afterEach(() => vi.useRealTimers());

  it("shows the first typed text immediately and preserves kind transitions", async () => {
    vi.useFakeTimers();
    const { decoder, deltas } = collect();
    decoder.write("Use `<div>` and Vec<String>.", "answer");
    expect(deltas[0]).toEqual({ answer: "Use `<div>` and Vec<String>.", reasoning: "", thinking: false });
    decoder.write("Consider a<b)", "reasoning");
    expect(deltas).toHaveLength(1);
    decoder.write("Done.", "answer");
    expect(deltas[1]).toEqual({ answer: "", reasoning: "Consider a<b)", thinking: true });
    await decoder.finish();
    expect(deltas[2]).toEqual({ answer: "Done.", reasoning: "", thinking: false });
    await decoder.finish();
    vi.runAllTimers();
    expect(deltas).toHaveLength(3);
    expect(decoder.needsFinalReply).toBe(false);
  });

  it("stops accepting bytes after a transport gap or completion", async () => {
    vi.useFakeTimers();
    const { decoder, deltas } = collect();
    decoder.write("", "answer");
    expect(deltas).toEqual([]);
    decoder.write("Safe", "answer");
    decoder.write("pending", "answer");
    decoder.markGap();
    decoder.write("PRIVATE", "answer");
    await decoder.finish();
    vi.runAllTimers();
    decoder.write("late", "reasoning");
    expect(decoder.needsFinalReply).toBe(true);
    expect(deltas.map((delta) => delta.answer)).toEqual(["Safe"]);
  });

  it("drops a disposed stream", async () => {
    vi.useFakeTimers();
    const { decoder, deltas } = collect();
    decoder.write("visible", "answer");
    decoder.write("PRIVATE", "answer");
    decoder.dispose();
    decoder.write("PRIVATE", "answer");
    await decoder.finish();
    vi.runAllTimers();
    expect(deltas.map((delta) => delta.answer)).toEqual(["visible"]);
  });

  it("coalesces a burst by time without copying each growing answer", async () => {
    vi.useFakeTimers();
    const { decoder, deltas } = collect();
    decoder.write("first", "answer");
    for (let at = 0; at < BURST_WRITES; at++) decoder.write("x", "answer");
    expect(deltas).toHaveLength(1);
    vi.advanceTimersByTime(49);
    expect(deltas).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(deltas).toHaveLength(2);
    expect(deltas[1]?.answer).toBe("x".repeat(BURST_WRITES));
    await decoder.finish();
  });

  it("flushes a bounded pending burst before it can grow without limit", async () => {
    vi.useFakeTimers();
    const { decoder, deltas } = collect();
    decoder.write("first", "answer");
    decoder.write("x".repeat(MAX_PENDING_CHARS), "answer");
    expect(deltas).toHaveLength(2);
    vi.runAllTimers();
    expect(deltas).toHaveLength(2);
    await decoder.finish();
  });
});
