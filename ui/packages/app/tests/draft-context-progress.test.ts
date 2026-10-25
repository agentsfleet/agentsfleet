// @vitest-environment node
import { expect, it, vi } from "vitest";
import { DraftLive } from "@/lib/api/draft-live";
import { CONTEXT_STATE } from "@/lib/context-progress";
import { DRAFT_ID, MESSAGE, OPERATION_ID } from "./helpers/draft-fixtures";

const running = [{...MESSAGE, state: "running" as const}];
const start = {generation: 1, state: CONTEXT_STATE.summarizing};
const end = {generation: 1, state: CONTEXT_STATE.resuming};
const stream = "0199a801-1000-7000-8000-000000000011";
const retry = "0199a801-1000-7000-8000-000000000012";
function frame(progress: unknown = start, extra: Record<string, unknown> = {}): string {
  return JSON.stringify({draft_id: DRAFT_ID, sequence: "1", stream_id: stream, progress, ...extra});
}

it("accepts ordered generation metadata while withholding duplicate and late starts", () => {
  const changed = vi.fn(); const live = new DraftLive(changed);
  expect(live.context(frame(), DRAFT_ID, "0", running)).toBe(true);
  expect(changed.mock.lastCall).toEqual([{contextProgress: {sequence: "1", progress: start}}]);
  expect(live.context(frame(), DRAFT_ID, "0", running)).toBe(false);
  expect(live.context(frame(end), DRAFT_ID, "0", running)).toBe(true);
  expect(live.context(frame(), DRAFT_ID, "0", running)).toBe(false);
  expect(live.context(frame(start, {stream_id: retry}), DRAFT_ID, "0", running)).toBe(false);
  expect(live.context(frame({...start, generation: 2}, {stream_id: retry}), DRAFT_ID, "0", running)).toBe(true);
  expect(changed.mock.lastCall).toEqual([{contextProgress: {sequence: "1", progress: {...start, generation: 2}}}]);
});

it("permits a fresh physical attempt after an interrupted summary without accepting its old start", () => {
  const live = new DraftLive(() => {});
  expect(live.context(frame({...start, state: CONTEXT_STATE.interrupted}), DRAFT_ID, "0", running)).toBe(true);
  expect(live.context(frame(), DRAFT_ID, "0", running)).toBe(false);
  expect(live.context(frame(start, {stream_id: retry}), DRAFT_ID, "0", running)).toBe(true);
  expect(live.context(frame({...end, generation: 2}), DRAFT_ID, "0", running)).toBe(true);
  expect(live.context(frame(start, {stream_id: retry}), DRAFT_ID, "0", running)).toBe(false);
});

it.each([
  "not-json", "x".repeat(2_049), frame(start, {draft_id: OPERATION_ID}), frame(start, {sequence: "0"}),
  frame(start, {sequence: "2"}), frame(start, {stream_id: "wrong"}), frame(start, {secret_notes: "private"}),
  frame({...start, generation: 0}), frame({...start, generation: Number.MAX_SAFE_INTEGER + 1}),
  frame({...start, state: "done"}), frame({...start, notes: "private"}),
])("rejects an invalid or foreign notice without changing visible state (%#)", data => {
  const changed = vi.fn(); const live = new DraftLive(changed);
  expect(live.context(data, DRAFT_ID, "0", running)).toBe(false);
  expect(changed).not.toHaveBeenCalled();
});

it("requires a known running user row and rejects stopped, completed or older sequences", () => {
  const changed = vi.fn(); const live = new DraftLive(changed);
  for (const messages of [[], [MESSAGE], [{...running[0]!, role: "assistant" as const}]]) {
    expect(live.context(frame(), DRAFT_ID, "0", messages)).toBe(false);
  }
  expect(live.context(frame(), DRAFT_ID, "1", running)).toBe(false);
  expect(changed).not.toHaveBeenCalled();
});

it("hides provisional status on reconnect, settles it from authoritative history and resets ownership", () => {
  const changed = vi.fn(); const live = new DraftLive(changed);
  live.context(frame(), DRAFT_ID, "0", running);
  live.gap();
  expect(changed.mock.lastCall).toEqual([{contextProgress: null}]);
  expect(live.context(frame(), DRAFT_ID, "0", running)).toBe(false);
  expect(live.context(frame(end), DRAFT_ID, "0", running)).toBe(true);
  live.reconcile([MESSAGE], "0");
  expect(changed.mock.lastCall).toEqual([{contextProgress: null}]);
  live.clear();
  expect(live.context(frame(), DRAFT_ID, "0", running)).toBe(true);
});

it("accepts a new message generation independently and withholds stale previous-message notices", () => {
  const changed = vi.fn(); const live = new DraftLive(changed);
  live.context(frame({...end, generation: 20}), DRAFT_ID, "0", running);
  const messages = [...running, {...MESSAGE, sequence: "2", state: "running" as const}];
  expect(live.context(frame(start, {sequence: "2"}), DRAFT_ID, "0", messages)).toBe(true);
  expect(live.context(frame({...start, generation: 21}), DRAFT_ID, "0", messages)).toBe(false);
  expect(changed.mock.lastCall).toEqual([{contextProgress: {sequence: "2", progress: start}}]);
});

it("bounds dispatch tracking per message without discarding the confirmed visible ending", () => {
  const changed = vi.fn(); const live = new DraftLive(changed);
  for (let index = 0; index < 32; index++) {
    const stream_id = `0199a801-1000-7000-8000-${String(index).padStart(12, "0")}`;
    expect(live.context(frame(start, {stream_id}), DRAFT_ID, "0", running)).toBe(true);
  }
  const stream_id = "0199a801-1000-7000-8000-000000000099";
  expect(live.context(frame(start, {stream_id}), DRAFT_ID, "0", running)).toBe(false);
  expect(changed).toHaveBeenCalledTimes(32);
});
