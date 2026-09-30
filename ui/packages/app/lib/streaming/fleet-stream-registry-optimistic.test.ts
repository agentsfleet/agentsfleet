import { describe, expect, it, vi } from "vitest";
import { appendOptimistic, discardOptimistic, getSnapshot, reconcileOptimistic, reconcileServerRows, subscribe } from "./fleet-stream-registry";
import { FRAME_KIND } from "@/lib/api/events-types";
import { setupRegistryTests, row, WS, Z_A, NO_SEED, IDLE_RELEASE_MS, sourceAt } from "@/tests/helpers/fleet-stream-registry-fixtures";
import { optimisticRow, reconcileRows } from "./fleet-stream-optimistic";

setupRegistryTests();

describe("fleet-stream-registry — optimistic mutations", () => {
  it("keeps a server trigger while carrying local submit timing through a raced reconciliation", () => {
    const temp = optimisticRow("temp", "local trigger", "steer:operator");
    const server = { ...temp, id: "real", text: "server trigger", status: "received" as const };
    const reconciled = reconcileRows([temp, server], "temp", "real");
    expect(reconciled.events).toHaveLength(1);
    expect(reconciled.events[0]).toMatchObject({ text: "server trigger", submittedAtMs: temp.submittedAtMs });
  });

  it("leaves an already present server row alone when the optimistic row has gone", () => {
    const server = { ...optimisticRow("real", "server trigger", "steer:operator"), status: "received" as const };
    const reconciled = reconcileRows([server], "missing", "real");
    expect(reconciled.events).toEqual([server]);
  });
  it("appendOptimistic adds a 'optimistic' row and returns a tempId", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "deploy canary", "steer:k@e2e.com");
    expect(tempId).toMatch(/^optim-/);
    const snap = getSnapshot(Z_A);
    expect(snap.events).toHaveLength(1);
    expect(snap.events[0]?.id).toBe(tempId);
    expect(snap.events[0]?.status).toBe("optimistic");
    expect(snap.events[0]?.text).toBe("deploy canary");
    a();
  });

  it("reconcileOptimistic swaps tempId for the real event_id and clears optimistic", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "x", "steer:k@e2e.com");
    expect(reconcileOptimistic(Z_A, tempId, "evt_real", false)).toBe(false);
    const snap = getSnapshot(Z_A);
    expect(snap.events).toHaveLength(1);
    expect(snap.events[0]?.id).toBe("evt_real");
    expect(snap.events[0]?.status).toBe("received");
    a();
  });

  // The thread reports a run only for a row this tab sent (`reportsOwnRun`),
  // and the submit clock is how it knows: the 202 and every frame after it
  // must carry the clock the optimistic paint stamped.
  it("carries the local submit clock through the 202 and the live frames", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "sent from this tab", "steer:k@e2e.com");
    const stamped = getSnapshot(Z_A).events[0]?.submittedAtMs;
    expect(stamped).toBeTypeOf("number");

    reconcileOptimistic(Z_A, tempId, "evt_mine", false);
    expect(getSnapshot(Z_A).events[0]?.submittedAtMs).toBe(stamped);
    sourceAt(0).emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "evt_mine", actor: "steer:k@e2e.com", created_at: Date.UTC(2026, 0, 1) });
    // A frame that lands on the row synchronously, so nothing here waits.
    sourceAt(0).emit({ kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: "evt_mine", name: "read_file", args_redacted: true });
    expect(getSnapshot(Z_A).events).toHaveLength(1);
    expect(getSnapshot(Z_A).events[0]).toMatchObject({ id: "evt_mine", tools: [{ name: "read_file" }], submittedAtMs: stamped });
    release();
  });

  it("keeps an acknowledged steer visible through backfill until its server timestamp arrives", () => {
    const clientInstant = Date.UTC(2026, 0, 1);
    vi.setSystemTime(clientInstant);
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "keep this turn visible", "steer:k@e2e.com");
    reconcileOptimistic(Z_A, tempId, "evt_ack", false);
    const newerRow = row({ event_id: "evt_other", status: "processed", created_at: Date.UTC(2026, 4, 15) });

    reconcileServerRows(Z_A, [newerRow]);
    expect(getSnapshot(Z_A).events.at(-1)?.id).toBe("evt_ack");
    expect(getSnapshot(Z_A).events.at(-1)?.clientTimestamp).toBe(true);

    sourceAt(0).emit({
      kind: FRAME_KIND.EVENT_RECEIVED,
      event_id: "evt_ack",
      actor: "steer:k@e2e.com",
      created_at: clientInstant,
    });
    reconcileServerRows(Z_A, [newerRow]);
    expect(getSnapshot(Z_A).events.map((event) => event.id)).toEqual(["evt_ack", "evt_other"]);
    expect(getSnapshot(Z_A).events[0]?.clientTimestamp).toBe(false);
    release();
  });

  it("grafts the operator's text onto a body-less live row that beat the POST response", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "deploy the canary", "steer:k@e2e.com");
    // The opening frame for this steer lands before its 202. It names only
    // the account, so it waits (`HeldTurns`) until the 202 names its row,
    // then lands on the row that holds the operator's text.
    const es = sourceAt(0);
    es.emit({
      kind: FRAME_KIND.EVENT_RECEIVED,
      event_id: "evt_early",
      actor: "steer:k@e2e.com",
    });
    expect(reconcileOptimistic(Z_A, tempId, "evt_early", false)).toBe(false);
    const events = getSnapshot(Z_A).events;
    expect(events).toHaveLength(1);
    expect(events[0]?.id).toBe("evt_early");
    // The optimistic row was the only holder of the operator's message;
    // reconciliation must not blank it out of the thread until reload.
    expect(events[0]?.text).toBe("deploy the canary");
    a();
  });

  it("drops the optimistic duplicate when the real event completed before reconciliation", () => {
    const a = subscribe(
      WS,
      Z_A,
      [row({ event_id: "evt_fast", status: "processed", response_text: "done" })],
      () => {},
    );
    const tempId = appendOptimistic(Z_A, "fast task", "steer:k@e2e.com");
    expect(reconcileOptimistic(Z_A, tempId, "evt_fast", false)).toBe(true);
    const events = getSnapshot(Z_A).events;
    expect(events).toHaveLength(1);
    expect(events[0]?.id).toBe("evt_fast");
    expect(events[0]?.status).toBe("processed");
    a();
  });

  it("appendOptimistic with no active subscription is a no-op (returns empty string)", () => {
    const tempId = appendOptimistic("never_subscribed", "x", "actor");
    expect(tempId).toBe("");
    expect(getSnapshot("never_subscribed").events).toHaveLength(0);
  });
});

describe("fleet-stream-registry — mutation edges", () => {
  it("reconcileOptimistic is a no-op for a fleet with no active subscription", () => {
    reconcileOptimistic("never_subscribed", "temp_x", "evt_x", false);
    expect(getSnapshot("never_subscribed").events).toHaveLength(0);
  });

  it("discardOptimistic removes only the matching row", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const keep = appendOptimistic(Z_A, "first", "steer:k");
    const stale = appendOptimistic(Z_A, "second", "steer:k");
    discardOptimistic(Z_A, stale);
    expect(getSnapshot(Z_A).events.map((e) => e.id)).toEqual([keep]);
    a();
  });

  it("discardOptimistic is a no-op for a fleet with no active subscription", () => {
    discardOptimistic("never_subscribed", "temp_x");
    expect(getSnapshot("never_subscribed").events).toHaveLength(0);
  });

  it("a stale tempId from a torn-down entry can never discard a fresh row", () => {
    // A refusal can land after its stream entry is gone: send, navigate away
    // past the idle window (entry torn down), come back, send again. A
    // per-entry counter would hand the new row the SAME id the late refusal
    // discards, removing the operator's newest pending message.
    const first = subscribe(WS, Z_A, NO_SEED, () => {});
    const staleTempId = appendOptimistic(Z_A, "old send", "steer:k");
    first();
    vi.advanceTimersByTime(IDLE_RELEASE_MS);
    expect(getSnapshot(Z_A).events).toHaveLength(0);

    const second = subscribe(WS, Z_A, NO_SEED, () => {});
    const freshTempId = appendOptimistic(Z_A, "newest message", "steer:k");
    expect(freshTempId).not.toBe(staleTempId);
    discardOptimistic(Z_A, staleTempId);
    const events = getSnapshot(Z_A).events;
    expect(events).toHaveLength(1);
    expect(events[0]?.id).toBe(freshTempId);
    expect(events[0]?.text).toBe("newest message");
    second();
  });

  it("rewrites only the matching optimistic row and leaves the others untouched", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const keep = appendOptimistic(Z_A, "first", "steer:k");
    const target = appendOptimistic(Z_A, "second", "steer:k");
    reconcileOptimistic(Z_A, target, "evt_real", false);
    const snap = getSnapshot(Z_A);
    expect(snap.events.find((e) => e.id === "evt_real")?.status).toBe("received");
    expect(snap.events.find((e) => e.id === keep)?.status).toBe("optimistic");
    a();
  });

  it("calling the returned unsubscribe again after teardown is a no-op", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    a();
    vi.advanceTimersByTime(IDLE_RELEASE_MS + 1);
    expect(() => a()).not.toThrow();
  });
});
