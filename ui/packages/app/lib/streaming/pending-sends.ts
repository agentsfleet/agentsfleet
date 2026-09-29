// The ledger of a fleet's unresolved sends: every steer from the moment the
// operator presses Send until the daemon's 202 is reconciled, one entry per
// operation id. It is what makes a lost response recoverable — the entry
// carries the id and the text, so Resend posts the same operation and the
// daemon answers the first admission instead of running a second.
//
// Module state, keyed by user, workspace and fleet, the way
// `fleet-stream-registry` holds its entries: it outlives a composer remount
// inside a navigation. A `localStorage` mirror makes it outlive the document
// too — a reload or a second tab reads what this one wrote. Storage is
// best-effort throughout: absent or throwing, the ledger is memory-only.
//
// Three rules keep tabs from losing each other's entries. A write merges with
// what storage holds now, never with this tab's older copy. A storage event
// updates every key this tab holds, whether or not a composer is mounted. And
// a tab never drops its own in-flight send because another tab's write did
// not know about it yet.
//
// Two more keep every send visible to the end, and gone once dismissed.
// Another tab's `sending` is hidden while that tab holds the send's lock and
// reads as unknown once it does not (`pending-sends-locks.ts`). A dismissal is
// a tombstone, not a removal, so a tab that has not heard of it cannot write
// the send back (`tombstoneRank`).
//
// The key names the signed-in user, so the next person on a shared browser
// never sees — or resends as themselves — what the last one typed, and once
// that person's page shows a ledger, the last one's leave storage and stay
// out, in every tab. Nothing is mirrored until the
// user is known, entries expire after a day, and a fleet holds at most
// `MAX_PENDING_SENDS`. The stored shape and every storage call live in
// `pending-sends-storage.ts`.

import {
  MAX_PENDING_SENDS,
  PENDING_SEND_STATE,
  PENDING_SEND_TTL_MS,
  capped,
  keptForeign,
  ledgerKey,
  live,
  mirror,
  parseEntries,
  claimReader,
  purgeOtherUsers,
  readLedger,
  readStored,
  removeMirrored,
  storage,
  storedReader,
  tombstoneRank,
  type ForeignEntry,
  type LedgerScope,
  type PendingSend,
  type PendingSendOutcome,
} from "./pending-sends-storage";
import { SendLocks } from "./pending-sends-locks";

export { MAX_PENDING_SENDS, PENDING_SEND_STATE, PENDING_SEND_TTL_MS, type LedgerScope, type PendingSend, type PendingSendOutcome };

const STORAGE_EVENT = "storage";
const EMPTY: readonly PendingSend[] = Object.freeze([]);

const LEDGERS = new Map<string, readonly PendingSend[]>();
const LOCKS = new SendLocks();
const LISTENERS = new Map<string, Set<() => void>>();
// Operation ids this document has sent and not yet heard back on. Another
// tab's write cannot know about them, so a merge keeps them.
const OWN_IN_FLIGHT = new Set<string>();
// Keys this document mirrors to storage; a memory-only ledger (user not yet
// known) is never re-read from storage, because storage never held it.
const MIRRORED = new Set<string>();
// Per key, the sends this document changed that storage never saw: a write
// refused for quota or a revoked permission. A merge keeps them until a write
// lands — storage would otherwise drop ended sends it never heard end — and
// nothing else, so what other tabs removed stays removed.
const UNSAVED = new Map<string, Set<string>>();
const NONE_UNSAVED: ReadonlySet<string> = new Set();
let storageListening = false;
let swept = false;
// The user this page last showed a ledger for.
let claimedFor: string | null = null;

// This document's first read of a key. An entry still `sending` belongs to
// another document, which may be gone — its POST may or may not have landed.
// With locks, `adopt` asks; without, it reads as unknown here, the state that
// offers a safe Resend.
function hydrate(scope: LedgerScope): readonly PendingSend[] {
  sweepExpired();
  const asked = LOCKS.supported();
  return live(readStored(mirror(scope), ledgerKey(scope)) ?? [], Date.now()).map((entry) =>
    entry.state === PENDING_SEND_STATE.SENDING && !asked ? { ...entry, state: PENDING_SEND_STATE.UNKNOWN } : entry,
  );
}

function read(scope: LedgerScope): readonly PendingSend[] {
  const key = ledgerKey(scope);
  const held = LEDGERS.get(key);
  if (held !== undefined) return held;
  const hydrated = hydrate(scope);
  adopt(key, hydrated);
  if (mirror(scope) !== null) MIRRORED.add(key);
  listenToStorage();
  return hydrated;
}

// Every ledger this document holds goes through here, so each `sending` it did
// not start is watched until its owner lets go.
function adopt(key: string, entries: readonly PendingSend[]): void {
  LEDGERS.set(key, entries);
  for (const entry of entries) {
    if (entry.state !== PENDING_SEND_STATE.SENDING || OWN_IN_FLIGHT.has(entry.operationId)) continue;
    LOCKS.watch(entry.operationId, () => ownerGone(key, entry.operationId));
  }
}

// Nobody holds the send's lock, or nobody can ask. Its owner writes the ending
// before it lets go, but that write's storage event can reach this tab after
// the grant, so storage itself is read first. A send still `sending` there
// never had an ending written, and reads as unknown — in memory only, as a
// reload would, because its owner may yet write the real ending.
function ownerGone(key: string, operationId: string): void {
  const held = LEDGERS.get(key);
  const orphan = held?.find((entry) => entry.operationId === operationId);
  if (held === undefined || orphan?.state !== PENDING_SEND_STATE.SENDING || OWN_IN_FLIGHT.has(operationId)) return;
  // Only a mirrored ledger holds another tab's send. Storage gone since, or
  // throwing, is not an empty ledger: the send is kept and read as unknown.
  const store = storage();
  const stored = store === null ? null : readStored(store, key);
  const current = stored === null ? held : mergeIncoming(live(stored, Date.now()), held, UNSAVED.get(key) ?? NONE_UNSAVED);
  adopt(key, current.map((entry) =>
    entry.operationId === operationId && entry.state === PENDING_SEND_STATE.SENDING ? { ...entry, state: PENDING_SEND_STATE.UNKNOWN } : entry));
  notify(key);
}

// What storage says now, plus this document's own sends it has not seen yet:
// those still in flight, and those whose change never reached storage. Entries
// other tabs removed stay removed. A `sending` this document
// did not start never overrides an ending it already knows: storage keeps the
// `sending` a closed tab left behind, and taking it back would hide that
// send's Resend for good — while showing Resend over a live send is safe,
// because it carries the same operation id.
function mergeIncoming(incoming: readonly PendingSend[], held: readonly PendingSend[], unsaved: ReadonlySet<string>): PendingSend[] {
  const heldById = new Map(held.map((entry) => [entry.operationId, entry]));
  const merged = incoming.map((entry) => {
    const mine = heldById.get(entry.operationId);
    if (mine === undefined) return entry;
    const staleSending = entry.state === PENDING_SEND_STATE.SENDING && mine.state !== PENDING_SEND_STATE.SENDING;
    return tombstoneRank(entry, mine) ?? (staleSending ? mine : entry);
  });
  const known = new Set(incoming.map((entry) => entry.operationId));
  const ownId = (id: string) => OWN_IN_FLIGHT.has(id) || unsaved.has(id);
  const kept = held.filter((entry) => ownId(entry.operationId) && !known.has(entry.operationId));
  return [...merged, ...kept];
}

// A read that throws is not an empty ledger: the merge is skipped, and this
// document's copy is the base.
function mutate(
  scope: LedgerScope,
  operationId: string,
  change: (entries: readonly PendingSend[]) => PendingSend[],
): void {
  const key = ledgerKey(scope);
  const held = read(scope);
  const store = writable(scope);
  const stored = store === null ? null : readLedger(store, key);
  const nowMs = Date.now();
  const base = stored === null ? held : mergeIncoming(live(stored.entries, nowMs), held, UNSAVED.get(key) ?? NONE_UNSAVED);
  const next = capped(change(base));
  adopt(key, next);
  const foreign = keptForeign(stored?.foreign ?? [], next, operationId, nowMs);
  if (store !== null) writeMirror(store, key, operationId, [...next, ...foreign]);
  notify(key);
}

// A send still in flight when the next person signed in — on this page or in
// another tab — ends after their page claimed the browser and purged it. Its
// ending stays in memory: writing it would put the last user's text back in
// the storage the purge just cleared.
function writable(scope: LedgerScope): Storage | null {
  const reader = storedReader() ?? claimedFor;
  return reader !== null && scope.subject !== reader ? null : mirror(scope);
}

function writeMirror(store: Storage, key: string, operationId: string, next: readonly (PendingSend | ForeignEntry)[]): void {
  try {
    if (next.length === 0) store.removeItem(key);
    else store.setItem(key, JSON.stringify(next));
    UNSAVED.delete(key);
  } catch {
    // Quota or a revoked permission: this send's change lives in this tab only.
    UNSAVED.set(key, new Set(UNSAVED.get(key)).add(operationId));
  }
}

function notify(key: string): void {
  for (const listener of LISTENERS.get(key) ?? []) listener();
}

// Another tab wrote a key this one holds: merge its value in. No
// unknown-coercion here — that tab is alive and its `sending` entry is in
// flight; its own settle or fail arrives the same way. A `clear()` names no
// key, so every mirrored ledger this document holds is re-read.
function onStorage(event: StorageEvent): void {
  const keys = event.key === null ? [...MIRRORED] : [event.key];
  const nowMs = Date.now();
  for (const key of keys) {
    const held = MIRRORED.has(key) ? LEDGERS.get(key) : undefined;
    if (held === undefined) continue;
    const incoming = event.key === null ? readStored(storage(), key) : parseEntries(event.newValue);
    // A read that throws is not an empty ledger, here as in `mutate`.
    if (incoming === null) continue;
    adopt(key, mergeIncoming(live(incoming, nowMs), held, UNSAVED.get(key) ?? NONE_UNSAVED));
    notify(key);
  }
}

function listenToStorage(): void {
  const target = (globalThis as { window?: Window }).window;
  if (target === undefined || storageListening) return;
  storageListening = true;
  target.addEventListener(STORAGE_EVENT, onStorage);
}

// Once per document: remove mirrored ledgers whose every entry has expired —
// fleets never revisited, whoever's they were.
function sweepExpired(): void {
  if (swept) return;
  swept = true;
  const nowMs = Date.now();
  removeMirrored((store, key) => {
    const stored = readLedger(store, key);
    return stored === null || live([...stored.entries, ...stored.foreign], nowMs).length === 0;
  });
}

// The page shows `subject`'s ledger. On a change of user — a first render, or
// a sign-in that swaps the user without a reload, either way round — the
// browser's reader becomes them and every other user's ledgers leave storage.
function claim(subject: string): void {
  if (claimedFor === subject) return;
  claimedFor = subject;
  claimReader(subject);
  purgeOtherUsers(subject);
}

export function getPendingSends(scope: LedgerScope): readonly PendingSend[] {
  return read(scope);
}

export function subscribePendingSends(scope: LedgerScope, listener: () => void): () => void {
  if (scope.subject !== null) claim(scope.subject);
  const key = ledgerKey(scope);
  const listeners = LISTENERS.get(key) ?? new Set<() => void>();
  listeners.add(listener);
  LISTENERS.set(key, listeners);
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) LISTENERS.delete(key);
  };
}

/** A send leaves: as `sending`, or back to `sending` when it is sent again.
 * Its lock is asked for before the entry is written, so no other tab looks
 * before the request is queued. */
export function beginPendingSend(scope: LedgerScope, send: Omit<PendingSend, "state">): void {
  OWN_IN_FLIGHT.add(send.operationId);
  LOCKS.hold(send.operationId);
  const entry: PendingSend = { ...send, state: PENDING_SEND_STATE.SENDING };
  mutate(scope, send.operationId, (entries) => [...entries.filter((held) => held.operationId !== send.operationId), entry]);
}

/** The daemon acknowledged it: nothing is left to recover, tombstone included. */
export function settlePendingSend(scope: LedgerScope, operationId: string): void {
  end(scope, operationId, (entries) => entries.filter((entry) => entry.operationId !== operationId));
}

/** The operator gave up on it: a tombstone, stamped now and without its text,
 * that expires a day later like any entry. */
export function dismissPendingSend(scope: LedgerScope, operationId: string): void {
  const tombstone: PendingSend = { operationId, text: "", state: PENDING_SEND_STATE.DISMISSED, submittedAtMs: Date.now() };
  end(scope, operationId, (entries) => entries.map((entry) => (entry.operationId === operationId ? tombstone : entry)));
}

/** How the send ended without an acknowledgement — unless it was dismissed
 * meanwhile, in this tab or another, and then it stays dismissed. */
export function failPendingSend(scope: LedgerScope, operationId: string, state: PendingSendOutcome): void {
  end(scope, operationId, (entries) => entries.map((entry) =>
    entry.operationId === operationId && entry.state !== PENDING_SEND_STATE.DISMISSED ? { ...entry, state } : entry));
}

// A send's ending, written while the send is still this document's own — a
// concurrent write from a tab that never saw it must not merge it away — and
// its lock let go only after, so a tab granted the lock reads the ending.
function end(scope: LedgerScope, operationId: string, change: (entries: readonly PendingSend[]) => PendingSend[]): void {
  mutate(scope, operationId, change);
  OWN_IN_FLIGHT.delete(operationId);
  LOCKS.release(operationId);
}

/** One entry an operator can act on, by operation id — read from the ledger,
 * not from a render. A tombstone is not one. */
export function findPendingSend(scope: LedgerScope, operationId: string): PendingSend | undefined {
  return read(scope).find((entry) => entry.operationId === operationId && entry.state !== PENDING_SEND_STATE.DISMISSED);
}

/** The snapshot a server render reads: nothing, because no browser wrote here. */
export const NO_PENDING_SENDS = EMPTY;

// Test surface — vitest must reset between tests; nothing in production
// should call this. `keepStorage` simulates a fresh document over what the
// last one mirrored.
export function __resetPendingSendsForTests({ keepStorage = false }: { keepStorage?: boolean } = {}): void {
  LEDGERS.clear();
  LOCKS.clear();
  OWN_IN_FLIGHT.clear();
  MIRRORED.clear();
  UNSAVED.clear();
  swept = false;
  claimedFor = null;
  if (!keepStorage) {
    removeMirrored(() => true);
    claimReader(null);
  }
  for (const key of LISTENERS.keys()) notify(key);
}
