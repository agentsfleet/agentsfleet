import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  PENDING_SEND_STATE,
  PENDING_SEND_TTL_MS,
  __resetPendingSendsForTests,
  beginPendingSend,
  getPendingSends,
  settlePendingSend,
} from "./pending-sends";
import { purgeOtherUsers } from "./pending-sends-storage";
import { NOW_MS, SCOPE, STORAGE_KEY, send } from "@/tests/fleet-thread/ledger-fixtures";

const PREFIX = "agentsfleet:pending-sends";
const SUBJECT = "user_ledger";
// Starts with `SUBJECT`: only the separator after a subject tells the two apart.
const LONGER_SUBJECT = `${SUBJECT}_2`;
const OWN_KEYS = [`${PREFIX}:${SUBJECT}:ws_a:fleet_a`, `${PREFIX}:${SUBJECT}:ws_b:fleet_b`];
const OTHER_USERS_KEYS = [`${PREFIX}:user_gone:ws_a:fleet_a`, `${PREFIX}:${LONGER_SUBJECT}:ws_a:fleet_a`];
const UNRELATED_KEY = "someone-else:setting";
const OTHER_FLEET_KEY = `${PREFIX}:${SUBJECT}:ws_ledger:fleet_other`;
const EXPIRED_FLEET_KEY = `${PREFIX}:${SUBJECT}:ws_ledger:fleet_expired`;

// A state this build has never heard of, with a field it does not read either.
const NEWER = { operationId: "op-newer", text: "from the next build", state: "queued", submittedAtMs: NOW_MS, attempts: { made: 2 } };

function storedHere(key = STORAGE_KEY): unknown[] {
  return JSON.parse(window.localStorage.getItem(key) ?? "[]") as unknown[];
}

afterEach(() => {
  window.localStorage.clear();
});

describe("purgeOtherUsers", () => {
  it("removes every other user's ledger and keeps this user's and every unrelated key", () => {
    for (const key of [...OWN_KEYS, ...OTHER_USERS_KEYS, UNRELATED_KEY]) window.localStorage.setItem(key, "[]");
    purgeOtherUsers(SUBJECT);
    expect(Object.keys(window.localStorage).sort()).toEqual([...OWN_KEYS, UNRELATED_KEY].sort());
  });
});

describe("test_tombstones_outlast_the_cap: an entry a newer build wrote", () => {
  beforeEach(() => {
    vi.useFakeTimers({ now: NOW_MS, toFake: ["Date"] });
    __resetPendingSendsForTests();
  });

  afterEach(() => {
    __resetPendingSendsForTests();
    vi.useRealTimers();
  });

  it("is never shown, and every write puts it back as it was stored", () => {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify([NEWER]));
    expect(getPendingSends(SCOPE)).toEqual([]);
    beginPendingSend(SCOPE, send("op-1", "deploy"));
    expect(storedHere()).toEqual([{ ...send("op-1", "deploy"), state: PENDING_SEND_STATE.SENDING }, NEWER]);
    // The last entry this build knows goes; the key stays for the one it does not.
    settlePendingSend(SCOPE, "op-1");
    expect(storedHere()).toEqual([NEWER]);
  });

  it("expires like any entry, and gives way to a write under its own id", () => {
    const expired = { ...NEWER, operationId: "op-expired", submittedAtMs: NOW_MS - PENDING_SEND_TTL_MS };
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify([NEWER, expired]));
    beginPendingSend(SCOPE, send(NEWER.operationId, NEWER.text));
    expect(storedHere()).toEqual([{ ...send(NEWER.operationId, NEWER.text), state: PENDING_SEND_STATE.SENDING }]);
  });

  it("keeps a ledger holding only such entries through the expiry sweep, until they expire", () => {
    window.localStorage.setItem(OTHER_FLEET_KEY, JSON.stringify([NEWER]));
    window.localStorage.setItem(EXPIRED_FLEET_KEY, JSON.stringify([{ ...NEWER, submittedAtMs: NOW_MS - PENDING_SEND_TTL_MS }]));
    getPendingSends(SCOPE);
    expect(storedHere(OTHER_FLEET_KEY)).toEqual([NEWER]);
    expect(window.localStorage.getItem(EXPIRED_FLEET_KEY)).toBeNull();
  });
});
