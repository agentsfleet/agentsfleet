import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { ContextProgressStatus } from "@/components/domain/ContextProgressStatus";
import { CONTEXT_STATE } from "@/lib/context-progress";
import { FRAME_KIND } from "@/lib/api/events-types";
import { applyLiveFrame, parseLiveFrame } from "@/lib/streaming/fleet-stream-frames";
import { hideContextProgress } from "@/lib/streaming/fleet-stream-context";
import { applyReplyDelta, applyReplyRecovery } from "@/lib/streaming/fleet-stream-reply-frames";
import { createDraftThreadProjection, draftThreadMessages } from "@/lib/api/draft-thread-messages";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { DRAFT, MESSAGE } from "./helpers/draft-fixtures";
import { evt } from "./helpers/fleet-stream-fixtures";

afterEach(cleanup);
const start = {generation: 1, state: CONTEXT_STATE.summarizing};
const end = {generation: 1, state: CONTEXT_STATE.resuming};
function status(progress: unknown = start, event = "e0") {
  const parsed = parseLiveFrame(JSON.stringify({kind: FRAME_KIND.CONTEXT_PROGRESS, event_id: event, ...Object(progress)}));
  if (!parsed) throw new Error("Expected JSON status frame");
  return parsed;
}

it.each([
  [CONTEXT_STATE.summarizing, "Context summarizing…"],
  [CONTEXT_STATE.resuming, "Context summarized, resuming…"],
  [CONTEXT_STATE.interrupted, "Context summarization interrupted."],
])("announces %s through an accessible live status", (state, label) => {
  render(<ContextProgressStatus value={{...start, state}} />);
  expect(screen.getByRole("status").textContent).toContain(label);
  expect(screen.getByRole("status").getAttribute("aria-live")).toBe("polite");
});

it.each([null, {}, {...start, notes: "private summary"}, {...start, generation: 0}, {...start, state: "complete"}])(
  "withholds malformed or content-bearing status metadata (%#)", value => {
    const {container} = render(<ContextProgressStatus value={value} />);
    expect(container.textContent).toBe("");
  },
);

it("updates only the running fleet row with monotonic progress and ignores duplicate/older notices", () => {
  const before = [evt(), evt({id: "other"})];
  const started = applyLiveFrame(before, status());
  expect(started[0]?.contextProgress).toEqual(start);
  expect(started[1]).toBe(before[1]);
  expect(applyLiveFrame(started, status())).toBe(started);
  const completed = applyLiveFrame(started, status(end));
  expect(completed[0]?.contextProgress).toEqual(end);
  expect(applyLiveFrame(completed, status())).toBe(completed);
  const again = applyLiveFrame(completed, status({...start, generation: 2}));
  expect(again[0]?.contextProgress?.generation).toBe(2);
  expect(applyLiveFrame(again, status(end))).toBe(again);
});

it("withholds unknown, terminal and malformed fleet progress without creating history", () => {
  const before = [evt(), evt({id: "done", status: "processed"})];
  for (const frame of [status(start, "missing"), status(start, "done"), status({...start, generation: 0}), status({...start, state: "bad"})]) {
    expect(applyLiveFrame(before, frame)).toBe(before);
  }
  expect(applyLiveFrame([], status())).toEqual([]);
});

it("hides status on reply/gap/recovery while retaining its duplicate ordering frontier", () => {
  const started = applyLiveFrame([evt()], status(end));
  const hidden = hideContextProgress(started);
  expect(hidden[0]?.contextProgressHidden).toBe(true);
  expect(hideContextProgress(hidden)).toBe(hidden);
  expect(applyLiveFrame(hidden, status())).toBe(hidden);
  const next = applyLiveFrame(hidden, status({...start, generation: 2}));
  expect(next[0]?.contextProgressHidden).toBeUndefined();
  const reply = applyReplyDelta(next, "e0", {answer: "Visible answer", reasoning: "", thinking: false});
  expect(reply[0]).toMatchObject({reply: "Visible answer", contextProgressHidden: true});
  expect(applyReplyRecovery(reply, "e0", false)[0]?.contextProgress).toBeUndefined();
});

it("projects setup status only onto the matching running answer and preserves settled message identities", () => {
  const project = createDraftThreadProjection();
  const messages = [MESSAGE, {...MESSAGE, sequence: "2", role: "assistant" as const, state: "running" as const}];
  const view = {...EMPTY_DRAFT_VIEW, draft: DRAFT, messages};
  const before = project(view);
  const contextProgress = {sequence: "2", progress: start};
  const after = project({...view, contextProgress});
  expect(after[0]).toBe(before[0]);
  expect(after[1]).not.toBe(before[1]);
  expect(after[1]?.metadata?.custom?.contextProgress).toEqual(start);
  expect(draftThreadMessages({...view, contextProgress, draft: {...DRAFT, cancelled_through: "2"}})[1]?.metadata?.custom?.contextProgress).toBeUndefined();
  expect(draftThreadMessages({...view, contextProgress, messages: [{...messages[1]!, state: "completed"} ]})[0]?.metadata?.custom?.contextProgress).toBeUndefined();
});
