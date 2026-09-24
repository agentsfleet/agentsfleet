import { beforeEach, describe, expect, it, vi } from "vitest";
import { FRAME_KIND } from "@/lib/api/events-types";
import { getSnapshot, retryConnection, subscribe } from "./fleet-stream-registry";
import { createEntry } from "./fleet-stream-entry";
import { applyFinalReply, applyReplyDelta } from "./fleet-stream-reply-frames";
import type { FleetEvent } from "./fleet-stream-row";
import { dispatchReplyFrame, markReplyGap, settleRepliesFromBackfill } from "./fleet-stream-reply-registry";
import { setupRegistryTests, row, sourceAt, WS, Z_A } from "@/tests/helpers/fleet-stream-registry-fixtures";
import { setupBackfillTests, fetchSpy, flushBackfill, pageWith, reconnect, MISSED_AT_MS, SEED_AT_MS } from "@/tests/helpers/fleet-stream-backfill-fixtures";

const getFleetEventActionMock = vi.hoisted(() => vi.fn());
const failedAction = vi.hoisted(() => ({ enabled: false, calls: 0 }));
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({
  getFleetEventAction: (...args: unknown[]) => {
    if (failedAction.enabled) {
      failedAction.calls += 1;
      return Promise.reject(new Error("detail unavailable"));
    }
    return getFleetEventActionMock(...args);
  },
}));

setupRegistryTests();
setupBackfillTests();
beforeEach(() => {
  getFleetEventActionMock.mockReset();
  getFleetEventActionMock.mockResolvedValue({ ok: false });
  failedAction.enabled = false;
  failedAction.calls = 0;
});

describe("fleet stream reply delivery", () => {
  it("keeps unrelated rows while opening an orphan and ignores a late final reply", () => {
    const prior = applyReplyDelta([], "unrelated", { answer: "Earlier", reasoning: "", thinking: false });
    const opened = applyReplyDelta(prior, "orphan", { answer: "", reasoning: "Working", thinking: true });
    expect(opened.map((event) => event.id)).toEqual(["unrelated", "orphan"]);
    expect(opened[0]?.reply).toBe("Earlier");
    expect(opened[1]).toMatchObject({ reasoning: "Working", thinking: true });
    const recovered = applyFinalReply(opened, row({ event_id: "expired", response_text: "Late answer" }));
    expect(recovered.find((event) => event.id === "expired")?.reply).toBe("Late answer");
    expect(applyFinalReply(opened, row({ event_id: "orphan", response_text: null }))[1]?.reply).toBe("");
  });

  it("drops parser callbacks once their registry entry has been replaced", async () => {
    vi.useRealTimers();
    const entry = createEntry(WS, []);
    const apply = vi.fn();
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.CHUNK, event_id: "stale", text: "Private", stream_start: true, stream_contiguous: true, stream_seq: 0 }, apply, () => false);
    await entry.replyStreams.get("stale")!.finish();
    expect(apply).not.toHaveBeenCalled();
  });

  it("ignores a completed decoder after its registry entry expires", async () => {
    vi.useRealTimers();
    const entry = createEntry(WS, []);
    const apply = vi.fn();
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.CHUNK, event_id: "stale", text: "Visible", stream_start: true, stream_contiguous: true, stream_seq: 0 }, apply, () => true);
    await vi.waitFor(() => expect(apply).toHaveBeenCalled());
    apply.mockClear();
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "stale", status: "processed" }, apply, () => false);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(apply).not.toHaveBeenCalled();
  });

  it("settles a lost completion from the terminal backfill row", async () => {
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: "evt_lost", response_text: "Recovered after gap" }) });
    fetchSpy.mockResolvedValueOnce(pageWith([row({ event_id: "evt_lost", created_at: MISSED_AT_MS, response_text: null })]));
    const release = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    sourceAt(0).emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_lost", text: "<tool_result>PRIVATE" });
    reconnect();
    await flushBackfill();
    expect(getFleetEventActionMock).toHaveBeenCalledWith(WS, Z_A, "evt_lost");
    expect(getSnapshot(Z_A).events.find((event) => event.id === "evt_lost")?.reply).toBe("Recovered after gap");
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    release();
  });

  it("never displays a late subscriber's mid-tool JSON and recovers the final answer", async () => {
    vi.useRealTimers();
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: "evt_late", response_text: "Safe final answer" }) });
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({
      kind: FRAME_KIND.CHUNK,
      event_id: "evt_late",
      text: '{"name":"memory_recall","arguments":{"query":"PRIVATE"}}',
      stream_start: false,
    });
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_late", text: "PRIVATE continuation", stream_start: false });
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_late", status: "processed" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Safe final answer"));
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    release();
  });

  it("recovers the durable answer after a runner loses a middle chunk", async () => {
    vi.useRealTimers();
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: "evt_pipe_gap", response_text: "Recovered answer" }) });
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_pipe_gap", text: "Safe <think>", stream_start: true, stream_contiguous: true });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Safe "));
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_pipe_gap", text: "PRIVATE mid-tool JSON", stream_start: false, stream_contiguous: false });
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_pipe_gap", status: "processed" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Recovered answer"));
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    release();
  });

  it("hides a downstream missing batch before its tool bytes reach the decoder", async () => {
    vi.useRealTimers();
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: "evt_batch_gap", response_text: "Durable answer" }) });
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_batch_gap", text: "Checking ", stream_seq: 0 });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Checking "));
    // Sequence 1 contained the tool opener; publication lost that whole batch.
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_batch_gap", text: '{"secret":"PRIVATE"}', stream_seq: 2 });
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_batch_gap", status: "processed" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Durable answer"));
    release();
  });

  it("replaces a partial draft when only the final chunk was lost", async () => {
    vi.useRealTimers();
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: "evt_last_gap", response_text: "Safe completed" }) });
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_last_gap", text: "Safe ", stream_seq: 0 });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Safe "));
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_last_gap", status: "processed" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Safe completed"));
    release();
  });

  it("recovers a completed reply when every chunk was lost or the viewer joined late", async () => {
    vi.useRealTimers();
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: "evt_no_chunks", response_text: "Complete answer" }) });
    const release = subscribe(WS, Z_A, [], () => {});
    sourceAt(0).emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_no_chunks", status: "processed", actor: "fleet", created_at: Date.now() });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Complete answer"));
    release();
  });

  it("ignores activity that arrives after the durable completion", async () => {
    vi.useRealTimers();
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: "evt_late_activity", response_text: "Durable answer" }) });
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_late_activity", status: "processed", actor: "fleet", created_at: Date.now() });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Durable answer"));
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_late_activity", text: "PRIVATE late draft", stream_start: true, stream_contiguous: true, stream_seq: 0 });
    source.emit({ kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: "evt_late_activity", name: "late_tool", args_redacted: true });
    source.emit({ kind: FRAME_KIND.TOOL_CALL_PROGRESS, event_id: "evt_late_activity", name: "late_tool", elapsed_ms: 1 });
    source.emit({ kind: FRAME_KIND.TOOL_CALL_COMPLETED, event_id: "evt_late_activity", name: "late_tool", ms: 2 });
    expect(getSnapshot(Z_A).events[0]?.reply).toBe("Durable answer");
    expect(getSnapshot(Z_A).events[0]?.tools).toBeUndefined();
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    release();
  });

  it("recovers after a transient final read failure without reconnecting", async () => {
    vi.useRealTimers();
    getFleetEventActionMock
      .mockRejectedValueOnce(new Error("temporary detail failure"))
      .mockResolvedValue({ ok: true, data: row({ event_id: "evt_transient", response_text: "Complete answer" }) });
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_transient", text: "Partial " });
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_transient", status: "processed" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Complete answer"));
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(2);
    release();
  });

  it("keeps retrying a failed final read at a bounded rate and settles when detail returns", async () => {
    getFleetEventActionMock.mockResolvedValue({ ok: false });
    const entry = createEntry(WS, []);
    let events: FleetEvent[] = applyReplyDelta([], "evt_all_fail", { answer: "Partial", reasoning: "", thinking: false });
    const apply = vi.fn((next: (prev: FleetEvent[]) => FleetEvent[]) => { events = next(events); });
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_all_fail", status: "processed" }, apply, () => true);
    await vi.advanceTimersByTimeAsync(400);
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(3);
    expect(events[0]?.replyRecovering).toBe(true);
    expect(entry.replyGaps.has("evt_all_fail")).toBe(true);
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: "evt_all_fail", response_text: "Eventually available" }) });
    await vi.advanceTimersByTimeAsync(5_000);
    expect(events[0]?.reply).toBe("Eventually available");
    expect(events[0]?.replyRecovering).toBe(false);
    expect(entry.replyRecoveries.has("evt_all_fail")).toBe(false);
    expect(entry.replyGaps.has("evt_all_fail")).toBe(false);
  });

  it("backs off a permanent detail refusal while keeping the reply safe", async () => {
    getFleetEventActionMock
      .mockResolvedValueOnce({ ok: false, status: 401 })
      .mockResolvedValue({ ok: true, data: row({ event_id: "evt_denied", response_text: "Recovered after login" }) });
    const entry = createEntry(WS, []);
    let events: FleetEvent[] = applyReplyDelta([], "evt_denied", { answer: "Partial", reasoning: "", thinking: false });
    const apply = vi.fn((next: (prev: FleetEvent[]) => FleetEvent[]) => { events = next(events); });
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_denied", status: "processed" }, apply, () => true);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(1);
    expect(events[0]?.replyRecovering).toBe(true);
    await vi.advanceTimersByTimeAsync(55_000);
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(2);
    expect(events[0]?.reply).toBe("Recovered after login");
    expect(events[0]?.replyRecovering).toBe(false);
  });

  it("keeps the short retry cadence for a rate-limited detail read", async () => {
    getFleetEventActionMock
      .mockResolvedValueOnce({ ok: false, status: 429 })
      .mockResolvedValue({ ok: true, data: row({ event_id: "evt_rate", response_text: "Ready" }) });
    const entry = createEntry(WS, []);
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_rate", status: "processed" }, vi.fn(), () => true);
    await vi.advanceTimersByTimeAsync(100);
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(2);
  });

  it("ignores a final read that succeeds after its subscriber leaves", async () => {
    vi.useRealTimers();
    let resolveDetail!: (value: unknown) => void;
    getFleetEventActionMock.mockReturnValue(new Promise((resolve) => { resolveDetail = resolve; }));
    const entry = createEntry(WS, []);
    const apply = vi.fn();
    let current = true;
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_gone", status: "processed" }, apply, () => current);
    apply.mockClear();
    current = false;
    resolveDetail({ ok: true, data: row({ event_id: "evt_gone", response_text: "Too late" }) });
    await vi.waitFor(() => expect(entry.replyRecoveries.has("evt_gone")).toBe(false));
    expect(apply).not.toHaveBeenCalled();
  });

  it("retries a failed durable reply read on terminal history backfill", async () => {
    vi.useRealTimers();
    getFleetEventActionMock
      .mockResolvedValueOnce({ ok: false })
      .mockResolvedValue({ ok: true, data: row({ event_id: "evt_retry", response_text: "Recovered after retry" }) });
    const entry = createEntry(WS, []);
    const apply = vi.fn();
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.CHUNK, event_id: "evt_retry", text: "Partial", stream_start: true, stream_contiguous: true, stream_seq: 0 }, apply, () => true);
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_retry", status: "processed" }, apply, () => true);
    await vi.waitFor(() => expect(entry.replyGaps.has("evt_retry")).toBe(true));
    await vi.waitFor(() => expect(getFleetEventActionMock).toHaveBeenCalledTimes(1));
    settleRepliesFromBackfill(entry, Z_A, [row({ event_id: "evt_retry", status: "processed" })], apply, () => true);
    await vi.waitFor(() => expect(getFleetEventActionMock).toHaveBeenCalledTimes(2));
    await vi.waitFor(() => expect(entry.replyGaps.has("evt_retry")).toBe(false));
    expect(apply).toHaveBeenCalled();
  });

  it("does not start a duplicate final read while one is pending", async () => {
    getFleetEventActionMock.mockReturnValueOnce(new Promise(() => {}));
    const entry = createEntry(WS, []);
    const apply = vi.fn();
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.CHUNK, event_id: "evt_pending", text: "Draft", stream_start: true, stream_contiguous: true, stream_seq: 0 }, apply, () => true);
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.CHUNK, event_id: "evt_pending", text: "PRIVATE", stream_seq: 2, stream_contiguous: true }, apply, () => true);
    const terminal = row({ event_id: "evt_pending", status: "processed" });
    settleRepliesFromBackfill(entry, Z_A, [terminal], apply, () => true);
    settleRepliesFromBackfill(entry, Z_A, [terminal], apply, () => true);
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(1);
    expect(entry.replyStreams.has("evt_pending")).toBe(false);
  });

  it.each([true, false])("finishes an intact stream from terminal history when current=%s", async (current) => {
    vi.useRealTimers();
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: "evt_terminal", response_text: "Durable" }) });
    const entry = createEntry(WS, []);
    const apply = vi.fn((next: (prev: FleetEvent[]) => FleetEvent[]) => { next([]); });
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.CHUNK, event_id: "evt_terminal", text: "Draft", stream_start: true, stream_contiguous: true, stream_seq: 0 }, apply, () => current);
    settleRepliesFromBackfill(entry, Z_A, [row({ event_id: "evt_terminal", status: "processed" })], apply, () => current);
    await vi.waitFor(() => expect(entry.replyStreams.has("evt_terminal")).toBe(false));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(current ? 1 : 0);
    expect(entry.replyGaps.has("evt_terminal")).toBe(false);
  });

  it("recovers a reclaimed event instead of feeding its new run into old reasoning", async () => {
    vi.useRealTimers();
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: "evt_reclaim", response_text: "New final answer" }) });
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_reclaim", text: "<think>old attempt ", stream_start: true });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reasoning).toBe("old attempt "));
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_reclaim", text: "New answer PRIVATE", stream_start: true });
    expect(getSnapshot(Z_A).events[0]?.reasoning).toBe("");
    expect(getSnapshot(Z_A).events[0]?.reply).toBe("");
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_reclaim", status: "processed" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("New final answer"));
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    expect(getSnapshot(Z_A).events[0]?.reasoning).toBe("");
    release();
  });

  it("recovers a late subscriber from history when the completion frame is also missed", async () => {
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: "evt_late_gap", response_text: "History answer" }) });
    fetchSpy.mockResolvedValueOnce(pageWith([row({ event_id: "evt_late_gap", created_at: MISSED_AT_MS, response_text: null })]));
    const release = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    sourceAt(0).emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_late_gap", text: "PRIVATE mid-tool", stream_start: false });
    reconnect();
    await flushBackfill();
    expect(getSnapshot(Z_A).events.find((event) => event.id === "evt_late_gap")?.reply).toBe("History answer");
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    release();
  });

  it("leaves a still-running backfill row open and ignores a stale terminal row", async () => {
    vi.useRealTimers();
    const entry = createEntry(WS, []);
    const apply = vi.fn();
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.CHUNK, event_id: "running", text: "Safe", stream_start: true, stream_contiguous: true, stream_seq: 0 }, apply, () => false);
    markReplyGap(entry);
    settleRepliesFromBackfill(entry, Z_A, [row({ event_id: "running", status: "received" })], apply, () => false);
    expect(entry.replyStreams.has("running")).toBe(true);
    settleRepliesFromBackfill(entry, Z_A, [row({ event_id: "running", status: "processed" })], apply, () => false);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(entry.replyStreams.has("running")).toBe(false);
    expect(getFleetEventActionMock).not.toHaveBeenCalled();
  });

  it.each([
    { ok: false, data: undefined },
    { ok: true, data: row({ response_text: null }) },
  ])("keeps protocol hidden when the final row is unavailable: %j", async (response) => {
    vi.useRealTimers();
    getFleetEventActionMock.mockResolvedValue(response);
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_unavailable", text: "<tool_result>PRIVATE" });
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_unavailable", status: "processed" });
    await vi.waitFor(() => expect(getFleetEventActionMock).toHaveBeenCalled());
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    release();
  });

  it("keeps tool protocol hidden when the final row read fails", async () => {
    vi.useRealTimers();
    failedAction.enabled = true;
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_read_failure", text: "<tool_result>PRIVATE" });
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_read_failure", status: "processed" });
    await vi.waitFor(() => expect(failedAction.calls).toBe(1));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    release();
  });

  it("shows answer bytes before completion and settles the same event", async () => {
    vi.useRealTimers();
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "evt_live", actor: "fleet" });
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_live", text: "Hel" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Hel"));
    expect(getSnapshot(Z_A).events[0]?.status).toBe("received");
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_live", text: "lo" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Hello"));
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_live", status: "processed" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.status).toBe("processed"));
    expect(getSnapshot(Z_A).events[0]?.reply).toBe("Hello");
    release();
  });

  it("keeps quoted tool arguments out of the typed reasoning and answer", async () => {
    vi.useRealTimers();
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    const raw = '<think>Checking <tool_call>{"name":"memory_store","arguments":{"content":"x </tool_call> PRIVATE"}}</tool_call>Done.</think>Answer.';
    for (const text of raw.match(/.{1,7}/gs) ?? []) {
      source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_orphan", text });
    }
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_orphan", status: "processed" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Answer."));
    expect(getSnapshot(Z_A).events[0]?.reasoning).toBe("Checking Done.");
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    release();
  });

  it("reads the durable final answer after an ambiguous tool result", async () => {
    vi.useRealTimers();
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: "evt_result", response_text: "Final answer" }) });
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "evt_result", actor: "fleet" });
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_result", text: "<think>Checking <tool_result>echo </tool_result> PRIVATE" });
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_result", text: "</tool_result> Done.</think>Raw answer." });
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_result", status: "processed" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Final answer"));
    expect(getFleetEventActionMock).toHaveBeenCalledWith(WS, Z_A, "evt_result");
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    release();
  });

  it("restores literal HTML code after the live safety parser pauses", async () => {
    vi.useRealTimers();
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: "evt_html", response_text: "Use `<div>` in JSX." }) });
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_html", text: "Use `<div>` in JSX." });
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_html", status: "processed" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Use `<div>` in JSX."));
    release();
  });

  it("suppresses resumed raw bytes after a connection gap", async () => {
    vi.useRealTimers();
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: "evt_gap", response_text: "Recovered answer" }) });
    const release = subscribe(WS, Z_A, [], () => {});
    sourceAt(0).emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_gap", text: "Safe " });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Safe "));
    sourceAt(0).fail();
    retryConnection(Z_A);
    const recovered = sourceAt(-1);
    recovered.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_gap", text: "PRIVATE continuation" });
    recovered.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_gap", status: "processed" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Recovered answer"));
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    release();
  });
});
