import type { ActivityLiveFrame, LiveFrame } from "@/lib/api/events";
import type { LiveEntry } from "./fleet-stream-dispatch";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FRAME_KIND } from "@/lib/api/events-types";
import { createEntry } from "./fleet-stream-entry";
import { acceptAttemptFrame } from "./fleet-stream-attempt";
import { mergeBackfill } from "./fleet-stream-frames";
import { onFrame } from "./fleet-stream-dispatch";
import { recoverFinalReply } from "./fleet-stream-detail-recovery";
import { setTurnDetailReader, disposeReplyStreams, settleRepliesFromBackfill } from "./fleet-stream-reply-registry";
import { row, WS, Z_A } from "@/tests/helpers/fleet-stream-registry-fixtures";

const EVENT = "evt_retry";
const entries: LiveEntry[] = [];

function live(): LiveEntry {
  const entry: LiveEntry = {...createEntry(WS, [row({turn_id: EVENT, status: "received", response_text: null})]),
    apply: next => { entry.snapshot = {...entry.snapshot, events: next(entry.snapshot.events)}; }, isCurrent: () => true};
  entries.push(entry);
  return entry;
}

function chunk(fence: string, text: string, sequence = 0): Extract<LiveFrame, {kind: typeof FRAME_KIND.CHUNK}> {
  return {kind: FRAME_KIND.CHUNK, turn_id: EVENT, fencing_token: fence, text,
    text_kind: "answer", stream_start: sequence === 0, stream_contiguous: true, stream_seq: sequence};
}

function hint(fence: string): Extract<LiveFrame, {kind: typeof FRAME_KIND.RUN_RESTARTED}> {
  return {kind: FRAME_KIND.RUN_RESTARTED, turn_id: EVENT, fencing_token: fence};
}

function validActivityFrames(): ActivityLiveFrame[] {
  const id = "019f0000-0000-7000-8000-000000000001";
  const progress = {activity_id: id, task_id: id, source_admission_id: null, operation: "goal",
    phase: "starting", provider: null, model: null, completed: 0, total: null};
  return [
    chunk("1", "replacement output"),
    {kind: FRAME_KIND.TOOL_CALL_STARTED, turn_id: EVENT, call_id: "1:0", name: "shell", args_redacted: {command: "pwd"}},
    {kind: FRAME_KIND.TOOL_CALL_PROGRESS, turn_id: EVENT, call_id: "1:0", name: "shell", elapsed_ms: 10},
    {kind: FRAME_KIND.TOOL_CALL_COMPLETED, turn_id: EVENT, call_id: "1:0", name: "shell", ms: 10, status: "succeeded"},
    {kind: FRAME_KIND.CONTEXT_PROGRESS, turn_id: EVENT, generation: 1, state: "summarizing"},
    {kind: FRAME_KIND.TASK_DECISION_STARTED, turn_id: EVENT, progress},
    {kind: FRAME_KIND.TASK_DECISION_COMPLETED, turn_id: EVENT, progress: {...progress, phase: "recorded"}},
    {kind: FRAME_KIND.TASK_TRANSFER_PROGRESS, turn_id: EVENT,
      progress: {...progress, operation: "capture", phase: "uploading", total: 10}},
  ];
}

afterEach(() => {
  for (const entry of entries.splice(0)) disposeReplyStreams(entry);
  setTurnDetailReader(null);
});

describe("attempt ownership on the live tail", () => {
  it("a newer attempt resets once and ignores delayed older output", async () => {
    const entry = live();
    onFrame(entry, Z_A, chunk("9007199254740992", "old draft"));
    await vi.waitFor(() => expect(entry.snapshot.events[0]?.reply).toBe("old draft"));
    onFrame(entry, Z_A, {kind: FRAME_KIND.TOOL_CALL_STARTED, turn_id: EVENT,
      fencing_token: "9007199254740992", call_id: "old:0", name: "shell", args_redacted: {}});
    // The hint was lost. Ordinary output with the next exact bigint still resets.
    onFrame(entry, Z_A, chunk("9007199254740993", "new draft"));
    onFrame(entry, Z_A, hint("9007199254740993"));
    onFrame(entry, Z_A, hint("9007199254740992"));
    onFrame(entry, Z_A, chunk("9007199254740992", "STALE", 1));
    await vi.waitFor(() => expect(entry.snapshot.events[0]?.reply).toBe("new draft"));
    expect(entry.snapshot.events[0]).toMatchObject({runRestarted: true, replyRecovering: false,
      tools: [{done: true, status: "interrupted"}]});
    expect(entry.replyAttempts.get(EVENT)).toBe("9007199254740993");
  });

  it("silently initializes from ordinary output but marks a first explicit restart", async () => {
    const entry = live();
    onFrame(entry, Z_A, chunk("17", "first observed"));
    await vi.waitFor(() => expect(entry.snapshot.events[0]?.reply).toBe("first observed"));
    expect(entry.snapshot.events[0]?.runRestarted).toBeUndefined();
    const explicit = live();
    onFrame(explicit, Z_A, hint("17"));
    expect(explicit.snapshot.events[0]?.runRestarted).toBe(true);
    onFrame(explicit, Z_A, chunk("17", "after hint"));
    await vi.waitFor(() => expect(explicit.snapshot.events[0]?.reply).toBe("after hint"));
    onFrame(explicit, Z_A, hint("17"));
    expect(explicit.snapshot.events[0]?.reply).toBe("after hint");
  });

  it.each([undefined, "", "0", "01", "+1", "1.0", "1e2", "-1", "9223372036854775808", "99999999999999999999"])(
    "drops a missing or malformed fence %s", fence => {
      const entry = live();
      onFrame(entry, Z_A, {...chunk("1", "untrusted"), fencing_token: fence});
      expect(entry.replyAttempts.size).toBe(0);
      expect(entry.replyStreams.size).toBe(0);
    });

  it.each(validActivityFrames())("admits current $kind and rejects the same valid older payload", async frame => {
    const current = live();
    onFrame(current, Z_A, hint("9223372036854775807"));
    const beforeCurrent = current.snapshot.events;
    onFrame(current, Z_A, {...frame, fencing_token: "9223372036854775807"});
    await vi.waitFor(() => expect(current.snapshot.events).not.toBe(beforeCurrent));
    const stale = live();
    onFrame(stale, Z_A, hint("9223372036854775807"));
    const beforeStale = stale.snapshot.events;
    onFrame(stale, Z_A, {...frame, fencing_token: "1"});
    await new Promise(resolve => setTimeout(resolve, 0));
    expect(stale.snapshot.events).toBe(beforeStale);
    expect(stale.replyStreams.size).toBe(0);
  });

  it("a second start in the same attempt remains a gap", async () => {
    const entry = live();
    onFrame(entry, Z_A, chunk("2", "partial"));
    await vi.waitFor(() => expect(entry.snapshot.events[0]?.reply).toBe("partial"));
    onFrame(entry, Z_A, chunk("2", "second start"));
    expect(entry.replyGaps.has(EVENT)).toBe(true);
    expect(entry.snapshot.events[0]).toMatchObject({reply: "", replyRecovering: true});
    expect(entry.snapshot.events[0]?.runRestarted).toBeUndefined();
  });

  it("drops queued old decoder work and an in-flight old detail read", async () => {
    const entry = live();
    onFrame(entry, Z_A, chunk("1", "old queued output"));
    const detail = Promise.withResolvers<{ok: true; data: ReturnType<typeof row>}>();
    recoverFinalReply(entry, Z_A, EVENT, entry.apply, entry.isCurrent, () => detail.promise);
    onFrame(entry, Z_A, hint("2"));
    onFrame(entry, Z_A, chunk("2", "current output"));
    detail.resolve({ok: true, data: row({turn_id: EVENT, response_text: "OLD SAVED READ"})});
    await vi.waitFor(() => expect(entry.snapshot.events[0]?.reply).toBe("current output"));
    expect(entry.snapshot.events[0]?.status).toBe("received");
  });

  it("does not reopen when a hint overtakes asynchronous completion", async () => {
    const entry = live();
    onFrame(entry, Z_A, chunk("1", "draft"));
    onFrame(entry, Z_A, {kind: FRAME_KIND.TURN_COMPLETE, turn_id: EVENT, status: "processed", final_reply: "saved"});
    onFrame(entry, Z_A, hint("2"));
    onFrame(entry, Z_A, chunk("2", "too late"));
    await vi.waitFor(() => expect(entry.snapshot.events[0]?.reply).toBe("saved"));
    expect(entry.replyAttempts.get(EVENT)).toBe("1");
    expect(entry.snapshot.events[0]?.runRestarted).toBeUndefined();
  });

  it("rejects id-less activity and a saved terminal row, and preserves unrelated rows", () => {
    const entry = live();
    for (const turnId of [undefined, 7]) {
      const frame = JSON.parse(JSON.stringify({kind: FRAME_KIND.CHUNK, turn_id: turnId, fencing_token: "1"})) as LiveFrame;
      expect(acceptAttemptFrame(entry, frame, entry.apply)).toBe(false);
    }
    const unrelated = {...entry.snapshot.events[0]!, id: "unrelated"};
    entry.snapshot.events.push(unrelated);
    onFrame(entry, Z_A, hint("9"));
    onFrame(entry, Z_A, hint("10"));
    expect(entry.replyAttempts.get(EVENT)).toBe("10");
    expect(entry.snapshot.events[1]).toBe(unrelated);
    entry.snapshot.events[0]!.status = "processed";
    onFrame(entry, Z_A, hint("11"));
    expect(entry.replyAttempts.get(EVENT)).toBe("10");
  });

  it("removes provisional tools only once the winning saved detail supplies its trace", () => {
    const entry = live();
    entry.snapshot.events[0]!.tools = [{name: "shell", callId: "1:0", done: true,
      startedAtMs: 0, ms: 3, status: "succeeded"}];
    onFrame(entry, Z_A, hint("2"));
    const detail = row({turn_id: EVENT, status: "processed", response_text: "winning reply", tool_calls: null});
    const {request_json: _request, response_text: _response, tool_calls: _trace, ...listRow} = detail;
    const fromList = mergeBackfill(entry.snapshot.events, [listRow]);
    expect(fromList[0]?.tools).toHaveLength(1);
    const fromDetail = mergeBackfill(fromList, [detail]);
    expect(fromDetail[0]?.tools).toBeUndefined();
    expect(fromDetail[0]?.reply).toBe("winning reply");
    expect(fromDetail[0]?.runRestarted).toBe(true);
  });

  it("releases a decoder when its subscriber leaves before asynchronous completion", async () => {
    const entry = live();
    let current = true;
    entry.isCurrent = () => current;
    onFrame(entry, Z_A, chunk("1", "draft"));
    onFrame(entry, Z_A, {kind: FRAME_KIND.TURN_COMPLETE, turn_id: EVENT, status: "processed", final_reply: "saved"});
    current = false;
    await vi.waitFor(() => expect(entry.replyStreams.size).toBe(0));
    expect(entry.snapshot.events[0]?.status).toBe("received");
  });

  it.each(["inline", "backfill"])("ignores a disposed decoder's pending %s completion", async completion => {
    const entry = live();
    onFrame(entry, Z_A, chunk("1", "draft"));
    if (completion === "inline") {
      onFrame(entry, Z_A, {kind: FRAME_KIND.TURN_COMPLETE, turn_id: EVENT, status: "processed", final_reply: "saved"});
    } else {
      settleRepliesFromBackfill(entry, Z_A, [row({turn_id: EVENT, status: "processed"})], entry.apply, entry.isCurrent);
    }
    disposeReplyStreams(entry);
    const before = entry.snapshot;
    await new Promise(resolve => setTimeout(resolve, 0));
    expect(entry.snapshot).toBe(before);
  });

});
