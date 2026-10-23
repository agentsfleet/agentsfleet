import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ReplyStream } from "./reply-stream";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());
const first = {sequence: 0, start: true, contiguous: true, kind: "answer", text: "Hello"};

it("yields first text immediately and batches subsequent text at the shared cadence", async () => {
  const changed = vi.fn(); const stream = new ReplyStream(changed);
  expect(stream.write(first)).toBe(true);
  expect(changed).toHaveBeenCalledWith({answer: "Hello", reasoning: "", thinking: false});
  stream.write({...first, sequence: 1, start: false, text: " 🌿"});
  expect(changed).toHaveBeenCalledOnce();
  await vi.advanceTimersByTimeAsync(50);
  expect(changed).toHaveBeenLastCalledWith({answer: " 🌿", reasoning: "", thinking: false});
  await stream.finish();
  expect(stream.write({...first, sequence: 2, start: false})).toBe(false);
  expect(vi.getTimerCount()).toBe(0);
});

it.each([
  {sequence: undefined}, {sequence: -1}, {sequence: 0.5}, {sequence: Number.MAX_SAFE_INTEGER + 1},
  {start: false}, {contiguous: false}, {kind: "unknown"},
])("requires an ordered, typed first frame (%j)", change => {
  const changed = vi.fn(); const stream = new ReplyStream(changed);
  expect(stream.write({...first, ...change})).toBe(false);
  expect(stream.needsFinalReply).toBe(true);
  expect(stream.write(first)).toBe(false);
  expect(changed).not.toHaveBeenCalled();
  stream.dispose();
});

it("drops buffered text after a gap and lets only the durable reader repair it", async () => {
  const changed = vi.fn(); const stream = new ReplyStream(changed);
  stream.write(first); stream.write({...first, sequence: 1, start: false, text: " pending"});
  expect(stream.write({...first, sequence: 3, start: false})).toBe(false);
  await vi.advanceTimersByTimeAsync(100);
  await stream.finish();
  expect(changed).toHaveBeenCalledOnce();
  expect(stream.needsFinalReply).toBe(true);
});

it("keeps reasoning typed and cancels pending renders on disposal", async () => {
  const changed = vi.fn(); const stream = new ReplyStream(changed);
  stream.write({...first, kind: "reasoning", text: "Thinking"});
  stream.write({...first, sequence: 1, start: false}); stream.dispose();
  await vi.advanceTimersByTimeAsync(100);
  expect(changed).toHaveBeenCalledOnce();
  expect(changed).toHaveBeenLastCalledWith({answer: "", reasoning: "Thinking", thinking: true});
});
