import { describe, expect, it, vi } from "vitest";
import { appendOptimistic, discardOptimistic, getSnapshot, markOptimisticFailed, reconcileOptimistic, subscribe } from "./fleet-stream-registry";
import { FRAME_KIND } from "@/lib/api/events-types";
import { setupRegistryTests, row, WS, Z_A, NO_SEED, IDLE_RELEASE_MS, sourceAt } from "@/tests/helpers/fleet-stream-registry-fixtures";

setupRegistryTests();

describe("fleet-stream-registry — optimistic mutations", () => {
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
    expect(reconcileOptimistic(Z_A, tempId, "evt_real")).toBe(false);
    const snap = getSnapshot(Z_A);
    expect(snap.events).toHaveLength(1);
    expect(snap.events[0]?.id).toBe("evt_real");
    expect(snap.events[0]?.status).toBe("received");
    a();
  });

  it("grafts the operator's text onto a body-less live row that beat the POST response", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "deploy the canary", "steer:k@e2e.com");
    // The SSE EVENT_RECEIVED for this steer lands before the Server Action
    // resolves — the frame carries no message body, so the live row holds
    // the real event id with an empty trigger.
    const es = sourceAt(0);
    es.emit({
      kind: FRAME_KIND.EVENT_RECEIVED,
      event_id: "evt_early",
      actor: "steer:k@e2e.com",
    });
    expect(reconcileOptimistic(Z_A, tempId, "evt_early")).toBe(false);
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
    expect(reconcileOptimistic(Z_A, tempId, "evt_fast")).toBe(true);
    const events = getSnapshot(Z_A).events;
    expect(events).toHaveLength(1);
    expect(events[0]?.id).toBe("evt_fast");
    expect(events[0]?.status).toBe("processed");
    a();
  });

  it("markOptimisticFailed flips the matching row to 'failed', keeping its tempId", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "send that fails", "steer:k@e2e.com");
    markOptimisticFailed(Z_A, tempId);
    const snap = getSnapshot(Z_A);
    expect(snap.events).toHaveLength(1);
    expect(snap.events[0]?.id).toBe(tempId);
    expect(snap.events[0]?.status).toBe("failed");
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
    reconcileOptimistic("never_subscribed", "temp_x", "evt_x");
    expect(getSnapshot("never_subscribed").events).toHaveLength(0);
  });

  it("markOptimisticFailed is a no-op for a fleet with no active subscription", () => {
    markOptimisticFailed("never_subscribed", "temp_x");
    expect(getSnapshot("never_subscribed").events).toHaveLength(0);
  });

  it("discardOptimistic removes only the matching row", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const keep = appendOptimistic(Z_A, "first", "steer:k");
    const stale = appendOptimistic(Z_A, "second", "steer:k");
    markOptimisticFailed(Z_A, stale);
    discardOptimistic(Z_A, stale);
    expect(getSnapshot(Z_A).events.map((e) => e.id)).toEqual([keep]);
    a();
  });

  it("discardOptimistic is a no-op for a fleet with no active subscription", () => {
    discardOptimistic("never_subscribed", "temp_x");
    expect(getSnapshot("never_subscribed").events).toHaveLength(0);
  });

  it("a stale tempId from a torn-down entry can never discard a fresh row", () => {
    // A FailedDelivery outlives the stream entry: fail, navigate away past
    // the idle window (entry torn down), come back, send a new message. A
    // per-entry counter would hand the new row the SAME id the failure
    // stored, and retry's discard would remove the operator's newest
    // pending message instead of the stale failed one.
    const first = subscribe(WS, Z_A, NO_SEED, () => {});
    const staleTempId = appendOptimistic(Z_A, "old failed send", "steer:k");
    markOptimisticFailed(Z_A, staleTempId);
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
    reconcileOptimistic(Z_A, target, "evt_real");
    const snap = getSnapshot(Z_A);
    expect(snap.events.find((e) => e.id === "evt_real")?.status).toBe("received");
    expect(snap.events.find((e) => e.id === keep)?.status).toBe("optimistic");
    a();
  });

  it("markOptimisticFailed touches only the matching row", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const keep = appendOptimistic(Z_A, "first", "steer:k");
    const target = appendOptimistic(Z_A, "second", "steer:k");
    markOptimisticFailed(Z_A, target);
    const snap = getSnapshot(Z_A);
    expect(snap.events.find((e) => e.id === target)?.status).toBe("failed");
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
