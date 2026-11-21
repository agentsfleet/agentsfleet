import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LiveFrame } from "@/lib/api/turns";
import { FRAME_KIND } from "@/lib/api/turns-types";
import { OUTCOME } from "@/lib/turns/turn-summary";
import { getSnapshot, subscribe } from "@/tests/helpers/fleet-streams";
import { createEntry } from "./fleet-stream-entry";
import { applyLiveFrame } from "./fleet-stream-frames";
import { applyReplyDelta } from "./fleet-stream-reply-frames";
import type { FleetTurn } from "./fleet-stream-row";
import { setTurns } from "./fleet-stream-snapshot";
import { dispatchReplyFrame, markReplyGap, setTurnDetailReader, settleRepliesFromBackfill, type ApplyTurns, type TurnDetailReader } from "./fleet-stream-reply-registry";
import { setupRegistryTests, row, sourceAt, WS, Z_A } from "@/tests/helpers/fleet-stream-registry-fixtures";
import { setupBackfillTests, fetchSpy, flushBackfill, pageWith, reconnect, reconnectAgain, MISSED_AT_MS, SEED_AT_MS } from "@/tests/helpers/fleet-stream-backfill-fixtures";
import { fleetActionsMock, getFleetTurnActionMock, resetFleetTurnAction } from "@/tests/helpers/fleet-stream-reply-action-mock";

setupRegistryTests();
setupBackfillTests();
beforeEach(() => {
  resetFleetTurnAction();
  setTurnDetailReader(fleetActionsMock().getFleetTurnAction as TurnDetailReader);
});

describe("fleet stream durable final read", () => {
  it("ignores activity that arrives after the durable completion", async () => {
    vi.useRealTimers();
    getFleetTurnActionMock.mockResolvedValue({ ok: true, data: row({ turn_id: "evt_late_activity", response_text: "Durable answer" }) });
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.TURN_COMPLETE, turn_id: "evt_late_activity", status: "processed", actor: "fleet", created_at: new Date().toISOString() });
    await vi.waitFor(() => expect(getSnapshot(Z_A).turns[0]?.reply).toBe("Durable answer"));
    source.emit({ text_kind: "answer", kind: FRAME_KIND.CHUNK, turn_id: "evt_late_activity", text: "PRIVATE late draft", stream_start: true, stream_contiguous: true, stream_seq: 0 });
    source.emit({ call_id: "late_tool", kind: FRAME_KIND.TOOL_CALL_STARTED, turn_id: "evt_late_activity", name: "late_tool", args_redacted: true });
    source.emit({ call_id: "late_tool", kind: FRAME_KIND.TOOL_CALL_PROGRESS, turn_id: "evt_late_activity", name: "late_tool", elapsed_ms: 1 });
    source.emit({ call_id: "late_tool", status: "succeeded", kind: FRAME_KIND.TOOL_CALL_COMPLETED, turn_id: "evt_late_activity", name: "late_tool", ms: 2 });
    expect(getSnapshot(Z_A).turns[0]?.reply).toBe("Durable answer");
    expect(getSnapshot(Z_A).turns[0]?.tools).toBeUndefined();
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    release();
  });

  it("recovers after a transient final read failure without reconnecting", async () => {
    vi.useRealTimers();
    getFleetTurnActionMock
      .mockRejectedValueOnce(new Error("temporary detail failure"))
      .mockResolvedValue({ ok: true, data: row({ turn_id: "evt_transient", response_text: "Complete answer" }) });
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ text_kind: "answer", kind: FRAME_KIND.CHUNK, turn_id: "evt_transient", text: "Partial " });
    source.emit({ kind: FRAME_KIND.TURN_COMPLETE, turn_id: "evt_transient", status: "processed" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).turns[0]?.reply).toBe("Complete answer"));
    expect(getFleetTurnActionMock).toHaveBeenCalledTimes(2);
    release();
  });

  it("keeps retrying a failed final read at a bounded rate and settles when detail returns", async () => {
    getFleetTurnActionMock.mockResolvedValue({ ok: false });
    const entry = createEntry(WS, []);
    let turns: FleetTurn[] = applyReplyDelta([], "evt_all_fail", { answer: "Partial", reasoning: "", thinking: false });
    const apply = vi.fn((next: (prev: FleetTurn[]) => FleetTurn[]) => { turns = next(turns); });
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.TURN_COMPLETE, turn_id: "evt_all_fail", status: "processed" }, apply, () => true);
    await vi.advanceTimersByTimeAsync(400);
    expect(getFleetTurnActionMock).toHaveBeenCalledTimes(3);
    expect(turns[0]?.replyRecovering).toBe(true);
    expect(entry.replyGaps.has("evt_all_fail")).toBe(true);
    getFleetTurnActionMock.mockResolvedValue({ ok: true, data: row({ turn_id: "evt_all_fail", response_text: "Eventually available" }) });
    await vi.advanceTimersByTimeAsync(5_000);
    expect(turns[0]?.reply).toBe("Eventually available");
    expect(turns[0]?.replyRecovering).toBe(false);
    expect(entry.replyRecoveries.has("evt_all_fail")).toBe(false);
    expect(entry.replyGaps.has("evt_all_fail")).toBe(false);
  });

  it("backs off a permanent detail refusal while keeping the reply safe", async () => {
    getFleetTurnActionMock
      .mockResolvedValueOnce({ ok: false, status: 401 })
      .mockResolvedValue({ ok: true, data: row({ turn_id: "evt_denied", response_text: "Recovered after login" }) });
    const entry = createEntry(WS, []);
    let turns: FleetTurn[] = applyReplyDelta([], "evt_denied", { answer: "Partial", reasoning: "", thinking: false });
    const apply = vi.fn((next: (prev: FleetTurn[]) => FleetTurn[]) => { turns = next(turns); });
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.TURN_COMPLETE, turn_id: "evt_denied", status: "processed" }, apply, () => true);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(getFleetTurnActionMock).toHaveBeenCalledTimes(1);
    expect(turns[0]?.replyRecovering).toBe(true);
    await vi.advanceTimersByTimeAsync(55_000);
    expect(getFleetTurnActionMock).toHaveBeenCalledTimes(2);
    expect(turns[0]?.reply).toBe("Recovered after login");
    expect(turns[0]?.replyRecovering).toBe(false);
  });

  it("stops reading a turn the read found gone, 404 or 410 alike", async () => {
    const PERMANENT_RETRY_MS = 60_000;
    for (const status of [404, 410]) {
      getFleetTurnActionMock.mockReset().mockResolvedValue({ ok: false, status });
      const entry = createEntry(WS, []);
      const turnId = `evt_gone_${status}`;
      let turns: FleetTurn[] = applyReplyDelta([], turnId, { answer: "Partial", reasoning: "", thinking: false });
      const apply = vi.fn((next: (prev: FleetTurn[]) => FleetTurn[]) => { turns = next(turns); });
      dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.TURN_COMPLETE, turn_id: turnId, status: "processed" }, apply, () => true);
      await vi.advanceTimersByTimeAsync(PERMANENT_RETRY_MS * 3);
      expect(getFleetTurnActionMock).toHaveBeenCalledTimes(1);
      expect(entry.replyRecoveries.has(turnId)).toBe(false);
      // Settled with a line, never left "Loading final reply".
      expect(turns[0]).toMatchObject({ status: "processed", reply: "", replyRecovering: false, outcome: OUTCOME.REPLY_GONE });
    }
  });

  it("keeps the short retry cadence for a rate-limited detail read", async () => {
    getFleetTurnActionMock
      .mockResolvedValueOnce({ ok: false, status: 429 })
      .mockResolvedValue({ ok: true, data: row({ turn_id: "evt_rate", response_text: "Ready" }) });
    const entry = createEntry(WS, []);
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.TURN_COMPLETE, turn_id: "evt_rate", status: "processed" }, vi.fn(), () => true);
    await vi.advanceTimersByTimeAsync(100);
    expect(getFleetTurnActionMock).toHaveBeenCalledTimes(2);
  });

  it("ignores a final read that succeeds after its subscriber leaves", async () => {
    vi.useRealTimers();
    let resolveDetail!: (value: unknown) => void;
    getFleetTurnActionMock.mockReturnValue(new Promise((resolve) => { resolveDetail = resolve; }));
    const entry = createEntry(WS, []);
    const apply = vi.fn();
    let current = true;
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.TURN_COMPLETE, turn_id: "evt_gone", status: "processed" }, apply, () => current);
    apply.mockClear();
    current = false;
    resolveDetail({ ok: true, data: row({ turn_id: "evt_gone", response_text: "Too late" }) });
    await vi.waitFor(() => expect(entry.replyRecoveries.has("evt_gone")).toBe(false));
    expect(apply).not.toHaveBeenCalled();
  });

  it("retries a failed durable reply read on terminal history backfill", async () => {
    vi.useRealTimers();
    getFleetTurnActionMock
      .mockResolvedValueOnce({ ok: false })
      .mockResolvedValue({ ok: true, data: row({ turn_id: "evt_retry", response_text: "Recovered after retry" }) });
    const entry = createEntry(WS, []);
    const apply = vi.fn();
    dispatchReplyFrame(entry, Z_A, { text_kind: "answer", kind: FRAME_KIND.CHUNK, turn_id: "evt_retry", text: "Partial", stream_start: true, stream_contiguous: true, stream_seq: 0 }, apply, () => true);
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.TURN_COMPLETE, turn_id: "evt_retry", status: "processed" }, apply, () => true);
    await vi.waitFor(() => expect(entry.replyGaps.has("evt_retry")).toBe(true));
    await vi.waitFor(() => expect(getFleetTurnActionMock).toHaveBeenCalledTimes(1));
    settleRepliesFromBackfill(entry, Z_A, [row({ turn_id: "evt_retry", status: "processed" })], apply, () => true);
    await vi.waitFor(() => expect(getFleetTurnActionMock).toHaveBeenCalledTimes(2));
    await vi.waitFor(() => expect(entry.replyGaps.has("evt_retry")).toBe(false));
    expect(apply).toHaveBeenCalled();
  });

  it("does not start a duplicate final read while one is pending", async () => {
    getFleetTurnActionMock.mockReturnValueOnce(new Promise(() => {}));
    const entry = createEntry(WS, []);
    const apply = vi.fn();
    dispatchReplyFrame(entry, Z_A, { text_kind: "answer", kind: FRAME_KIND.CHUNK, turn_id: "evt_pending", text: "Draft", stream_start: true, stream_contiguous: true, stream_seq: 0 }, apply, () => true);
    dispatchReplyFrame(entry, Z_A, { text_kind: "answer", kind: FRAME_KIND.CHUNK, turn_id: "evt_pending", text: "PRIVATE", stream_seq: 2, stream_contiguous: true }, apply, () => true);
    const terminal = row({ turn_id: "evt_pending", status: "processed" });
    settleRepliesFromBackfill(entry, Z_A, [terminal], apply, () => true);
    settleRepliesFromBackfill(entry, Z_A, [terminal], apply, () => true);
    expect(getFleetTurnActionMock).toHaveBeenCalledTimes(1);
    expect(entry.replyStreams.has("evt_pending")).toBe(false);
  });

  it.each([true, false])("finishes an intact stream from terminal history when current=%s", async (current) => {
    vi.useRealTimers();
    getFleetTurnActionMock.mockResolvedValue({ ok: true, data: row({ turn_id: "evt_terminal", response_text: "Durable" }) });
    const entry = createEntry(WS, []);
    const apply = vi.fn((next: (prev: FleetTurn[]) => FleetTurn[]) => { next([]); });
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.CHUNK, turn_id: "evt_terminal", text: "Draft", text_kind: "answer", stream_start: true, stream_contiguous: true, stream_seq: 0 }, apply, () => current);
    settleRepliesFromBackfill(entry, Z_A, [row({ turn_id: "evt_terminal", status: "processed" })], apply, () => current);
    await vi.waitFor(() => expect(entry.replyStreams.has("evt_terminal")).toBe(false));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(getFleetTurnActionMock).toHaveBeenCalledTimes(current ? 1 : 0);
    expect(entry.replyGaps.has("evt_terminal")).toBe(false);
  });

  it("recovers a reassigned turn instead of feeding its new run into old reasoning", async () => {
    vi.useRealTimers();
    getFleetTurnActionMock.mockResolvedValue({ ok: true, data: row({ turn_id: "evt_reassigned", response_text: "New final answer" }) });
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.CHUNK, turn_id: "evt_reassigned", text: "old attempt ", text_kind: "reasoning", stream_start: true, stream_contiguous: true, stream_seq: 0 });
    await vi.waitFor(() => expect(getSnapshot(Z_A).turns[0]?.reasoning).toBe("old attempt "));
    source.emit({ kind: FRAME_KIND.CHUNK, turn_id: "evt_reassigned", text: "New answer PRIVATE", text_kind: "answer", stream_start: true, stream_contiguous: true, stream_seq: 0 });
    expect(getSnapshot(Z_A).turns[0]?.reasoning).toBe("");
    expect(getSnapshot(Z_A).turns[0]?.reply).toBe("");
    source.emit({ kind: FRAME_KIND.TURN_COMPLETE, turn_id: "evt_reassigned", status: "processed" });
    await vi.waitFor(() => expect(getSnapshot(Z_A).turns[0]?.reply).toBe("New final answer"));
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    expect(getSnapshot(Z_A).turns[0]?.reasoning).toBe("");
    release();
  });

  it("recovers a late subscriber from history when the completion frame is also missed", async () => {
    getFleetTurnActionMock.mockResolvedValue({ ok: true, data: row({ turn_id: "evt_late_gap", response_text: "History answer" }) });
    fetchSpy.mockResolvedValueOnce(pageWith([row({ turn_id: "evt_late_gap", created_at: new Date(MISSED_AT_MS).toISOString(), response_text: null })]));
    const release = subscribe(WS, Z_A, [row({ turn_id: "evt_seed", created_at: new Date(SEED_AT_MS).toISOString() })], () => {});
    sourceAt(0).emit({ text_kind: "answer", kind: FRAME_KIND.CHUNK, turn_id: "evt_late_gap", text: "PRIVATE mid-tool", stream_start: false });
    reconnect();
    await flushBackfill();
    expect(getSnapshot(Z_A).turns.find((turn) => turn.id === "evt_late_gap")?.reply).toBe("History answer");
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    release();
  });

  it("leaves a still-running backfill row open and ignores a stale terminal row", async () => {
    vi.useRealTimers();
    const entry = createEntry(WS, []);
    const apply = vi.fn();
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.CHUNK, turn_id: "running", text: "Safe", text_kind: "answer", stream_start: true, stream_contiguous: true, stream_seq: 0 }, apply, () => false);
    markReplyGap(entry);
    settleRepliesFromBackfill(entry, Z_A, [row({ turn_id: "running", status: "received" })], apply, () => false);
    expect(entry.replyStreams.has("running")).toBe(true);
    settleRepliesFromBackfill(entry, Z_A, [row({ turn_id: "running", status: "processed" })], apply, () => false);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(entry.replyStreams.has("running")).toBe(false);
    expect(getFleetTurnActionMock).not.toHaveBeenCalled();
  });

  it.each([
    { ok: false, data: undefined },
    { ok: true, data: row({ response_text: null }) },
  ])("drops an unclassified chunk when the final row is unavailable: %j", async (response) => {
    vi.useRealTimers();
    getFleetTurnActionMock.mockResolvedValue(response);
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emitNamed(FRAME_KIND.CHUNK, JSON.stringify({kind: FRAME_KIND.CHUNK, turn_id: "evt_unavailable", text: "<tool_result>PRIVATE"}));
    source.emit({ kind: FRAME_KIND.TURN_COMPLETE, turn_id: "evt_unavailable", status: "processed" });
    await vi.waitFor(() => expect(getFleetTurnActionMock).toHaveBeenCalled());
    expect(JSON.stringify(getSnapshot(Z_A))).not.toContain("PRIVATE");
    release();
  });

  it("fetches nothing and keeps the gap open when no detail reader is installed", () => {
    // A bundle without the dashboard (the browser transport probe) never
    // installs the Server Action; recovery must stay idle rather than throw.
    setTurnDetailReader(null);
    const entry = createEntry(WS, []);
    dispatchReplyFrame(entry, Z_A, { kind: FRAME_KIND.TURN_COMPLETE, turn_id: "evt_no_reader", status: "processed" }, vi.fn(), () => true);
    expect(getFleetTurnActionMock).not.toHaveBeenCalled();
    expect(entry.replyRecoveries.has("evt_no_reader")).toBe(false);
    expect(entry.replyGaps.has("evt_no_reader")).toBe(true);
  });
});

// A turn that started and ended while the stream was away arrives only as a
// list row, which carries no bodies.
describe("a body-less row a backfill brings in", () => {
  const STEER_ID = "evt_while_away";
  const ASKED = "Deploy the preview";
  const ANSWERED = "Deployed.";
  const listRow = { ...row({ turn_id: STEER_ID, actor: "steer:user_abc", status: "processed", created_at: new Date(MISSED_AT_MS).toISOString() }), request_json: undefined, response_text: undefined };

  it("reads its saved row once, and shows the message and the answer", async () => {
    getFleetTurnActionMock.mockResolvedValue({
      ok: true,
      data: row({ turn_id: STEER_ID, actor: "steer:user_abc", status: "processed", created_at: new Date(MISSED_AT_MS).toISOString(), request_json: JSON.stringify({ message: ASKED }), response_text: ANSWERED }),
    });
    fetchSpy.mockResolvedValue(pageWith([listRow]));
    const release = subscribe(WS, Z_A, [row({ turn_id: "evt_seed", created_at: new Date(SEED_AT_MS).toISOString() })], () => {});
    const source = reconnect();
    await flushBackfill();
    await vi.advanceTimersByTimeAsync(0);
    expect(getSnapshot(Z_A).turns.find((turn) => turn.id === STEER_ID)).toMatchObject({ text: ASKED, reply: ANSWERED });

    // Another outage restating the same row reads nothing more.
    reconnectAgain(source);
    await flushBackfill();
    await vi.advanceTimersByTimeAsync(0);
    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(getFleetTurnActionMock).toHaveBeenCalledExactlyOnceWith(WS, Z_A, STEER_ID);
    release();
  });

  it("reads nothing for a row whose bodies it already holds, or with no reader", async () => {
    const entry = createEntry(WS, [row({ turn_id: STEER_ID, actor: "steer:user_abc", request_json: JSON.stringify({ message: ASKED }), response_text: ANSWERED })]);
    settleRepliesFromBackfill(entry, Z_A, [listRow], vi.fn(), () => true);
    settleRepliesFromBackfill(entry, Z_A, [{ ...listRow, turn_id: "evt_unknown" }], vi.fn(), () => true);
    setTurnDetailReader(null);
    const bodiless = createEntry(WS, [listRow]);
    settleRepliesFromBackfill(bodiless, Z_A, [listRow], vi.fn(), () => true);
    await vi.advanceTimersByTimeAsync(0);
    expect(getFleetTurnActionMock).not.toHaveBeenCalled();
    expect(bodiless.bodyReads.size).toBe(0);
  });
});

// A message past the frame bound rides its frames as a head; the saved row
// holds it whole. Unmarked, the same rows read nothing.
describe("a message its frames carried only the head of", () => {
  const STEER_ID = "evt_long_message";
  const SENDER = "steer:user_abc";
  const HEAD = "Deploy the preview and";
  const WHOLE = `${HEAD} run the smoke suite against it`;
  const ANSWERED = "Deployed.";
  const saved = row({ turn_id: STEER_ID, actor: SENDER, status: "processed", request_json: JSON.stringify({ message: WHOLE }), response_text: ANSWERED });
  const admitted = (trimmed: boolean): LiveFrame => ({ kind: FRAME_KIND.TURN_ADMITTED, turn_id: STEER_ID, actor: SENDER, message: HEAD, ...(trimmed ? { message_trimmed: true } : {}) });

  it.each([[true, WHOLE, 1], [false, HEAD, 0]])("reads a settled row with its answer when marked=%s", async (trimmed, text, reads) => {
    getFleetTurnActionMock.mockResolvedValue({ ok: true, data: saved });
    // The answer is on hand; the message is only what the frame carried.
    const entry = createEntry(WS, [row({ ...saved, request_json: "{}" })]);
    const apply: ApplyTurns = (next, facts) => setTurns(entry, next, facts);
    apply((prev) => applyLiveFrame(prev, admitted(trimmed)), {});
    const listRow = { ...saved, request_json: undefined, response_text: undefined };
    settleRepliesFromBackfill(entry, Z_A, [listRow], apply, () => true);
    settleRepliesFromBackfill(entry, Z_A, [listRow], apply, () => true);
    await vi.advanceTimersByTimeAsync(0);
    expect(getFleetTurnActionMock).toHaveBeenCalledTimes(reads);
    expect(entry.snapshot.turns[0]).toMatchObject({ text, reply: ANSWERED });
    expect(entry.snapshot.turns[0]?.textTrimmed).toBeUndefined();
  });

  it.each([[true, WHOLE, 1], [false, HEAD, 0]])("reads the saved row after an inline answer when marked=%s", async (trimmed, text, reads) => {
    getFleetTurnActionMock.mockResolvedValue({ ok: true, data: saved });
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit(admitted(trimmed));
    source.emit({ kind: FRAME_KIND.TURN_COMPLETE, turn_id: STEER_ID, status: "processed", final_reply: ANSWERED });
    await vi.advanceTimersByTimeAsync(0);
    expect(getFleetTurnActionMock).toHaveBeenCalledTimes(reads);
    expect(getSnapshot(Z_A).turns[0]).toMatchObject({ text, reply: ANSWERED, status: "processed" });
    expect(getSnapshot(Z_A).turns[0]?.textTrimmed).toBeUndefined();
    release();
  });
});
