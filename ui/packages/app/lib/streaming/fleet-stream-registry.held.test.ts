import { describe, expect, it } from "vitest";
import { FRAME_KIND } from "@/lib/api/events-types";
import { reportsOwnRun } from "@/components/domain/fleetReplyMessage";
import { appendOptimistic, discardOptimistic, getSnapshot, reconcileOptimistic, subscribe } from "./fleet-stream-registry";
import { NO_SEED, row, setupRegistryTests, sourceAt, WS, Z_A } from "@/tests/helpers/fleet-stream-registry-fixtures";
import { fetchSpy, flushBackfill, pageWith, setupBackfillTests } from "@/tests/helpers/fleet-stream-backfill-fixtures";

// A steer's opening frame names the account, never the tab. One under this
// tab's own account that lands while its send awaits its 202 waits until the
// 202 says whose it is, so another tab's turn never borrows this tab's run.

setupRegistryTests();
setupBackfillTests();

const SUBJECT = "k@e2e.com";
const OWN = `steer:${SUBJECT}`;
const TEAMMATE = "steer:teammate@e2e.com";
const PENDING = "steer:pending";
const MINE = "evt_mine";
const ELSEWHERE = "evt_elsewhere";

function announce(eventId: string, actor = OWN): void {
  sourceAt(0).emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: eventId, actor });
}

function toolStarted(eventId: string): void {
  sourceAt(0).emit({ kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: eventId, name: "read_file", args_redacted: true });
}

function ids(): string[] {
  return getSnapshot(Z_A).events.map((event) => event.id);
}

describe("fleet-stream-registry — turns held on a send's 202", () => {
  it("lands this tab's own turn on its row once the 202 names it", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "deploy the canary", PENDING, OWN);
    const stamped = getSnapshot(Z_A).events[0]?.submittedAtMs;
    announce(MINE);
    toolStarted(MINE);
    // Held: the thread still shows the one optimistic row, and nothing else.
    expect(ids()).toEqual([tempId]);

    reconcileOptimistic(Z_A, tempId, MINE, false);
    const events = getSnapshot(Z_A).events;
    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({ id: MINE, text: "deploy the canary", tools: [{ name: "read_file" }], submittedAtMs: stamped });
    expect(reportsOwnRun(events, SUBJECT)).toBe(true);
    release();
  });

  it("lands another tab's turn as its own row, which never runs this thread", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "deploy the canary", PENDING, OWN);
    announce(ELSEWHERE);
    expect(ids()).toEqual([tempId]);

    reconcileOptimistic(Z_A, tempId, MINE, false);
    const events = getSnapshot(Z_A).events;
    expect(events.map((event) => event.id)).toEqual([MINE, ELSEWHERE]);
    expect(events[1]?.submittedAtMs).toBeUndefined();
    expect(reportsOwnRun(events, SUBJECT)).toBe(false);
    release();
  });

  it("never holds a teammate's turn, however long the 202 takes", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "deploy the canary", PENDING, OWN);
    announce(ELSEWHERE, TEAMMATE);
    toolStarted(ELSEWHERE);
    expect(ids()).toEqual([tempId, ELSEWHERE]);
    expect(getSnapshot(Z_A).events[1]?.tools).toEqual([expect.objectContaining({ name: "read_file" })]);
    release();
  });

  it("lands a held turn when the waiting send is discarded", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "refused", PENDING, OWN);
    announce(ELSEWHERE);
    discardOptimistic(Z_A, tempId);
    expect(ids()).toEqual([ELSEWHERE]);
    release();
  });

  it("lands a held turn's activity before a backfill settles it", async () => {
    // The row the gap walk reads back finished while its frames were held; the
    // list carries no tools, so only the held frames can put them on the row.
    fetchSpy.mockResolvedValue(pageWith([row({ event_id: ELSEWHERE, actor: OWN, status: "processed", response_text: "done" })]));
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    const es = sourceAt(0);
    es.open();
    es.heartbeat();
    appendOptimistic(Z_A, "deploy the canary", PENDING, OWN);
    announce(ELSEWHERE);
    toolStarted(ELSEWHERE);
    es.emit({ kind: FRAME_KIND.CATCHING_UP, dropped: 0 });
    await flushBackfill();

    const settled = getSnapshot(Z_A).events.find((event) => event.id === ELSEWHERE);
    expect(settled).toMatchObject({ status: "processed", tools: [expect.objectContaining({ name: "read_file" })] });
    release();
  });
});
