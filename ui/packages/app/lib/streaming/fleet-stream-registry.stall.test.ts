import { beforeEach, describe, expect, it, vi } from "vitest";
import { FRAME_KIND } from "@/lib/api/events-types";
import { appendOptimistic, getSnapshot, reconcileOptimistic, subscribe } from "./fleet-stream-registry";
import { REPLY_STALL_MS, setEventDetailReader, type EventDetailReader } from "./fleet-stream-reply-registry";
import { AGENTSFLEET_EVENT_STATUS } from "./fleet-stream-row";
import { setupRegistryTests, row, sourceAt, WS, Z_A } from "@/tests/helpers/fleet-stream-registry-fixtures";
import { failedAction, fleetActionsMock, getFleetEventActionMock, resetFleetEventAction } from "@/tests/helpers/fleet-stream-reply-action-mock";

// Every running event's row reaches an ending the operator can see, even when
// the frame that would have ended it never arrives.

const { PROCESSED, RECEIVED } = AGENTSFLEET_EVENT_STATUS;
const RAN = "evt_ran";
const THINKING = "evt_thinking";
const ACTOR = "steer:pending";
const MESSAGE = "deploy";
// The server's keepalive cadence while nothing else is said.
const KEEPALIVE_MS = 15_000;
const SAVED = {
  ok: true,
  data: row({ event_id: RAN, actor: ACTOR, status: PROCESSED, request_json: JSON.stringify({ message: MESSAGE }), response_text: "Deployed." }),
};
const UNAVAILABLE = { ok: false, error: "unavailable", status: 503 };

setupRegistryTests();
beforeEach(() => {
  resetFleetEventAction();
  setEventDetailReader(fleetActionsMock().getFleetEventAction as EventDetailReader);
});

function shown(eventId: string) {
  return getSnapshot(Z_A).events.find((event) => event.id === eventId);
}

// A send answered as a replay of an event this page never held.
function replayed(eventId: string): void {
  const tempId = appendOptimistic(Z_A, MESSAGE, ACTOR);
  reconcileOptimistic(Z_A, tempId, eventId, true);
}

// The stream stays up for `ms`, saying nothing but keepalives.
async function keepaliveFor(ms: number): Promise<void> {
  for (let passed = 0; passed < ms; passed += KEEPALIVE_MS) {
    await vi.advanceTimersByTimeAsync(KEEPALIVE_MS);
    sourceAt(0).heartbeat();
  }
  await vi.advanceTimersByTimeAsync(0);
}

describe("a replayed send", () => {
  it("test_replayed_answer_settles_from_detail", async () => {
    getFleetEventActionMock.mockResolvedValueOnce(SAVED);
    const release = subscribe(WS, Z_A, [], () => {});
    replayed(RAN);
    await vi.advanceTimersByTimeAsync(0);
    expect(getFleetEventActionMock).toHaveBeenCalledExactlyOnceWith(WS, Z_A, RAN);
    expect(getSnapshot(Z_A).events).toHaveLength(1);
    expect(shown(RAN)).toMatchObject({ status: PROCESSED, reply: "Deployed.", text: MESSAGE });
    // Settled, so the sweep reads nothing more.
    await keepaliveFor(REPLY_STALL_MS);
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(1);
    release();
  });

  it("keeps its row when the read fails, and the stall read retries it once", async () => {
    getFleetEventActionMock.mockResolvedValueOnce(UNAVAILABLE).mockResolvedValueOnce(SAVED);
    const release = subscribe(WS, Z_A, [], () => {});
    replayed(RAN);
    await vi.advanceTimersByTimeAsync(0);
    expect(shown(RAN)?.status).toBe(RECEIVED);

    await keepaliveFor(REPLY_STALL_MS);
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(2);
    expect(shown(RAN)?.status).toBe(PROCESSED);
    release();
  });

  it("keeps its row when the read throws", async () => {
    failedAction.enabled = true;
    const release = subscribe(WS, Z_A, [], () => {});
    replayed(RAN);
    await vi.advanceTimersByTimeAsync(0);
    expect(failedAction.calls).toBe(1);
    expect(shown(RAN)?.status).toBe(RECEIVED);
    release();
  });

  it("waits for frames when its event is loaded and still running", async () => {
    const release = subscribe(WS, Z_A, [], () => {});
    sourceAt(0).emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: RAN, actor: ACTOR });
    replayed(RAN);
    await vi.advanceTimersByTimeAsync(0);
    expect(getFleetEventActionMock).not.toHaveBeenCalled();
    release();
  });

  it("reads nothing without a reader, and applies nothing once its fleet is released", async () => {
    setEventDetailReader(null);
    const release = subscribe(WS, Z_A, [], () => {});
    replayed(RAN);
    await keepaliveFor(REPLY_STALL_MS);
    expect(shown(RAN)?.status).toBe(RECEIVED);

    setEventDetailReader(fleetActionsMock().getFleetEventAction as EventDetailReader);
    let answer: (value: typeof SAVED) => void = () => undefined;
    getFleetEventActionMock.mockReturnValueOnce(new Promise((resolve) => (answer = resolve)));
    replayed("evt_late");
    release();
    answer(SAVED);
    await vi.advanceTimersByTimeAsync(0);
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(1);
  });
});

describe("a running reply that stops hearing frames", () => {
  it("test_lost_completion_stops_the_thought_clock", async () => {
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: THINKING, actor: ACTOR });
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: THINKING, text: "Checking", text_kind: "reasoning", stream_start: true, stream_contiguous: true, stream_seq: 0 });
    await vi.advanceTimersByTimeAsync(0);
    expect(shown(THINKING)?.thinking).toBe(true);
    expect(shown(THINKING)?.reasoningEndedAtMs).toBeUndefined();

    // The completion never arrives; the stream stays up.
    getFleetEventActionMock.mockResolvedValueOnce({ ok: true, data: row({ event_id: THINKING, status: PROCESSED, response_text: "Done." }) });
    await keepaliveFor(REPLY_STALL_MS);
    const settled = shown(THINKING);
    expect(settled).toMatchObject({ status: PROCESSED, thinking: false, reply: "Done." });
    expect(settled?.reasoningEndedAtMs).toBeGreaterThanOrEqual(settled?.reasoningStartedAtMs ?? Infinity);
    release();
  });

  it("is heard again on every frame, read once per silence, and left as it is while it runs", async () => {
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: THINKING, status: RECEIVED, response_text: null }) });
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: THINKING, actor: ACTOR });
    await keepaliveFor(REPLY_STALL_MS - KEEPALIVE_MS);
    source.emit({ kind: FRAME_KIND.TOOL_CALL_PROGRESS, event_id: THINKING, name: "deploy", elapsed_ms: 1 });
    await keepaliveFor(REPLY_STALL_MS - KEEPALIVE_MS);
    expect(getFleetEventActionMock).not.toHaveBeenCalled();

    await keepaliveFor(REPLY_STALL_MS * 3);
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(1);
    expect(shown(THINKING)?.status).toBe(RECEIVED);
    release();
  });

  it("forgets an event its completion frame ended", async () => {
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: THINKING, actor: ACTOR });
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: THINKING, status: PROCESSED, final_reply: "Done." });
    await keepaliveFor(REPLY_STALL_MS);
    expect(getFleetEventActionMock).not.toHaveBeenCalled();
    release();
  });
});
