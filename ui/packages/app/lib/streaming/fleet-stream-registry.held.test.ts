import { describe, expect, it } from "vitest";
import { FRAME_KIND } from "@/lib/api/events-types";
import { reportsOwnRun } from "@/components/domain/fleetReplyMessage";
import { appendOptimistic, discardOptimistic, getSnapshot, reconcileOptimistic, subscribe } from "./fleet-stream-registry";
import { NO_SEED, setupRegistryTests, sourceAt, WS, Z_A } from "@/tests/helpers/fleet-stream-registry-fixtures";

// A steer's opening frame names the account, never the tab. One that lands
// while this tab's send awaits its 202 waits until the 202 says whose it is,
// so another tab's turn never borrows this tab's run.

setupRegistryTests();

const SUBJECT = "k@e2e.com";
const ACTOR = `steer:${SUBJECT}`;
const MINE = "evt_mine";
const ELSEWHERE = "evt_elsewhere";

function announce(eventId: string): void {
  sourceAt(0).emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: eventId, actor: ACTOR });
}

describe("fleet-stream-registry — turns held on a send's 202", () => {
  it("lands this tab's own turn on its row once the 202 names it", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "deploy the canary", "steer:pending");
    const stamped = getSnapshot(Z_A).events[0]?.submittedAtMs;
    announce(MINE);
    sourceAt(0).emit({ kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: MINE, name: "read_file", args_redacted: true });
    // Held: the thread still shows the one optimistic row, and nothing else.
    expect(getSnapshot(Z_A).events.map((event) => event.id)).toEqual([tempId]);

    reconcileOptimistic(Z_A, tempId, MINE, false);
    const events = getSnapshot(Z_A).events;
    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({ id: MINE, text: "deploy the canary", tools: [{ name: "read_file" }], submittedAtMs: stamped });
    expect(reportsOwnRun(events, SUBJECT)).toBe(true);
    release();
  });

  it("lands another tab's turn as its own row, which never runs this thread", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "deploy the canary", "steer:pending");
    announce(ELSEWHERE);
    expect(getSnapshot(Z_A).events.map((event) => event.id)).toEqual([tempId]);

    reconcileOptimistic(Z_A, tempId, MINE, false);
    const events = getSnapshot(Z_A).events;
    expect(events.map((event) => event.id)).toEqual([MINE, ELSEWHERE]);
    expect(events[1]?.submittedAtMs).toBeUndefined();
    expect(reportsOwnRun(events, SUBJECT)).toBe(false);
    release();
  });

  it("lands a held turn when the waiting send is discarded", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "refused", "steer:pending");
    announce(ELSEWHERE);
    discardOptimistic(Z_A, tempId);
    expect(getSnapshot(Z_A).events.map((event) => event.id)).toEqual([ELSEWHERE]);
    release();
  });
});
