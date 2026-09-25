import { beforeEach, describe, expect, it, vi } from "vitest";
import { FRAME_KIND } from "@/lib/api/events-types";
import { getSnapshot, subscribe } from "./fleet-stream-registry";
import { createEntry } from "./fleet-stream-entry";
import { applyReplyDelta } from "./fleet-stream-reply-frames";
import type { FleetEvent } from "./fleet-stream-row";
import { dispatchReplyFrame, markReplyGap, setEventDetailReader, settleRepliesFromBackfill, type EventDetailReader } from "./fleet-stream-reply-registry";
import { setupRegistryTests, row, sourceAt, WS, Z_A } from "@/tests/helpers/fleet-stream-registry-fixtures";
import { setupBackfillTests, fetchSpy, flushBackfill, pageWith, reconnect, MISSED_AT_MS, SEED_AT_MS } from "@/tests/helpers/fleet-stream-backfill-fixtures";
import { fleetActionsMock, getFleetEventActionMock, resetFleetEventAction } from "@/tests/helpers/fleet-stream-reply-action-mock";

setupRegistryTests();
setupBackfillTests();
beforeEach(() => {
  resetFleetEventAction();
  setEventDetailReader(fleetActionsMock().getFleetEventAction as EventDetailReader);
});

describe("fleet stream durable final read", () => {
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
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.CHUNK, event_id: "evt_terminal", text: "Draft", text_kind: "answer", stream_start: true, stream_contiguous: true, stream_seq: 0 }, apply, () => current);
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
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_reclaim", text: "old attempt ", text_kind: "reasoning", stream_start: true, stream_contiguous: true, stream_seq: 0 });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reasoning).toBe("old attempt "));
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_reclaim", text: "New answer PRIVATE", text_kind: "answer", stream_start: true, stream_contiguous: true, stream_seq: 0 });
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
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.CHUNK, event_id: "running", text: "Safe", text_kind: "answer", stream_start: true, stream_contiguous: true, stream_seq: 0 }, apply, () => false);
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

  it("fetches nothing and keeps the gap open when no detail reader is installed", () => {
    // A bundle without the dashboard (the browser transport probe) never
    // installs the Server Action; recovery must stay idle rather than throw.
    setEventDetailReader(null);
    const entry = createEntry(WS, []);
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_no_reader", status: "processed" }, vi.fn(), () => true);
    expect(getFleetEventActionMock).not.toHaveBeenCalled();
    expect(entry.replyRecoveries.has("evt_no_reader")).toBe(false);
    expect(entry.replyGaps.has("evt_no_reader")).toBe(true);
  });
});
