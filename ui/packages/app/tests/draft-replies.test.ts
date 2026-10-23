import { afterEach, beforeEach, expect, it, vi, type Mock } from "vitest";
import { DraftReplies, type DraftReply } from "@/lib/api/draft-replies";
import { DRAFT, MESSAGE, FLEET_ID } from "./helpers/draft-fixtures";

const STREAM = "0199a801-1000-7000-8000-000000000011";
const NEXT = "0199a801-1000-7000-8000-000000000012";
const PREVIEW_LIMIT_BYTES = 131_072;
const frame = (change = {}) => JSON.stringify({draft_id: DRAFT.id, sequence: "1", stream_id: STREAM, stream_seq: 1, stream_start: true, text_kind: "answer", text: "First 🌿", ...change});
let changed: Mock<(reply: DraftReply | null) => void>;
let replies: DraftReplies;
beforeEach(() => {vi.useFakeTimers(); changed = vi.fn(); replies = new DraftReplies(changed);});
afterEach(() => {replies.clear(); vi.useRealTimers();});

it("renders before completion, then lets a durable final clear every provisional callback", async () => {
  const escaped = "\u0000".repeat(4096);
  expect(replies.accept(frame({text: escaped}), DRAFT.id, "0", [])).toBe(true);
  expect(changed).toHaveBeenLastCalledWith(expect.objectContaining({text: escaped, gapped: false}));
  replies.accept(frame({stream_seq: 2, stream_start: false, text: " buffered"}), DRAFT.id, "0", []);
  const final = {...MESSAGE, role: "assistant" as const, state: "completed" as const};
  replies.reconcile([final], "0");
  expect(changed).toHaveBeenLastCalledWith(null);
  await vi.advanceTimersByTimeAsync(50);
  expect(changed).toHaveBeenLastCalledWith(null);
  expect(replies.accept(frame(), DRAFT.id, "0", [final])).toBe(false);
});

it("ignores an old pass after a new pass begins without discarding the current pass", async () => {
  replies.accept(frame(), DRAFT.id, "0", []);
  replies.accept(frame({stream_id: NEXT, text: "Second"}), DRAFT.id, "0", []);
  expect(replies.accept(frame({stream_seq: 2, stream_start: false, text: "stale"}), DRAFT.id, "0", [])).toBe(false);
  replies.accept(frame({stream_id: NEXT, stream_seq: 2, stream_start: false, text: " answer"}), DRAFT.id, "0", []);
  await vi.advanceTimersByTimeAsync(50);
  expect(changed).toHaveBeenLastCalledWith(expect.objectContaining({stream: NEXT, text: "Second answer", gapped: false}));
});

it("requires durable recovery after a missing frame and ignores older messages", () => {
  replies.accept(frame({sequence: "2"}), DRAFT.id, "0", []);
  replies.accept(frame({sequence: "2", stream_seq: 3, stream_start: false}), DRAFT.id, "0", []);
  expect(changed).toHaveBeenLastCalledWith(expect.objectContaining({sequence: "2", text: "First 🌿", gapped: true}));
  expect(replies.accept(frame(), DRAFT.id, "0", [])).toBe(false);
  replies.reconcile([], "2"); expect(changed).toHaveBeenLastCalledWith(null);
});

it.each(["not JSON", frame({draft_id: FLEET_ID}), frame({text_kind: "reasoning"}), frame({text: "🌿".repeat(1025)}), frame({sequence: "0"}), "x".repeat(32769)])("rejects private or oversized input without publishing (%#)", data => {
  expect(replies.accept(data, DRAFT.id, "0", [])).toBe(false);
  expect(changed).not.toHaveBeenCalled();
});

it("caps assembled previews and falls back to the durable final", async () => {
  for (let index = 1; index <= 33; index += 1) {
    replies.accept(frame({stream_seq: index, stream_start: index === 1, text: "x".repeat(4096)}), DRAFT.id, "0", []);
    await vi.advanceTimersByTimeAsync(50);
  }
  expect(changed).toHaveBeenLastCalledWith(expect.objectContaining({text: "x".repeat(PREVIEW_LIMIT_BYTES), gapped: true}));
});

it("ignores gaps without a preview and retains running text until its own final or cutoff", () => {
  replies.gap(); replies.reconcile([], "0");
  expect(changed).not.toHaveBeenCalled();
  replies.accept(frame(), DRAFT.id, "0", []);
  replies.reconcile([{...MESSAGE, role: "user"}, {...MESSAGE, role: "assistant", state: "running"}], "0");
  expect(changed).toHaveBeenLastCalledWith(expect.objectContaining({text: "First 🌿"}));
  expect(replies.accept(frame(), DRAFT.id, "1", [])).toBe(false);
});

it("bounds distinct provider passes for one message and accepts a subsequent message", () => {
  const passes = 32;
  for (let index = 0; index < passes; index += 1) {
    const stream_id = `0199a801-1000-7000-8000-${index.toString().padStart(12, "0")}`;
    expect(replies.accept(frame({stream_id}), DRAFT.id, "0", [])).toBe(true);
  }
  const overflow = "0199a801-1000-7000-8000-999999999999";
  expect(replies.accept(frame({stream_id: overflow}), DRAFT.id, "0", [])).toBe(false);
  expect(changed).toHaveBeenLastCalledWith(expect.objectContaining({gapped: true}));
  expect(replies.accept(frame({sequence: "2", stream_id: overflow}), DRAFT.id, "0", [])).toBe(true);
  expect(changed).toHaveBeenLastCalledWith(expect.objectContaining({sequence: "2", gapped: false}));
});

it("rejects oversized encoded frames before parsing unrelated Unicode metadata", () => {
  expect(replies.accept(frame({padding: "🌿".repeat(8192)}), DRAFT.id, "0", [])).toBe(false);
  expect(changed).not.toHaveBeenCalled();
});
