import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  PENDING_SEND_STATE,
  __resetPendingSendsForTests,
  beginPendingSend,
  dismissPendingSend,
  failPendingSend,
  findPendingSendByText,
  getPendingSends,
  settlePendingSend,
  subscribePendingSends,
} from "./pending-sends";

const WS = "ws_ledger";
const FLEET = "fleet_ledger";
const OTHER_FLEET = "fleet_other";
const STORAGE_KEY = `agentsfleet:pending-sends:${WS}:${FLEET}`;
const SUBMITTED_AT_MS = 1_700_000_000_000;

function send(operationId: string, text: string) {
  return { operationId, text, submittedAtMs: SUBMITTED_AT_MS };
}

beforeEach(() => {
  __resetPendingSendsForTests();
});

afterEach(() => {
  __resetPendingSendsForTests();
  vi.restoreAllMocks();
});

describe("pending-sends ledger", () => {
  it("test_ledger_entry_lives_from_submit_to_ack", () => {
    beginPendingSend(WS, FLEET, send("op-1", "deploy"));
    expect(getPendingSends(WS, FLEET)).toEqual([
      { operationId: "op-1", text: "deploy", state: PENDING_SEND_STATE.SENDING, submittedAtMs: SUBMITTED_AT_MS },
    ]);
    // Mirrored while unresolved, so a reload can find it.
    expect(window.localStorage.getItem(STORAGE_KEY)).toContain("op-1");

    settlePendingSend(WS, FLEET, "op-1");
    expect(getPendingSends(WS, FLEET)).toEqual([]);
    expect(window.localStorage.getItem(STORAGE_KEY)).toBeNull();
  });

  it("keeps one entry per operation id and moves its state", () => {
    beginPendingSend(WS, FLEET, send("op-a", "a"));
    beginPendingSend(WS, FLEET, send("op-b", "b"));
    failPendingSend(WS, FLEET, "op-a", PENDING_SEND_STATE.REFUSED);
    failPendingSend(WS, FLEET, "op-b", PENDING_SEND_STATE.SESSION);
    expect(getPendingSends(WS, FLEET).map((entry) => [entry.operationId, entry.state])).toEqual([
      ["op-a", PENDING_SEND_STATE.REFUSED],
      ["op-b", PENDING_SEND_STATE.SESSION],
    ]);
    // Sending again puts the same id back in flight, at the tail, once.
    beginPendingSend(WS, FLEET, send("op-a", "a"));
    expect(getPendingSends(WS, FLEET).map((entry) => [entry.operationId, entry.state])).toEqual([
      ["op-b", PENDING_SEND_STATE.SESSION],
      ["op-a", PENDING_SEND_STATE.SENDING],
    ]);
    dismissPendingSend(WS, FLEET, "op-b");
    expect(getPendingSends(WS, FLEET).map((entry) => entry.operationId)).toEqual(["op-a"]);
    // Dismissing what is not there changes nothing, and notifies nobody.
    const before = getPendingSends(WS, FLEET);
    dismissPendingSend(WS, FLEET, "op-missing");
    expect(getPendingSends(WS, FLEET)).toBe(before);
  });

  it("finds an unresolved send by its exact text, per fleet", () => {
    beginPendingSend(WS, FLEET, send("op-1", "old"));
    beginPendingSend(WS, OTHER_FLEET, send("op-2", "old"));
    expect(findPendingSendByText(WS, FLEET, "old")?.operationId).toBe("op-1");
    expect(findPendingSendByText(WS, FLEET, "old\nnew")).toBeUndefined();
    expect(findPendingSendByText(WS, OTHER_FLEET, "old")?.operationId).toBe("op-2");
  });

  it("test_ledger_survives_reload_and_syncs_tabs", async () => {
    beginPendingSend(WS, FLEET, send("op-1", "deploy"));
    beginPendingSend(WS, FLEET, send("op-2", "stop"));
    failPendingSend(WS, FLEET, "op-2", PENDING_SEND_STATE.REFUSED);

    // A fresh module is a fresh document: the entry still `sending` was owned
    // by a document that is gone, so it reads as unknown; a refusal keeps
    // its state.
    vi.resetModules();
    const fresh = await import("./pending-sends");
    expect(fresh.getPendingSends(WS, FLEET).map((entry) => [entry.operationId, entry.state])).toEqual([
      ["op-1", PENDING_SEND_STATE.UNKNOWN],
      ["op-2", PENDING_SEND_STATE.REFUSED],
    ]);

    // Another tab writes the key: this one takes the value as it stands.
    const listener = vi.fn();
    const unsubscribe = fresh.subscribePendingSends(WS, FLEET, listener);
    const written = [{ operationId: "op-3", text: "from tab b", state: PENDING_SEND_STATE.SENDING, submittedAtMs: SUBMITTED_AT_MS }];
    window.dispatchEvent(new StorageEvent("storage", { key: STORAGE_KEY, newValue: JSON.stringify(written) }));
    expect(listener).toHaveBeenCalledTimes(1);
    expect(fresh.getPendingSends(WS, FLEET)).toEqual(written);
    // A key nobody here reads is ignored.
    window.dispatchEvent(new StorageEvent("storage", { key: "agentsfleet:pending-sends:ws_x:fleet_x", newValue: "[]" }));
    expect(listener).toHaveBeenCalledTimes(1);
    unsubscribe();
    fresh.__resetPendingSendsForTests();
  });

  it("reads a malformed or foreign stored value as nothing", async () => {
    window.localStorage.setItem(STORAGE_KEY, "{not json");
    vi.resetModules();
    let fresh = await import("./pending-sends");
    expect(fresh.getPendingSends(WS, FLEET)).toEqual([]);

    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify([{ operationId: 7 }, { operationId: "op-ok", text: "t", state: "refused", submittedAtMs: 1 }, { operationId: "op-bad", text: "t", state: "lost", submittedAtMs: 1 }]),
    );
    vi.resetModules();
    fresh = await import("./pending-sends");
    expect(fresh.getPendingSends(WS, FLEET).map((entry) => entry.operationId)).toEqual(["op-ok"]);
    fresh.__resetPendingSendsForTests();
  });

  it("test_ledger_without_storage", () => {
    const original = Object.getOwnPropertyDescriptor(window, "localStorage");
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      get() {
        throw new Error("storage denied");
      },
    });
    try {
      beginPendingSend(WS, FLEET, send("op-1", "deploy"));
      failPendingSend(WS, FLEET, "op-1", PENDING_SEND_STATE.UNKNOWN);
      expect(getPendingSends(WS, FLEET).map((entry) => entry.state)).toEqual([PENDING_SEND_STATE.UNKNOWN]);
      settlePendingSend(WS, FLEET, "op-1");
      expect(getPendingSends(WS, FLEET)).toEqual([]);
    } finally {
      if (original) Object.defineProperty(window, "localStorage", original);
      else Reflect.deleteProperty(window, "localStorage");
    }
  });

  it("notifies subscribers on every write and stops after unsubscribe", () => {
    const listener = vi.fn();
    const unsubscribe = subscribePendingSends(WS, FLEET, listener);
    beginPendingSend(WS, FLEET, send("op-1", "deploy"));
    failPendingSend(WS, FLEET, "op-1", PENDING_SEND_STATE.REFUSED);
    settlePendingSend(WS, FLEET, "op-1");
    expect(listener).toHaveBeenCalledTimes(3);
    unsubscribe();
    beginPendingSend(WS, FLEET, send("op-2", "again"));
    expect(listener).toHaveBeenCalledTimes(3);
  });
});
