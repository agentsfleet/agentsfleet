import { afterEach, describe, expect, it, vi } from "vitest";
import { createEntry } from "./fleet-stream-entry";
import { recoverGap, type LiveEntry } from "./fleet-stream-dispatch";
import { recoverRunningReplies, setTurnDetailReader, watchReply, type TurnDetailReader } from "./fleet-stream-reply-registry";
import { row, WS, Z_A } from "@/tests/helpers/fleet-stream-registry-fixtures";
import type { TaskActivityHistory } from "../task-activity";

const TURN = "running-progress";
const progress: TaskActivityHistory = {items: [{kind: "task_transfer_progress", progress: {
  activity_id: "019f0000-0000-7000-8000-000000000001", task_id: null, source_turn_id: null,
  operation: "capture", phase: "uploading", provider: null, model: null, completed: 3, total: 10,
}}], truncated: false};

function entry(): LiveEntry {
  const current = createEntry(WS, [row({turn_id: TURN, status: "received"})]);
  return Object.assign(current, {isCurrent: () => true,
    apply: (update: Parameters<LiveEntry["apply"]>[0]) => { current.snapshot.turns = update(current.snapshot.turns); }});
}

afterEach(() => { setTurnDetailReader(null); vi.unstubAllGlobals(); });

describe("running progress readback", () => {
  it("recovers a turn older than an empty gap list while live traffic continues", async () => {
    const current = entry();
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ok: true, json: async () => ({items: [], next_cursor: null})}));
    const read = vi.fn<TurnDetailReader>().mockResolvedValue({ok: true,
      data: row({turn_id: TURN, status: "received", task_activity: progress})});
    setTurnDetailReader(read);
    current.replyHeard.set(TURN, Date.now());
    recoverGap(current, Z_A);
    await vi.waitFor(() => expect(current.backfillInFlight).toBe(false));
    expect(read).toHaveBeenCalledExactlyOnceWith(WS, Z_A, TURN);
    expect(current.snapshot.turns[0]?.taskActivity).toEqual(progress);
    expect(current.snapshot.turns[0]?.status).toBe("received");
  });

  it("waits for an older in-flight read before recovering a newer gap", async () => {
    const current = entry();
    let answer!: (result: Awaited<ReturnType<TurnDetailReader>>) => void;
    const read = vi.fn<TurnDetailReader>()
      .mockReturnValueOnce(new Promise(resolve => { answer = resolve; }))
      .mockResolvedValue({ok: true, data: row({turn_id: TURN, status: "received", task_activity: progress})});
    setTurnDetailReader(read);
    watchReply(current, Z_A, TURN, current.apply, current.isCurrent, true);
    watchReply(current, Z_A, TURN, current.apply, current.isCurrent, true);
    const recovery = recoverRunningReplies(current, Z_A, current.apply, current.isCurrent);
    expect(read).toHaveBeenCalledTimes(1);
    answer({ok: true, data: row({turn_id: TURN, status: "received"})});
    await recovery;
    expect(read).toHaveBeenCalledTimes(2);
    expect(current.snapshot.turns[0]?.taskActivity).toEqual(progress);
    expect(current.runningReads.size).toBe(0);
  });

  it("rechecks ownership after waiting for an older detail read", async () => {
    const current = entry();
    let answer!: (result: Awaited<ReturnType<TurnDetailReader>>) => void;
    const read = vi.fn<TurnDetailReader>().mockReturnValue(new Promise(resolve => { answer = resolve; }));
    setTurnDetailReader(read);
    let owned = true;
    const isCurrent = () => owned;
    watchReply(current, Z_A, TURN, current.apply, isCurrent, true);
    const recovery = recoverRunningReplies(current, Z_A, current.apply, isCurrent);
    owned = false;
    answer({ok: true, data: row({turn_id: TURN, status: "received", task_activity: progress})});
    await recovery;
    expect(read).toHaveBeenCalledTimes(1);
    expect(current.snapshot.turns[0]?.taskActivity).toBeUndefined();
    expect(current.runningReads.size).toBe(0);
  });

  it("does not read or write for an entry whose subscriber is gone", async () => {
    const current = entry();
    const read = vi.fn<TurnDetailReader>();
    setTurnDetailReader(read);
    await recoverRunningReplies(current, Z_A, current.apply, () => false);
    expect(read).not.toHaveBeenCalled();
    expect(current.snapshot.turns[0]?.taskActivity).toBeUndefined();
  });
});
