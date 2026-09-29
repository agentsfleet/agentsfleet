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
  otherTabStored,
  otherTabWrote,
  send,
  states,
  stored,
} from "@/tests/fleet-thread/ledger-fixtures";

const { REFUSED, SENDING, UNKNOWN } = PENDING_SEND_STATE;
const FAR = "op-far";
const NEAR = "op-near";
const OWN = "op-own";
const OTHER_FLEET = { ...SCOPE, fleetId: "fleet_other" };
const OTHER_FLEET_KEY = STORAGE_KEY.replace(SCOPE.fleetId, OTHER_FLEET.fleetId);

function sending(operationId: string) {
  return { ...send(operationId, "deploy"), state: SENDING };
}

function withLocks() {
  const manager = fakeLockManager();
  vi.stubGlobal("navigator", { locks: manager.locks });
  return manager;
}

// A manager that is there and refuses every request, as an opaque origin's does.
function withRefusingLocks() {
  const request = vi.fn(() => Promise.reject(new DOMException("denied", "SecurityError")));
  vi.stubGlobal("navigator", { locks: { request } });
  return request;
}

beforeEach(() => {
  vi.useFakeTimers({ now: NOW_MS, toFake: ["Date"] });
  __resetPendingSendsForTests();
});

afterEach(() => {
  // Storage back first, so the reset can clear what the test wrote.
  vi.restoreAllMocks();
  __resetPendingSendsForTests();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("another tab's send", () => {
  it("test_foreign_sending_surfaces_when_its_tab_is_gone", async () => {
    const { heldElsewhere } = withLocks();
    const closeOwnerTab = heldElsewhere(`${SEND_LOCK_PREFIX}${FAR}`);
    getPendingSends(SCOPE);
    otherTabStored([sending(FAR)]);
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

  it("reads the ending its owner stored when the grant beats that write's storage event", async () => {
    const { heldElsewhere } = withLocks();
    const releaseFar = heldElsewhere(`${SEND_LOCK_PREFIX}${FAR}`);
    const releaseNear = heldElsewhere(`${SEND_LOCK_PREFIX}${NEAR}`);
    getPendingSends(SCOPE);
    otherTabStored([sending(FAR), sending(NEAR)]);
    // The owner refuses one, settles the other, and lets go; the event is late.
    window.localStorage.setItem(STORAGE_KEY, stored([{ ...sending(FAR), state: REFUSED }]));
    releaseFar();
    releaseNear();
    await lockTurn();
    expect(states()).toEqual([[FAR, REFUSED]]);
  });

  it("keeps another tab's send, as unknown, when storage cannot be read at the grant", async () => {
    const { heldElsewhere } = withLocks();
    const release = heldElsewhere(`${SEND_LOCK_PREFIX}${FAR}`);
    getPendingSends(SCOPE);
    otherTabStored([sending(FAR)]);
    // Storage revoked while the owner was still sending.
    const revoked = vi.spyOn(window, "localStorage", "get").mockImplementation(() => {
      throw new Error("revoked");
    });
    release();
    await lockTurn();
    expect(revoked).toHaveBeenCalled();
    expect(states()).toEqual([[FAR, UNKNOWN]]);
  });

  it("test_refusing_lock_manager_reads_unknown: reads another tab's send as unknown when the lock manager refuses to say, as without Web Locks", async () => {
    const request = withRefusingLocks();
    window.localStorage.setItem(STORAGE_KEY, stored([sending(FAR)]));
    expect(states()).toEqual([[FAR, SENDING]]);
    await lockTurn();
    expect(states()).toEqual([[FAR, UNKNOWN]]);
    // From the refusal on, a first read coerces at once and asks nothing.
    window.localStorage.setItem(OTHER_FLEET_KEY, stored([sending(NEAR)]));
    expect(states(OTHER_FLEET)).toEqual([[NEAR, UNKNOWN]]);
    expect(request).toHaveBeenCalledTimes(1);
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

  it("asks once per held or watched id, and asks nothing more of a manager that refused", async () => {
    const request = withRefusingLocks();
    const locks = new SendLocks();
    const gone = vi.fn();
    locks.hold(OWN);
    locks.hold(OWN);
    locks.watch(OWN, vi.fn());
    locks.watch(FAR, gone);
    locks.watch(FAR, gone);
    expect(request).toHaveBeenCalledTimes(2);
    await lockTurn();
    // The refused watch reports its send as one nobody can vouch for.
    expect(gone).toHaveBeenCalledOnce();
    expect(locks.supported()).toBe(false);
    locks.hold(NEAR);
    locks.watch(NEAR, vi.fn());
    expect(request).toHaveBeenCalledTimes(2);
  });

  it("stops asking once a held lock's request is refused", async () => {
    const request = vi.fn(() => Promise.reject(new DOMException("gone", "InvalidStateError")));
    vi.stubGlobal("navigator", { locks: { request } });
    const locks = new SendLocks();
    locks.hold(OWN);
    await lockTurn();
    expect(locks.supported()).toBe(false);
  });

  it("does not take a watch whose callback threw for a manager that refused", async () => {
    withLocks();
    const locks = new SendLocks();
    locks.watch(FAR, () => {
      throw new Error("gone threw");
    });
    await lockTurn();
    expect(locks.supported()).toBe(true);
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
