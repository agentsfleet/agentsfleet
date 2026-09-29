// The pending-sends suites' shared ledger: one user's fleet, its storage key,
// and the writes another tab makes — plus a stand-in for `navigator.locks`
// that lets a test play the tab holding a send's lock. No test-runner imports.

import { getPendingSends, type LedgerScope, type PendingSend } from "@/lib/streaming/pending-sends";

export const SUBJECT = "user_ledger";
export const SCOPE: LedgerScope = { subject: SUBJECT, workspaceId: "ws_ledger", fleetId: "fleet_ledger" };
export const STORAGE_KEY = `agentsfleet:pending-sends:${SUBJECT}:ws_ledger:fleet_ledger`;
export const NOW_MS = 1_790_553_600_000;

export function send(operationId: string, text: string, submittedAtMs = NOW_MS) {
  return { operationId, text, submittedAtMs };
}

export function stored(entries: PendingSend[]): string {
  return JSON.stringify(entries);
}

export function states(scope: LedgerScope = SCOPE): [string, string][] {
  return getPendingSends(scope).map((entry) => [entry.operationId, entry.state]);
}

// Another tab's write, as the browser reports it to this one.
export function otherTabWrote(key: string | null, newValue: string | null): void {
  window.dispatchEvent(new StorageEvent("storage", { key, newValue }));
}

// Another tab's write, landed in storage and reported here.
export function otherTabStored(entries: PendingSend[], key = STORAGE_KEY): void {
  window.localStorage.setItem(key, stored(entries));
  otherTabWrote(key, stored(entries));
}

/** One origin's locks, granted in request order. `heldElsewhere` is another
 * tab taking a lock; the function it returns lets go, as closing that tab would. */
export function fakeLockManager() {
  const queues = new Map<string, (() => void)[]>();
  const busy = new Set<string>();
  const grantNext = (name: string) => {
    const waiting = queues.get(name)?.shift();
    if (waiting === undefined) busy.delete(name);
    else waiting();
  };
  const request = (name: string, granted: () => unknown): Promise<unknown> =>
    new Promise((resolve, reject) => {
      const run = () => {
        busy.add(name);
        void Promise.resolve().then(granted).then(resolve, reject).finally(() => grantNext(name));
      };
      if (busy.has(name)) queues.set(name, [...(queues.get(name) ?? []), run]);
      else run();
    });
  const heldElsewhere = (name: string): (() => void) => {
    let release = (): void => undefined;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    void request(name, () => held);
    return release;
  };
  return { locks: { request }, heldElsewhere };
}

/** Lets queued lock grants and their callbacks run. */
export function lockTurn(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}
