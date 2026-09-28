import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  PENDING_SEND_STATE,
  __resetPendingSendsForTests,
  beginPendingSend,
  failPendingSend,
  getPendingSends,
  settlePendingSend,
} from "./pending-sends";
import { SEND_LOCK_PREFIX, SendLocks } from "./pending-sends-locks";
import {
  NOW_MS,
  SCOPE,
  STORAGE_KEY,
  fakeLockManager,
  lockTurn,
  otherTabWrote,
  send,
  states,
  stored,
} from "@/tests/fleet-thread/ledger-fixtures";

const { SENDING, UNKNOWN } = PENDING_SEND_STATE;
const FAR = "op-far";
const OWN = "op-own";

function sending(operationId: string) {
  return { ...send(operationId, "deploy"), state: SENDING };
}

function withLocks() {
  const manager = fakeLockManager();
  vi.stubGlobal("navigator", { locks: manager.locks });
  return manager;
}

beforeEach(() => {
  vi.useFakeTimers({ now: NOW_MS, toFake: ["Date"] });
  __resetPendingSendsForTests();
});

afterEach(() => {
  __resetPendingSendsForTests();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("another tab's send", () => {
  it("test_foreign_sending_surfaces_when_its_tab_is_gone", async () => {
    const { heldElsewhere } = withLocks();
    const closeOwnerTab = heldElsewhere(`${SEND_LOCK_PREFIX}${FAR}`);
    getPendingSends(SCOPE);
    otherTabWrote(STORAGE_KEY, stored([sending(FAR)]));
    await lockTurn();
    // Its owner is alive: still in flight, which the notice hides.
    expect(states()).toEqual([[FAR, SENDING]]);

    closeOwnerTab();
    await lockTurn();
    expect(states()).toEqual([[FAR, UNKNOWN]]);
  });

  it("stays hidden without Web Locks, as before them", async () => {
    getPendingSends(SCOPE);
    otherTabWrote(STORAGE_KEY, stored([sending(FAR)]));
    await lockTurn();
    expect(states()).toEqual([[FAR, SENDING]]);
  });

  it("reads a stored `sending` as unknown on load only once no tab holds its lock", async () => {
    const { heldElsewhere } = withLocks();
    heldElsewhere(`${SEND_LOCK_PREFIX}op-live`);
    window.localStorage.setItem(STORAGE_KEY, stored([sending("op-live"), sending("op-dead")]));
    expect(states()).toEqual([["op-live", SENDING], ["op-dead", SENDING]]);
    await lockTurn();
    expect(states()).toEqual([["op-live", SENDING], ["op-dead", UNKNOWN]]);
  });

  it("takes the ending its owner wrote over the lock's release", async () => {
    const { heldElsewhere } = withLocks();
    const release = heldElsewhere(`${SEND_LOCK_PREFIX}${FAR}`);
    getPendingSends(SCOPE);
    otherTabWrote(STORAGE_KEY, stored([sending(FAR)]));
    // The owner writes its refusal, then lets go.
    otherTabWrote(STORAGE_KEY, stored([{ ...sending(FAR), state: PENDING_SEND_STATE.REFUSED }]));
    release();
    await lockTurn();
    expect(states()).toEqual([[FAR, PENDING_SEND_STATE.REFUSED]]);
  });
});

describe("this tab's own send", () => {
  it("holds its lock from begin until the ending is written", async () => {
    const { locks } = withLocks();
    beginPendingSend(SCOPE, send(OWN, "deploy"));
    let granted = false;
    void locks.request(`${SEND_LOCK_PREFIX}${OWN}`, () => {
      granted = true;
    });
    await lockTurn();
    expect(granted).toBe(false);

    failPendingSend(SCOPE, OWN, UNKNOWN);
    await lockTurn();
    expect(granted).toBe(true);
  });

  it("is never read as orphaned by its own document", async () => {
    withLocks();
    beginPendingSend(SCOPE, send(OWN, "deploy"));
    otherTabWrote(STORAGE_KEY, stored([sending(OWN)]));
    await lockTurn();
    expect(states()).toEqual([[OWN, SENDING]]);
  });

  it("lets go of a lock whose send settled before the grant", async () => {
    const { locks } = withLocks();
    beginPendingSend(SCOPE, send(OWN, "deploy"));
    settlePendingSend(SCOPE, OWN);
    let granted = false;
    void locks.request(`${SEND_LOCK_PREFIX}${OWN}`, () => {
      granted = true;
    });
    await lockTurn();
    expect(granted).toBe(true);
  });
});

describe("SendLocks", () => {
  it("does nothing without a lock manager, or with one whose getter throws", () => {
    const locks = new SendLocks();
    expect(locks.supported()).toBe(false);
    vi.stubGlobal("navigator", {
      get locks(): never {
        throw new Error("blocked");
      },
    });
    expect(locks.supported()).toBe(false);
    const gone = vi.fn();
    locks.hold(OWN);
    locks.watch(FAR, gone);
    locks.release(OWN);
    expect(gone).not.toHaveBeenCalled();
  });

  it("asks once per held or watched id, and forgets a watch whose request fails", async () => {
    const request = vi.fn(() => Promise.reject(new Error("aborted")));
    vi.stubGlobal("navigator", { locks: { request } });
    const locks = new SendLocks();
    locks.hold(OWN);
    locks.hold(OWN);
    locks.watch(OWN, vi.fn());
    locks.watch(FAR, vi.fn());
    locks.watch(FAR, vi.fn());
    expect(request).toHaveBeenCalledTimes(2);
    await lockTurn();
    // The failed watch is forgotten, so the next sighting asks again.
    locks.watch(FAR, vi.fn());
    expect(request).toHaveBeenCalledTimes(3);
  });

  it("lets every held lock go on clear", async () => {
    const { locks: manager } = withLocks();
    const locks = new SendLocks();
    locks.hold(OWN);
    locks.clear();
    let granted = false;
    void manager.request(`${SEND_LOCK_PREFIX}${OWN}`, () => {
      granted = true;
    });
    await lockTurn();
    expect(granted).toBe(true);
  });
});
