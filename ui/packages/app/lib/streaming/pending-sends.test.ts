import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  MAX_PENDING_SENDS,
  PENDING_SEND_STATE,
  PENDING_SEND_TTL_MS,
  __resetPendingSendsForTests,
  beginPendingSend,
  dismissPendingSend,
  failPendingSend,
  findPendingSend,
  getPendingSends,
  settlePendingSend,
  subscribePendingSends,
  type LedgerScope,
  type PendingSend,
} from "./pending-sends";

const SUBJECT = "user_ledger";
const SCOPE: LedgerScope = { subject: SUBJECT, workspaceId: "ws_ledger", fleetId: "fleet_ledger" };
const SIGNED_OUT: LedgerScope = { ...SCOPE, subject: null };
const STORAGE_KEY = `agentsfleet:pending-sends:${SUBJECT}:ws_ledger:fleet_ledger`;
const OTHER_FLEET_KEY = `agentsfleet:pending-sends:${SUBJECT}:ws_ledger:fleet_other`;
const FOREIGN_KEY = "agentsfleet:pending-sends:user_gone:ws_x:fleet_x";
const UNRELATED_KEY = "someone-else:setting";
const NOW_MS = 1_790_553_600_000;
const HOUR_MS = 3_600_000;

function send(operationId: string, text: string, submittedAtMs = NOW_MS) {
  return { operationId, text, submittedAtMs };
}

function stored(entries: PendingSend[]): string {
  return JSON.stringify(entries);
}

function states(scope: LedgerScope = SCOPE): [string, string][] {
  return getPendingSends(scope).map((entry) => [entry.operationId, entry.state]);
}

// A Storage over a Map, with any method replaced. The test DOM's own
// localStorage is a proxy a spy cannot be taken back off, so a misbehaving
// storage is swapped in whole through the window getter instead.
function fakeStorage(overrides: Partial<Pick<Storage, "getItem" | "setItem" | "removeItem">> = {}): Storage {
  const held = new Map<string, string>();
  const base: Storage = {
    get length() { return held.size; },
    key: (index) => [...held.keys()][index] ?? null,
    getItem: (key) => held.get(key) ?? null,
    setItem: (key, value) => { held.set(key, value); },
    removeItem: (key) => { held.delete(key); },
    clear: () => { held.clear(); },
  };
  return { ...base, ...overrides, get length() { return held.size; } } as Storage;
}

// Another tab's write, as the browser reports it to this one.
function otherTabWrote(key: string | null, newValue: string | null): void {
  window.dispatchEvent(new StorageEvent("storage", { key, newValue }));
}

beforeEach(() => {
  vi.useFakeTimers({ now: NOW_MS, toFake: ["Date"] });
  __resetPendingSendsForTests();
});

afterEach(() => {
  __resetPendingSendsForTests();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("pending-sends ledger", () => {
  it("test_ledger_entry_lives_from_submit_to_ack", () => {
    beginPendingSend(SCOPE, send("op-1", "deploy"));
    expect(getPendingSends(SCOPE)).toEqual([
      { operationId: "op-1", text: "deploy", state: PENDING_SEND_STATE.SENDING, submittedAtMs: NOW_MS },
    ]);
    // Mirrored while unresolved, so a reload can find it.
    expect(window.localStorage.getItem(STORAGE_KEY)).toContain("op-1");

    settlePendingSend(SCOPE, "op-1");
    expect(getPendingSends(SCOPE)).toEqual([]);
    expect(window.localStorage.getItem(STORAGE_KEY)).toBeNull();
  });

  it("keeps one entry per operation id and moves its state", () => {
    beginPendingSend(SCOPE, send("op-a", "a"));
    beginPendingSend(SCOPE, send("op-b", "b"));
    failPendingSend(SCOPE, "op-a", PENDING_SEND_STATE.REFUSED);
    failPendingSend(SCOPE, "op-b", PENDING_SEND_STATE.SESSION);
    expect(states()).toEqual([["op-a", PENDING_SEND_STATE.REFUSED], ["op-b", PENDING_SEND_STATE.SESSION]]);
    // Sending again puts the same id back in flight, at the tail, once.
    beginPendingSend(SCOPE, send("op-a", "a"));
    expect(states()).toEqual([["op-b", PENDING_SEND_STATE.SESSION], ["op-a", PENDING_SEND_STATE.SENDING]]);
    dismissPendingSend(SCOPE, "op-b");
    expect(states()).toEqual([["op-a", PENDING_SEND_STATE.SENDING]]);
    expect(findPendingSend(SCOPE, "op-a")?.text).toBe("a");
    expect(findPendingSend(SCOPE, "op-b")).toBeUndefined();
  });

  it("test_ledger_survives_reload_and_syncs_tabs", () => {
    beginPendingSend(SCOPE, send("op-1", "deploy"));
    beginPendingSend(SCOPE, send("op-2", "stop"));
    failPendingSend(SCOPE, "op-2", PENDING_SEND_STATE.REFUSED);

    // A fresh document: the entry still `sending` was owned by a document that
    // is gone, so it reads as unknown; a refusal keeps its state.
    __resetPendingSendsForTests({ keepStorage: true });
    expect(states()).toEqual([["op-1", PENDING_SEND_STATE.UNKNOWN], ["op-2", PENDING_SEND_STATE.REFUSED]]);

    // Another tab writes the key: this one takes its value.
    const listener = vi.fn();
    subscribePendingSends(SCOPE, listener);
    const written: PendingSend[] = [{ operationId: "op-3", text: "from tab b", state: PENDING_SEND_STATE.REFUSED, submittedAtMs: NOW_MS }];
    otherTabWrote(STORAGE_KEY, stored(written));
    expect(listener).toHaveBeenCalledTimes(1);
    expect(getPendingSends(SCOPE)).toEqual(written);
    // A key this document never read is ignored.
    otherTabWrote(FOREIGN_KEY, "[]");
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("applies another tab's write to a ledger no composer is showing, so a remount is current", () => {
    const off = subscribePendingSends(SCOPE, () => {});
    beginPendingSend(SCOPE, send("op-1", "a"));
    failPendingSend(SCOPE, "op-1", PENDING_SEND_STATE.REFUSED);
    off();
    // Another tab resent and settled it, then removed the key.
    otherTabWrote(STORAGE_KEY, null);
    expect(getPendingSends(SCOPE)).toEqual([]);
  });

  it("keeps this tab's own in-flight send when another tab's write had not seen it", () => {
    beginPendingSend(SCOPE, send("op-mine", "mine"));
    const theirs: PendingSend = { operationId: "op-theirs", text: "theirs", state: PENDING_SEND_STATE.REFUSED, submittedAtMs: NOW_MS };
    otherTabWrote(STORAGE_KEY, stored([theirs]));
    expect(states()).toEqual([["op-theirs", PENDING_SEND_STATE.REFUSED], ["op-mine", PENDING_SEND_STATE.SENDING]]);
    // The next write merges with storage, so neither tab's entry is lost.
    window.localStorage.setItem(STORAGE_KEY, stored([theirs]));
    failPendingSend(SCOPE, "op-mine", PENDING_SEND_STATE.UNKNOWN);
    expect(JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "[]").map((e: PendingSend) => e.operationId)).toEqual(["op-theirs", "op-mine"]);
  });

  it("never takes a closed tab's stale `sending` back over an ending this tab knows", () => {
    // A tab that died mid-send left `sending` in storage; this document read
    // it as unknown, which is what shows Resend.
    const orphan: PendingSend = { operationId: "op-orphan", text: "lost", state: PENDING_SEND_STATE.SENDING, submittedAtMs: NOW_MS };
    window.localStorage.setItem(STORAGE_KEY, stored([orphan]));
    expect(states()).toEqual([["op-orphan", PENDING_SEND_STATE.UNKNOWN]]);
    // An unrelated write here merges with storage, which still says `sending`.
    beginPendingSend(SCOPE, send("op-new", "new"));
    expect(states()).toEqual([["op-orphan", PENDING_SEND_STATE.UNKNOWN], ["op-new", PENDING_SEND_STATE.SENDING]]);
  });

  it("re-reads every mirrored ledger when another tab clears storage", () => {
    beginPendingSend(SCOPE, send("op-1", "a"));
    failPendingSend(SCOPE, "op-1", PENDING_SEND_STATE.REFUSED);
    beginPendingSend(SIGNED_OUT, send("op-memory", "typed before sign-in finished"));
    failPendingSend(SIGNED_OUT, "op-memory", PENDING_SEND_STATE.REFUSED);
    const listener = vi.fn();
    subscribePendingSends(SCOPE, listener);
    window.localStorage.clear();
    otherTabWrote(null, null);
    expect(getPendingSends(SCOPE)).toEqual([]);
    expect(listener).toHaveBeenCalledTimes(1);
    // A memory-only ledger was never in storage, so a clear leaves it alone.
    expect(states(SIGNED_OUT)).toEqual([["op-memory", PENDING_SEND_STATE.REFUSED]]);
  });

  it("keeps its copy when a clear arrives and storage will not be read", () => {
    beginPendingSend(SCOPE, send("op-1", "a"));
    failPendingSend(SCOPE, "op-1", PENDING_SEND_STATE.UNKNOWN);
    const revoked = fakeStorage({ getItem: () => { throw new Error("revoked"); } });
    vi.spyOn(window, "localStorage", "get").mockReturnValue(revoked);
    otherTabWrote(null, null);
    expect(states()).toEqual([["op-1", PENDING_SEND_STATE.UNKNOWN]]);
  });

  it("expires entries after a day, and sweeps ledgers whose entries all expired", () => {
    const old: PendingSend = { operationId: "op-old", text: "yesterday", state: PENDING_SEND_STATE.REFUSED, submittedAtMs: NOW_MS - PENDING_SEND_TTL_MS };
    const fresh: PendingSend = { operationId: "op-fresh", text: "today", state: PENDING_SEND_STATE.REFUSED, submittedAtMs: NOW_MS - HOUR_MS };
    window.localStorage.setItem(STORAGE_KEY, stored([old, fresh]));
    window.localStorage.setItem(OTHER_FLEET_KEY, stored([old]));
    window.localStorage.setItem(UNRELATED_KEY, "kept");
    expect(states()).toEqual([["op-fresh", PENDING_SEND_STATE.REFUSED]]);
    expect(window.localStorage.getItem(OTHER_FLEET_KEY)).toBeNull();
    expect(window.localStorage.getItem(UNRELATED_KEY)).toBe("kept");
  });

  it("caps a fleet's ledger, dropping the oldest ended sends and never one in flight", () => {
    beginPendingSend(SCOPE, send("op-flying", "still out"));
    for (let index = 0; index < MAX_PENDING_SENDS; index += 1) {
      beginPendingSend(SCOPE, send(`op-${index}`, `refused ${index}`));
      failPendingSend(SCOPE, `op-${index}`, PENDING_SEND_STATE.REFUSED);
    }
    const ids = getPendingSends(SCOPE).map((entry) => entry.operationId);
    expect(ids).toHaveLength(MAX_PENDING_SENDS);
    expect(ids).toContain("op-flying");
    expect(ids).not.toContain("op-0");
  });

  it("reads a malformed or foreign stored value as nothing", () => {
    for (const raw of ["{not json", JSON.stringify({ an: "object" })]) {
      __resetPendingSendsForTests();
      window.localStorage.setItem(STORAGE_KEY, raw);
      expect(getPendingSends(SCOPE)).toEqual([]);
    }
    __resetPendingSendsForTests();
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify([7, { operationId: 7 }, { operationId: "op-ok", text: "t", state: "refused", submittedAtMs: NOW_MS }, { operationId: "op-bad", text: "t", state: "lost", submittedAtMs: NOW_MS }]),
    );
    expect(getPendingSends(SCOPE).map((entry) => entry.operationId)).toEqual(["op-ok"]);
  });

  it("test_ledger_without_storage", () => {
    const cases: Array<() => void> = [
      () => vi.spyOn(window, "localStorage", "get").mockImplementation(() => { throw new Error("storage denied"); }),
      () => vi.spyOn(window, "localStorage", "get").mockReturnValue(undefined as unknown as Storage),
      () => vi.stubGlobal("window", undefined),
    ];
    for (const withoutStorage of cases) {
      __resetPendingSendsForTests();
      withoutStorage();
      beginPendingSend(SCOPE, send("op-1", "deploy"));
      failPendingSend(SCOPE, "op-1", PENDING_SEND_STATE.UNKNOWN);
      expect(states()).toEqual([["op-1", PENDING_SEND_STATE.UNKNOWN]]);
      settlePendingSend(SCOPE, "op-1");
      expect(getPendingSends(SCOPE)).toEqual([]);
      vi.unstubAllGlobals();
      vi.restoreAllMocks();
    }
  });

  it("keeps the ledger in memory when a write is refused for quota", () => {
    let full = true;
    const room = fakeStorage();
    const quota = fakeStorage({
      getItem: (key) => room.getItem(key),
      setItem: (key, value) => {
        if (full) throw new DOMException("full", "QuotaExceededError");
        room.setItem(key, value);
      },
    });
    vi.spyOn(window, "localStorage", "get").mockReturnValue(quota);
    beginPendingSend(SCOPE, send("op-1", "deploy"));
    expect(states()).toEqual([["op-1", PENDING_SEND_STATE.SENDING]]);
    expect(room.getItem(STORAGE_KEY)).toBeNull();
    // The next write builds on this tab's copy: storage never saw op-1 end.
    failPendingSend(SCOPE, "op-1", PENDING_SEND_STATE.UNKNOWN);
    beginPendingSend(SCOPE, send("op-2", "stop"));
    expect(states()).toEqual([["op-1", PENDING_SEND_STATE.UNKNOWN], ["op-2", PENDING_SEND_STATE.SENDING]]);
    // A write that lands puts the whole ledger back in storage.
    full = false;
    failPendingSend(SCOPE, "op-2", PENDING_SEND_STATE.REFUSED);
    expect(JSON.parse(room.getItem(STORAGE_KEY) ?? "[]")).toHaveLength(2);
  });

  it("brings back only its own unsaved sends, never what another tab removed", () => {
    let full = false;
    const room = fakeStorage();
    const quota = fakeStorage({
      getItem: (key) => room.getItem(key),
      setItem: (key, value) => {
        if (full) throw new DOMException("full", "QuotaExceededError");
        room.setItem(key, value);
      },
    });
    vi.spyOn(window, "localStorage", "get").mockReturnValue(quota);
    room.setItem(STORAGE_KEY, stored([{ operationId: "op-x", text: "x", state: PENDING_SEND_STATE.REFUSED, submittedAtMs: NOW_MS }]));
    expect(states()).toEqual([["op-x", PENDING_SEND_STATE.REFUSED]]);
    full = true;
    beginPendingSend(SCOPE, send("op-a", "a"));
    // Another tab dismisses op-x while this one cannot write.
    room.removeItem(STORAGE_KEY);
    otherTabWrote(STORAGE_KEY, null);
    expect(states()).toEqual([["op-a", PENDING_SEND_STATE.SENDING]]);
    full = false;
    failPendingSend(SCOPE, "op-a", PENDING_SEND_STATE.UNKNOWN);
    expect(JSON.parse(room.getItem(STORAGE_KEY) ?? "[]").map((entry: PendingSend) => entry.operationId)).toEqual(["op-a"]);
  });

  it("survives a storage whose reads and removals throw after it was found", () => {
    const revoked = fakeStorage({
      getItem: () => { throw new Error("revoked"); },
      removeItem: () => { throw new Error("revoked"); },
    });
    revoked.setItem(FOREIGN_KEY, stored([]));
    vi.spyOn(window, "localStorage", "get").mockReturnValue(revoked);
    // The first read sweeps (removal throws) and hydrates (read throws): both
    // are best-effort, so the ledger is simply empty.
    expect(getPendingSends(SCOPE)).toEqual([]);
    expect(findPendingSend(SCOPE, "op-1")).toBeUndefined();
    // A read that throws is not an empty ledger: the ended send survives the next write.
    beginPendingSend(SCOPE, send("op-1", "deploy"));
    failPendingSend(SCOPE, "op-1", PENDING_SEND_STATE.UNKNOWN);
    beginPendingSend(SCOPE, send("op-2", "stop"));
    expect(states()).toEqual([["op-1", PENDING_SEND_STATE.UNKNOWN], ["op-2", PENDING_SEND_STATE.SENDING]]);
  });

  it("notifies subscribers on every write and stops after unsubscribe", () => {
    const listener = vi.fn();
    const bystander = vi.fn();
    const unsubscribe = subscribePendingSends(SCOPE, listener);
    const unsubscribeBystander = subscribePendingSends(SCOPE, bystander);
    beginPendingSend(SCOPE, send("op-1", "deploy"));
    failPendingSend(SCOPE, "op-1", PENDING_SEND_STATE.REFUSED);
    settlePendingSend(SCOPE, "op-1");
    expect(listener).toHaveBeenCalledTimes(3);
    unsubscribe();
    beginPendingSend(SCOPE, send("op-2", "again"));
    expect(listener).toHaveBeenCalledTimes(3);
    // The other subscriber is still told.
    expect(bystander).toHaveBeenCalledTimes(4);
    unsubscribeBystander();
  });
});
