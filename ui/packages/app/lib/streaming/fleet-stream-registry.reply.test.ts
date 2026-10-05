import { beforeEach, describe, expect, it, vi } from "vitest";
import { FRAME_KIND } from "@/lib/api/events-types";
import { getSnapshot, retryConnection, subscribe } from "./fleet-stream-registry";
import { createEntry } from "./fleet-stream-entry";
import { applyFinalReply, applyFinalReplyText, applyReplyDelta } from "./fleet-stream-reply-frames";
import { rowToEvent, type FleetEvent } from "./fleet-stream-row";
import { dispatchReplyFrame, setEventDetailReader, type EventDetailReader } from "./fleet-stream-reply-registry";
import { setupRegistryTests, row, sourceAt, WS, Z_A } from "@/tests/helpers/fleet-stream-registry-fixtures";
import { setupBackfillTests, fetchSpy, flushBackfill, pageWith, reconnect, MISSED_AT_MS, SEED_AT_MS } from "@/tests/helpers/fleet-stream-backfill-fixtures";
import { failedAction, fleetActionsMock, getFleetEventActionMock, resetFleetEventAction } from "@/tests/helpers/fleet-stream-reply-action-mock";

setupRegistryTests();
setupBackfillTests();
beforeEach(() => {
  resetFleetEventAction();
  setEventDetailReader(fleetActionsMock().getFleetEventAction as EventDetailReader);
});

describe("fleet stream reply delivery", () => {
  it("ignores a late inline ending after its row was removed", () => {
    expect(applyFinalReplyText([], "missing", "late")).toEqual([]);
  });

  it("settles a typed draft from inline final text without a detail request or recovery state", async () => {
    vi.useRealTimers();
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_inline", text: "Draft", text_kind: "answer", stream_start: true, stream_contiguous: true, stream_seq: 0 });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Draft"));
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_inline", status: "processed", final_reply: "Saved answer" });
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_inline", text: "PRIVATE late", text_kind: "answer", stream_contiguous: true, stream_seq: 1 });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Saved answer"));
    expect(getSnapshot(Z_A).events[0]?.replyRecovering).toBe(false);
    expect(getFleetEventActionMock).not.toHaveBeenCalled();
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    release();
  });

  it("accepts an authoritative empty final reply without fetching detail", () => {
    const entry = createEntry(WS, []);
    let events: FleetEvent[] = applyReplyDelta([], "evt_empty", { answer: "draft", reasoning: "", thinking: false });
    const apply = vi.fn((next: (prev: FleetEvent[]) => FleetEvent[]) => { events = next(events); });
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_empty", status: "processed", final_reply: "" }, apply, () => true);
    expect(events[0]?.reply).toBe("");
    expect(events[0]?.replyRecovering).toBe(false);
    expect(getFleetEventActionMock).not.toHaveBeenCalled();
  });

  it("test_settle_rereads_the_trace_after_closing_a_call", async () => {
    // The answer arrives inline, but the turn closed a call it never heard
    // end: one read of the saved row says how the call really went.
    const entry = createEntry(WS, []);
    const open: FleetEvent = { ...rowToEvent(row({ event_id: "evt_tools", status: "received" })), tools: [{ name: "http_request", callId: "f1:0", startedAtMs: 1, ms: null, done: false }] };
    let events: FleetEvent[] = [open];
    const apply = vi.fn((next: (prev: FleetEvent[]) => FleetEvent[]) => { events = next(events); });
    const saved = { name: "http_request", call_id: "f1:0", arguments: {}, status: "succeeded", duration_ms: 40 };
    getFleetEventActionMock.mockResolvedValueOnce({ ok: true, data: row({ event_id: "evt_tools", status: "processed", response_text: "Done.", tool_calls: { calls: [saved], omitted_call_count: 0 } }) });
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_tools", status: "processed", final_reply: "Done." }, apply, () => true);
    // The guess shows until the read lands, then the saved outcome replaces it.
    expect(events[0]?.tools?.[0]).toMatchObject({ status: "interrupted", closedAtSettle: true });
    await vi.waitFor(() => expect(events[0]?.tools?.[0]).toMatchObject({ status: "succeeded", ms: 40 }));
    expect(events[0]?.tools?.[0]?.closedAtSettle).toBeUndefined();
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(1);
  });

  it("should leave the guess, without retrying, when the saved row cannot be read", async () => {
    vi.useRealTimers();
    const entry = createEntry(WS, []);
    let events: FleetEvent[] = [{ ...rowToEvent(row({ event_id: "evt_tools", status: "received" })), tools: [{ name: "http_request", callId: "f1:0", startedAtMs: 1, ms: null, done: false }] }];
    const apply = vi.fn((next: (prev: FleetEvent[]) => FleetEvent[]) => { events = next(events); });
    getFleetEventActionMock.mockRejectedValueOnce(new Error("offline"));
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_tools", status: "processed", final_reply: "Done." }, apply, () => true);
    await vi.waitFor(() => expect(getFleetEventActionMock).toHaveBeenCalledTimes(1));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(events[0]?.tools?.[0]).toMatchObject({ status: "interrupted", closedAtSettle: true });
    expect(events[0]?.reply).toBe("Done.");
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(1);
  });

  it("should read nothing without a reader, and apply nothing once the entry is gone", async () => {
    vi.useRealTimers();
    const open = () => [{ ...rowToEvent(row({ event_id: "evt_tools", status: "received" })), tools: [{ name: "http_request", callId: "f1:0", startedAtMs: 1, ms: null, done: false }] }];
    const complete = { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_tools", status: "processed", final_reply: "Done." } as const;
    setEventDetailReader(null);
    let events: FleetEvent[] = open();
    dispatchReplyFrame(createEntry(WS, []), Z_A, complete, (next) => { events = next(events); }, () => true);
    expect(events[0]?.tools?.[0]).toMatchObject({ closedAtSettle: true });
    setEventDetailReader(fleetActionsMock().getFleetEventAction as EventDetailReader);
    getFleetEventActionMock.mockResolvedValueOnce({ ok: true, data: row({ event_id: "evt_tools", status: "processed", tool_calls: { calls: [], omitted_call_count: 0 } }) });
    let current = true;
    const apply = vi.fn((next: (prev: FleetEvent[]) => FleetEvent[]) => { events = next(events); });
    events = open();
    dispatchReplyFrame(createEntry(WS, []), Z_A, complete, apply, () => current);
    current = false;
    await vi.waitFor(() => expect(getFleetEventActionMock).toHaveBeenCalledTimes(1));
    await new Promise((resolve) => setTimeout(resolve, 0));
    // Only the settle itself applied; the read landed for an entry no longer shown.
    expect(apply).toHaveBeenCalledTimes(1);
  });

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
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.CHUNK, event_id: "stale", text: "Private", text_kind: "answer", stream_start: true, stream_contiguous: true, stream_seq: 0 }, apply, () => false);
    await entry.replyStreams.get("stale")!.finish();
    expect(apply).not.toHaveBeenCalled();
  });

  it("ignores a completed decoder after its registry entry expires", async () => {
    vi.useRealTimers();
    const entry = createEntry(WS, []);
    const apply = vi.fn();
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.CHUNK, event_id: "stale", text: "Visible", text_kind: "answer", stream_start: true, stream_contiguous: true, stream_seq: 0 }, apply, () => true);
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
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_pipe_gap", text: "Safe ", text_kind: "answer", stream_start: true, stream_contiguous: true, stream_seq: 0 });
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
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_batch_gap", text: "Checking ", text_kind: "answer", stream_start: true, stream_contiguous: true, stream_seq: 0 });
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
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_last_gap", text: "Safe ", text_kind: "answer", stream_start: true, stream_contiguous: true, stream_seq: 0 });
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
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_live", text: "Hel", text_kind: "answer", stream_start: true, stream_contiguous: true, stream_seq: 0 });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Hel"));
    expect(getSnapshot(Z_A).events[0]?.status).toBe("received");
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_live", text: "lo", text_kind: "answer", stream_contiguous: true, stream_seq: 1 });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Hello"));
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_live", status: "processed" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.status).toBe("processed"));
    expect(getSnapshot(Z_A).events[0]?.reply).toBe("Hello");
    release();
  });

  it("renders typed reasoning and answer while keeping old raw tool frames hidden", async () => {
    vi.useRealTimers();
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_orphan", text: "Checking Done.", text_kind: "reasoning", stream_start: true, stream_contiguous: true, stream_seq: 0 });
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_orphan", text: "Answer.", text_kind: "answer", stream_contiguous: true, stream_seq: 1 });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Answer."));
    expect(getSnapshot(Z_A).events[0]?.reasoning).toBe("Checking Done.");
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_orphan", status: "processed", final_reply: "Answer." });
    await vi.waitFor(() => expect(getSnapshot(Z_A).events[0]?.reply).toBe("Answer."));
    expect(getSnapshot(Z_A).events[0]?.reasoning).toBe("Checking Done.");
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_orphan", text: "<tool_call>PRIVATE", stream_seq: 2 });
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
    sourceAt(0).emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_gap", text: "Safe ", text_kind: "answer", stream_start: true, stream_contiguous: true, stream_seq: 0 });
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
