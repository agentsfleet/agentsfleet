import { beforeEach, describe, expect, it, vi } from "vitest";
import { FRAME_KIND } from "@/lib/api/events-types";
import { OUTCOME } from "@/lib/events/event-summary";
import { appendOptimistic, getSnapshot, reconcileOptimistic, reconcileServerRows, subscribe } from "./fleet-stream-registry";
import { REPLY_STALL_MS, setEventDetailReader, type EventDetailReader } from "./fleet-stream-reply-registry";
import { AGENTSFLEET_EVENT_STATUS } from "./fleet-stream-row";
import { IDLE_RELEASE_MS, setupRegistryTests, row, sourceAt, WS, Z_A } from "@/tests/helpers/fleet-stream-registry-fixtures";
import { failedAction, fleetActionsMock, getFleetEventActionMock, resetFleetEventAction } from "@/tests/helpers/fleet-stream-reply-action-mock";

// Every running event's row reaches an ending the operator can see, even when
// the frame that would have ended it never arrives.

// Every write to any entry's rows, counted: a torn-down entry has no listener
// left to notice one, so this is the only witness to a stale write.
const rowWrites = vi.hoisted(() => ({ count: 0 }));
vi.mock("./fleet-stream-snapshot", async (importActual) => {
  const actual = await importActual<typeof import("./fleet-stream-snapshot")>();
  return {
    ...actual,
    setEvents: (...args: Parameters<typeof actual.setEvents>) => {
      rowWrites.count += 1;
      actual.setEvents(...args);
    },
  };
});

const { AGENT_ERROR, PROCESSED, RECEIVED } = AGENTSFLEET_EVENT_STATUS;
const RAN = "evt_ran";
const THINKING = "evt_thinking";
const LATE = "evt_late";
const DEPLOYED = "Deployed.";
const DONE = "Done.";
const DEPLOY_TOOL = "deploy";
const ACTOR = "steer:pending";
const MESSAGE = "deploy";
// The server's keepalive cadence while nothing else is said.
const KEEPALIVE_MS = 15_000;
const SAVED = {
  ok: true,
  data: row({ event_id: RAN, actor: ACTOR, status: PROCESSED, request_json: JSON.stringify({ message: MESSAGE }), response_text: DEPLOYED }),
};
const UNAVAILABLE = { ok: false, error: "unavailable", status: 503 };
const NOT_FOUND = { ok: false, error: "not found", status: 404 };
const GONE = { ok: false, error: "gone", status: 410 };

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
    expect(shown(RAN)).toMatchObject({ status: PROCESSED, reply: DEPLOYED, text: MESSAGE });
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

  it("test_replayed_answer_settles_from_detail: a first read of 404 is an event still queued, and its row waits for it", async () => {
    getFleetEventActionMock.mockResolvedValueOnce(NOT_FOUND).mockResolvedValueOnce(SAVED);
    const release = subscribe(WS, Z_A, [], () => {});
    replayed(RAN);
    await vi.advanceTimersByTimeAsync(0);
    // No runner has written its row yet: nothing settles it as gone.
    expect(shown(RAN)).toMatchObject({ status: RECEIVED, clientTimestamp: true });
    expect(shown(RAN)?.outcome).not.toBe(OUTCOME.REPLY_GONE);
    // Read again one silence window later, and settled by the row once it exists.
    await keepaliveFor(REPLY_STALL_MS);
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(2);
    expect(shown(RAN)).toMatchObject({ status: PROCESSED, reply: DEPLOYED });
    release();
  });

  it("a queued replay whose first read was 404 still settles from its frames", async () => {
    getFleetEventActionMock.mockResolvedValue(NOT_FOUND);
    const release = subscribe(WS, Z_A, [], () => {});
    replayed(RAN);
    await vi.advanceTimersByTimeAsync(0);
    sourceAt(0).emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: RAN, actor: ACTOR });
    sourceAt(0).emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: RAN, status: PROCESSED, final_reply: DEPLOYED });
    await vi.advanceTimersByTimeAsync(0);
    expect(shown(RAN)).toMatchObject({ status: PROCESSED, reply: DEPLOYED });
    await keepaliveFor(REPLY_STALL_MS);
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(1);
    release();
  });

  it("an event a read once found running, then 404, settles as gone", async () => {
    getFleetEventActionMock
      .mockResolvedValueOnce({ ok: true, data: row({ event_id: RAN, actor: ACTOR, status: RECEIVED, response_text: null }) })
      .mockResolvedValueOnce(NOT_FOUND);
    const release = subscribe(WS, Z_A, [], () => {});
    replayed(RAN);
    await keepaliveFor(REPLY_STALL_MS);
    expect(shown(RAN)).toMatchObject({ status: AGENT_ERROR, outcome: OUTCOME.REPLY_GONE });
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

  it("test_replayed_answer_settles_from_detail: reads nothing for a replay whose settled event the page already holds", async () => {
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: RAN, actor: ACTOR });
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: RAN, status: PROCESSED, final_reply: DEPLOYED });
    await vi.advanceTimersByTimeAsync(0);
    const settled = shown(RAN);
    expect(settled).toMatchObject({ status: PROCESSED, reply: DEPLOYED });

    replayed(RAN);
    // Neither at once nor after a silence: the row it holds is already final.
    await keepaliveFor(REPLY_STALL_MS);
    expect(getFleetEventActionMock).not.toHaveBeenCalled();
    expect(getSnapshot(Z_A).events).toHaveLength(1);
    expect(shown(RAN)).toMatchObject({ status: PROCESSED, reply: DEPLOYED, outcome: settled?.outcome });
    release();
  });

  it("reads nothing without a reader", async () => {
    setEventDetailReader(null);
    const release = subscribe(WS, Z_A, [], () => {});
    replayed(RAN);
    await keepaliveFor(REPLY_STALL_MS * 2);
    expect(shown(RAN)?.status).toBe(RECEIVED);
    expect(getFleetEventActionMock).not.toHaveBeenCalled();
    release();
  });

  it("writes nothing from a read that answers after its fleet was torn down", async () => {
    let answer: (value: unknown) => void = () => undefined;
    getFleetEventActionMock.mockReturnValueOnce(new Promise((resolve) => (answer = resolve)));
    const release = subscribe(WS, Z_A, [], () => {});
    replayed(LATE);
    expect(getFleetEventActionMock).toHaveBeenCalledExactlyOnceWith(WS, Z_A, LATE);
    // The send's own writes prove the witness counts.
    expect(rowWrites.count).toBeGreaterThan(0);

    release();
    await vi.advanceTimersByTimeAsync(IDLE_RELEASE_MS);
    expect(getSnapshot(Z_A).events).toHaveLength(0);

    rowWrites.count = 0;
    answer({ ok: true, data: row({ event_id: LATE, actor: ACTOR, status: PROCESSED, response_text: "Too late." }) });
    await vi.advanceTimersByTimeAsync(0);
    expect(rowWrites.count).toBe(0);
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
    getFleetEventActionMock.mockResolvedValueOnce({ ok: true, data: row({ event_id: THINKING, status: PROCESSED, response_text: DONE }) });
    await keepaliveFor(REPLY_STALL_MS);
    const settled = shown(THINKING);
    expect(settled).toMatchObject({ status: PROCESSED, thinking: false, reply: DONE });
    expect(settled?.reasoningEndedAtMs).toBeGreaterThanOrEqual(settled?.reasoningStartedAtMs ?? Infinity);
    release();
  });

  it("is heard again on every frame, and read once per silence window while it runs", async () => {
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: row({ event_id: THINKING, status: RECEIVED, response_text: null }) });
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: THINKING, actor: ACTOR });
    await keepaliveFor(REPLY_STALL_MS - KEEPALIVE_MS);
    source.emit({ kind: FRAME_KIND.TOOL_CALL_PROGRESS, event_id: THINKING, name: DEPLOY_TOOL, elapsed_ms: 1 });
    await keepaliveFor(REPLY_STALL_MS - KEEPALIVE_MS);
    expect(getFleetEventActionMock).not.toHaveBeenCalled();

    // Still running each time it is read, so it is read again one window
    // later: three windows of silence, three reads, never a tight loop.
    await keepaliveFor(REPLY_STALL_MS * 3);
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(3);
    expect(shown(THINKING)?.status).toBe(RECEIVED);
    release();
  });

  it("test_stall_read_retries_each_silence_window: reads a lost completion again after a failed read, one silence window later", async () => {
    getFleetEventActionMock
      .mockResolvedValueOnce(UNAVAILABLE)
      .mockResolvedValueOnce({ ok: true, data: row({ event_id: THINKING, status: PROCESSED, response_text: DONE }) });
    const release = subscribe(WS, Z_A, [], () => {});
    sourceAt(0).emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: THINKING, actor: ACTOR });

    await keepaliveFor(REPLY_STALL_MS);
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(1);
    expect(shown(THINKING)?.status).toBe(RECEIVED);

    await keepaliveFor(REPLY_STALL_MS);
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(2);
    expect(shown(THINKING)).toMatchObject({ status: PROCESSED, reply: DONE });

    // Settled, so the watch is gone.
    await keepaliveFor(REPLY_STALL_MS);
    expect(getFleetEventActionMock).toHaveBeenCalledTimes(2);
    release();
  });

  it.each([NOT_FOUND, GONE])("test_stall_read_retries_each_silence_window: stops reading an event its read found gone ($status), and settles its row", async (gone) => {
    getFleetEventActionMock.mockResolvedValue(gone);
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: THINKING, actor: ACTOR });
    source.emit({ kind: FRAME_KIND.CHUNK, event_id: THINKING, text: "Checking", text_kind: "reasoning", stream_start: true, stream_contiguous: true, stream_seq: 0 });
    // Final: three windows of silence, one read.
    await keepaliveFor(REPLY_STALL_MS * 3);
    expect(getFleetEventActionMock).toHaveBeenCalledExactlyOnceWith(WS, Z_A, THINKING);
    // Nothing is left to wait for: the wait and the Thought clock both end.
    const settled = shown(THINKING);
    expect(settled).toMatchObject({ status: AGENT_ERROR, reply: "", thinking: false, replyRecovering: false, outcome: OUTCOME.REPLY_GONE });
    expect(settled?.reasoningEndedAtMs).toBeDefined();
    release();
  });

  it("test_frame_ending_silence_reads_nothing: does not read the event whose frame ends a long silence", async () => {
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: THINKING, actor: ACTOR });
    // The stream stays up without a word about this event for a whole window,
    // and the next thing it says is about this event.
    await vi.advanceTimersByTimeAsync(KEEPALIVE_MS);
    source.heartbeat();
    await vi.advanceTimersByTimeAsync(KEEPALIVE_MS);
    source.heartbeat();
    await vi.advanceTimersByTimeAsync(KEEPALIVE_MS);
    source.emit({ kind: FRAME_KIND.TOOL_CALL_PROGRESS, event_id: THINKING, name: DEPLOY_TOOL, elapsed_ms: 1 });
    await vi.advanceTimersByTimeAsync(0);
    expect(getFleetEventActionMock).not.toHaveBeenCalled();
    release();
  });

  it("forgets an event its completion frame ended", async () => {
    const release = subscribe(WS, Z_A, [], () => {});
    const source = sourceAt(0);
    source.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: THINKING, actor: ACTOR });
    source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: THINKING, status: PROCESSED, final_reply: DONE });
    await keepaliveFor(REPLY_STALL_MS);
    expect(getFleetEventActionMock).not.toHaveBeenCalled();
    release();
  });
});

// The first open reads no backfill, so a completion published between the
// server render and the subscribe reaches this tab only through a read.
describe("a row the server render shows running", () => {
  const RUNNING = row({ event_id: THINKING, actor: ACTOR, status: RECEIVED, response_text: null });
  const ENDED = { ok: true, data: row({ event_id: THINKING, actor: ACTOR, status: PROCESSED, response_text: DONE }) };

  it("test_initial_running_row_settles_after_the_stall_window", async () => {
    getFleetEventActionMock.mockResolvedValueOnce(ENDED);
    const release = subscribe(WS, Z_A, [RUNNING], () => {});
    sourceAt(0).open();
    // No frame about it ever arrives; nothing is read before the window ends.
    await keepaliveFor(REPLY_STALL_MS - KEEPALIVE_MS);
    expect(getFleetEventActionMock).not.toHaveBeenCalled();

    await keepaliveFor(KEEPALIVE_MS);
    expect(getFleetEventActionMock).toHaveBeenCalledExactlyOnceWith(WS, Z_A, THINKING);
    expect(shown(THINKING)).toMatchObject({ status: PROCESSED, reply: DONE });
    release();
  });

  it("watches a running row a later server render brings in, and keeps an earlier row's stamp", async () => {
    getFleetEventActionMock.mockResolvedValue(ENDED);
    const release = subscribe(WS, Z_A, [RUNNING], () => {});
    await keepaliveFor(REPLY_STALL_MS - KEEPALIVE_MS);
    // A re-render still shows it running: its stall read is not pushed back.
    reconcileServerRows(Z_A, [RUNNING, row({ event_id: LATE, status: RECEIVED, response_text: null })]);
    await keepaliveFor(KEEPALIVE_MS);
    expect(getFleetEventActionMock).toHaveBeenCalledExactlyOnceWith(WS, Z_A, THINKING);

    // The row the re-render brought in is read one window after it arrived.
    await keepaliveFor(REPLY_STALL_MS - KEEPALIVE_MS);
    expect(getFleetEventActionMock).toHaveBeenLastCalledWith(WS, Z_A, LATE);
    release();
  });

  it("watches nothing for a server row already ended", async () => {
    const release = subscribe(WS, Z_A, [row({ event_id: RAN, status: PROCESSED })], () => {});
    await keepaliveFor(REPLY_STALL_MS * 2);
    expect(getFleetEventActionMock).not.toHaveBeenCalled();
    release();
  });
});
