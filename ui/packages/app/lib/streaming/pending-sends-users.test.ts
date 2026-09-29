import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  PENDING_SEND_STATE,
  __resetPendingSendsForTests,
  beginPendingSend,
  failPendingSend,
  getPendingSends,
  subscribePendingSends,
  type LedgerScope,
} from "./pending-sends";
import { NOW_MS, SCOPE, STORAGE_KEY, SUBJECT, send, states } from "@/tests/fleet-thread/ledger-fixtures";

// A shared browser: which user a page shows decides whose ledgers storage may
// hold. Split from `pending-sends.test.ts` at the length cap.

const OTHER_SUBJECT = "user_next_on_this_browser";
const OTHER_FLEET: LedgerScope = { ...SCOPE, fleetId: "fleet_other" };
const OTHER_USER: LedgerScope = { ...SCOPE, subject: OTHER_SUBJECT };
const SIGNED_OUT: LedgerScope = { ...SCOPE, subject: null };
const LEDGER_PREFIX = "agentsfleet:pending-sends";
const READER_KEY = "agentsfleet:ledger-reader";
const unsubscribes: (() => void)[] = [];

// A page showing `scope`'s ledger, subscribed the way useSyncExternalStore does.
function shows(scope: LedgerScope): void {
  unsubscribes.push(subscribePendingSends(scope, () => undefined));
}

beforeEach(() => {
  vi.useFakeTimers({ now: NOW_MS, toFake: ["Date"] });
  __resetPendingSendsForTests();
});

afterEach(() => {
  for (const unsubscribe of unsubscribes.splice(0)) unsubscribe();
  __resetPendingSendsForTests();
  vi.useRealTimers();
});

describe("pending-sends ledger on a shared browser", () => {
  it("keys the ledger by user, and mirrors nothing until the user is known", () => {
    shows(SCOPE);
    beginPendingSend(SCOPE, send("op-1", "mine"));
    failPendingSend(SCOPE, "op-1", PENDING_SEND_STATE.REFUSED);
    // The same person's other fleet sees none of it.
    expect(getPendingSends(OTHER_FLEET)).toEqual([]);

    // A page that does not know its user yet claims nothing and mirrors nothing.
    shows(SIGNED_OUT);
    beginPendingSend(SIGNED_OUT, send("op-2", "early"));
    const mirrored = Object.keys(window.localStorage).filter((key) => key.startsWith(LEDGER_PREFIX));
    expect(mirrored).toEqual([STORAGE_KEY]);
    expect(window.localStorage.getItem(READER_KEY)).toBe(SUBJECT);

    // Nor does the next person on this browser, whose page removes it from
    // storage — in the same document, as a sign-in without a reload.
    shows(OTHER_USER);
    expect(getPendingSends(OTHER_USER)).toEqual([]);
    expect(window.localStorage.getItem(STORAGE_KEY)).toBeNull();
  });

  it("keeps a previous user's late ending out of the storage the next user purged", () => {
    shows(SCOPE);
    beginPendingSend(SCOPE, send("op-late", "still out"));
    shows(OTHER_USER);
    failPendingSend(SCOPE, "op-late", PENDING_SEND_STATE.UNKNOWN);
    expect(window.localStorage.getItem(STORAGE_KEY)).toBeNull();
    expect(states()).toEqual([["op-late", PENDING_SEND_STATE.UNKNOWN]]);
  });

  it("keeps a late ending out when the next user signed in from another tab", () => {
    shows(SCOPE);
    beginPendingSend(SCOPE, send("op-late", "still out"));
    // Another tab's page claims the browser for the next user and purges.
    window.localStorage.setItem(READER_KEY, OTHER_SUBJECT);
    window.localStorage.removeItem(STORAGE_KEY);
    failPendingSend(SCOPE, "op-late", PENDING_SEND_STATE.UNKNOWN);
    expect(window.localStorage.getItem(STORAGE_KEY)).toBeNull();
  });

  it("saves a returning user's sends again after a switch there and back", () => {
    shows(SCOPE);
    shows(OTHER_USER);
    shows(SCOPE);
    beginPendingSend(SCOPE, send("op-back", "after the round trip"));
    expect(window.localStorage.getItem(STORAGE_KEY)).not.toBeNull();
    expect(window.localStorage.getItem(READER_KEY)).toBe(SUBJECT);
  });
});
