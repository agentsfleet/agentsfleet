import { afterEach, describe, expect, it, vi } from "vitest";
import { createEntry } from "./fleet-stream-entry";
import { recoverGap, type LiveEntry } from "./fleet-stream-dispatch";
import { recoverRunningReplies, setEventDetailReader, watchReply, type EventDetailReader } from "./fleet-stream-reply-registry";
import { row, WS, Z_A } from "@/tests/helpers/fleet-stream-registry-fixtures";
import type { TaskActivityHistory } from "../task-activity";

const EVENT = "running-progress";
const progress: TaskActivityHistory = {items: [{kind: "task_transfer_progress", progress: {
  activity_id: "019f0000-0000-7000-8000-000000000001", task_id: null, source_admission_id: null,
  operation: "capture", phase: "uploading", provider: null, model: null, completed: 3, total: 10,
}}], truncated: false};

function entry(): LiveEntry {
  const current = createEntry(WS, [row({event_id: EVENT, status: "received"})]);
  return Object.assign(current, {isCurrent: () => true,
    apply: (update: Parameters<LiveEntry["apply"]>[0]) => { current.snapshot.events = update(current.snapshot.events); }});
}

afterEach(() => { setEventDetailReader(null); vi.unstubAllGlobals(); });

describe("running progress readback", () => {
  it("recovers an event older than an empty gap list while live traffic continues", async () => {
    const current = entry();
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ok: true, json: async () => ({items: [], next_cursor: null})}));
    const read = vi.fn<EventDetailReader>().mockResolvedValue({ok: true,
      data: row({event_id: EVENT, status: "received", task_activity: progress})});
    setEventDetailReader(read);
    current.replyHeard.set(EVENT, Date.now());
    recoverGap(current, Z_A);
    await vi.waitFor(() => expect(current.backfillInFlight).toBe(false));
    expect(read).toHaveBeenCalledExactlyOnceWith(WS, Z_A, EVENT);
    expect(current.snapshot.events[0]?.taskActivity).toEqual(progress);
    expect(current.snapshot.events[0]?.status).toBe("received");
  });

  it("waits for an older in-flight read before recovering a newer gap", async () => {
    const current = entry();
    let answer!: (result: Awaited<ReturnType<EventDetailReader>>) => void;
    const read = vi.fn<EventDetailReader>()
      .mockReturnValueOnce(new Promise(resolve => { answer = resolve; }))
      .mockResolvedValue({ok: true, data: row({event_id: EVENT, status: "received", task_activity: progress})});
    setEventDetailReader(read);
    watchReply(current, Z_A, EVENT, current.apply, current.isCurrent, true);
    watchReply(current, Z_A, EVENT, current.apply, current.isCurrent, true);
    const recovery = recoverRunningReplies(current, Z_A, current.apply, current.isCurrent);
    expect(read).toHaveBeenCalledTimes(1);
    answer({ok: true, data: row({event_id: EVENT, status: "received"})});
    await recovery;
    expect(read).toHaveBeenCalledTimes(2);
    expect(current.snapshot.events[0]?.taskActivity).toEqual(progress);
    expect(current.runningReads.size).toBe(0);
  });

  it("rechecks ownership after waiting for an older detail read", async () => {
    const current = entry();
    let answer!: (result: Awaited<ReturnType<EventDetailReader>>) => void;
    const read = vi.fn<EventDetailReader>().mockReturnValue(new Promise(resolve => { answer = resolve; }));
    setEventDetailReader(read);
    let owned = true;
    const isCurrent = () => owned;
    watchReply(current, Z_A, EVENT, current.apply, isCurrent, true);
    const recovery = recoverRunningReplies(current, Z_A, current.apply, isCurrent);
    owned = false;
    answer({ok: true, data: row({event_id: EVENT, status: "received", task_activity: progress})});
    await recovery;
    expect(read).toHaveBeenCalledTimes(1);
    expect(current.snapshot.events[0]?.taskActivity).toBeUndefined();
    expect(current.runningReads.size).toBe(0);
  });

  it("does not read or write for an entry whose subscriber is gone", async () => {
    const current = entry();
    const read = vi.fn<EventDetailReader>();
    setEventDetailReader(read);
    await recoverRunningReplies(current, Z_A, current.apply, () => false);
    expect(read).not.toHaveBeenCalled();
    expect(current.snapshot.events[0]?.taskActivity).toBeUndefined();
  });
});
