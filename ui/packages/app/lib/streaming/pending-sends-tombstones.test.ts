import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  PENDING_SEND_STATE,
  __resetPendingSendsForTests,
  beginPendingSend,
  dismissPendingSend,
  failPendingSend,
  findPendingSend,
  getPendingSends,
  settlePendingSend,
  type PendingSend,
} from "./pending-sends";
import { NOW_MS, SCOPE, STORAGE_KEY, otherTabWrote, send, states, stored } from "@/tests/fleet-thread/ledger-fixtures";

// A dismissal is a tombstone every tab keeps until it expires, so a tab that
// had not heard of it cannot write the send back.

const { DISMISSED, SENDING, UNKNOWN } = PENDING_SEND_STATE;
const OP = "op-1";
const LATER_MS = NOW_MS + 1_000;

function entry(state: PendingSend["state"], submittedAtMs: number, text = "deploy"): PendingSend {
  return { operationId: OP, text, state, submittedAtMs };
}

// Another tab's write, landed in storage and reported here.
function otherTabStored(entries: PendingSend[]): void {
  window.localStorage.setItem(STORAGE_KEY, stored(entries));
  otherTabWrote(STORAGE_KEY, stored(entries));
}

function storedHere(): PendingSend[] {
  return JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "[]") as PendingSend[];
}

beforeEach(() => {
  vi.useFakeTimers({ now: NOW_MS, toFake: ["Date"] });
  __resetPendingSendsForTests();
});

afterEach(() => {
  __resetPendingSendsForTests();
  vi.useRealTimers();
});

describe("a dismissed send", () => {
  it("test_dismissed_send_never_returns", () => {
    // This tab resends; another tab dismisses while the POST is out.
    beginPendingSend(SCOPE, send(OP, "deploy"));
    const tombstone = entry(DISMISSED, LATER_MS, "");
    otherTabStored([tombstone]);
    expect(states()).toEqual([[OP, DISMISSED]]);
    expect(findPendingSend(SCOPE, OP)).toBeUndefined();

    failPendingSend(SCOPE, OP, UNKNOWN);
    expect(states()).toEqual([[OP, DISMISSED]]);
    expect(storedHere()).toEqual([tombstone]);
  });

  it("keeps its tombstone, without the text, over another tab's older write", () => {
    beginPendingSend(SCOPE, send(OP, "deploy"));
    failPendingSend(SCOPE, OP, UNKNOWN);
    vi.setSystemTime(LATER_MS);
    dismissPendingSend(SCOPE, OP);
    expect(storedHere()).toEqual([entry(DISMISSED, LATER_MS, "")]);

    otherTabWrote(STORAGE_KEY, stored([entry(UNKNOWN, NOW_MS)]));
    expect(states()).toEqual([[OP, DISMISSED]]);
  });

  it("keeps one tombstone when two tabs dismiss it", () => {
    beginPendingSend(SCOPE, send(OP, "deploy"));
    failPendingSend(SCOPE, OP, UNKNOWN);
    vi.setSystemTime(LATER_MS);
    dismissPendingSend(SCOPE, OP);
    otherTabWrote(STORAGE_KEY, stored([entry(DISMISSED, LATER_MS + 1, "")]));
    expect(getPendingSends(SCOPE)).toEqual([entry(DISMISSED, LATER_MS + 1, "")]);
  });

  it("gives its id back to a send begun after the dismissal, here or in another tab", () => {
    beginPendingSend(SCOPE, send(OP, "deploy"));
    failPendingSend(SCOPE, OP, UNKNOWN);
    vi.setSystemTime(LATER_MS);
    dismissPendingSend(SCOPE, OP);
    otherTabWrote(STORAGE_KEY, stored([entry(SENDING, LATER_MS + 1)]));
    expect(states()).toEqual([[OP, SENDING]]);

    dismissPendingSend(SCOPE, OP);
    vi.setSystemTime(LATER_MS + 2);
    beginPendingSend(SCOPE, send(OP, "deploy", LATER_MS + 2));
    expect(states()).toEqual([[OP, SENDING]]);
  });

  it("is removed, tombstone and all, when the daemon acknowledges the send", () => {
    beginPendingSend(SCOPE, send(OP, "deploy"));
    otherTabStored([entry(DISMISSED, LATER_MS, "")]);
    settlePendingSend(SCOPE, OP);
    expect(getPendingSends(SCOPE)).toEqual([]);
    expect(window.localStorage.getItem(STORAGE_KEY)).toBeNull();
  });
});
