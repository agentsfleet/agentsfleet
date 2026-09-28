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
// The key names the signed-in user, so the next person on a shared browser
// never sees — or resends as themselves — what the last one typed. Nothing is
// mirrored until the user is known, entries expire after a day, and a fleet
// holds at most `MAX_PENDING_SENDS`.

import { z } from "zod";

export const PENDING_SEND_STATE = {
  /** The POST is out; the optimistic row already shows the message. */
  SENDING: "sending",
  /** The server answered no. */
  REFUSED: "refused",
  /** The server answered 401. */
  SESSION: "session",
  /** Nothing answered: the transport failed, or the document that sent it is gone. */
  UNKNOWN: "unknown",
} as const;

export type PendingSendState = (typeof PENDING_SEND_STATE)[keyof typeof PENDING_SEND_STATE];

/** How a send ended without an acknowledgement. */
export type PendingSendOutcome = Exclude<PendingSendState, typeof PENDING_SEND_STATE.SENDING>;

// The stored shape, parsed rather than trusted: another tab or an older build
// wrote it.
const PendingSendSchema = z.object({
  operationId: z.string(),
  text: z.string(),
  state: z.enum([
    PENDING_SEND_STATE.SENDING,
    PENDING_SEND_STATE.REFUSED,
    PENDING_SEND_STATE.SESSION,
    PENDING_SEND_STATE.UNKNOWN,
  ]),
  submittedAtMs: z.number(),
});

export type PendingSend = z.infer<typeof PendingSendSchema>;

/** Whose ledger, for which fleet. `subject` is null until the user is known. */
export type LedgerScope = {
  subject: string | null;
  workspaceId: string;
  fleetId: string;
};

const STORAGE_KEY_PREFIX = "agentsfleet:pending-sends";
const KEY_SEPARATOR = ":";
const STORAGE_EVENT = "storage";
const MS_PER_HOUR = 3_600_000;
const HOURS_PER_DAY = 24;
/** How long an unresolved send stays recoverable. */
export const PENDING_SEND_TTL_MS = HOURS_PER_DAY * MS_PER_HOUR;
/** The most unresolved sends one fleet keeps; the oldest settled-state ones go first. */
export const MAX_PENDING_SENDS = 20;
const EMPTY: readonly PendingSend[] = Object.freeze([]);

const LEDGERS = new Map<string, readonly PendingSend[]>();
const LISTENERS = new Map<string, Set<() => void>>();
// Operation ids this document has sent and not yet heard back on. Another
// tab's write cannot know about them, so a merge keeps them.
const OWN_IN_FLIGHT = new Set<string>();
// Keys this document mirrors to storage; a memory-only ledger (user not yet
// known) is never re-read from storage, because storage never held it.
const MIRRORED = new Set<string>();
let storageListening = false;
let swept = false;

function ledgerKey({ subject, workspaceId, fleetId }: LedgerScope): string {
  return [STORAGE_KEY_PREFIX, subject ?? "", workspaceId, fleetId].join(KEY_SEPARATOR);
}

// `localStorage` can be absent or throw in locked-down privacy modes, so every
// mirror operation is best-effort despite the non-null lib.dom type.
function storage(): Storage | null {
  try {
    return (globalThis as { window?: { localStorage?: Storage } }).window?.localStorage ?? null;
  } catch {
    return null;
  }
}

/** The mirror for this scope: none until the user is known. */
function mirror(scope: LedgerScope): Storage | null {
  return scope.subject === null ? null : storage();
}

function readStored(store: Storage | null, key: string): PendingSend[] {
  if (store === null) return [];
  try {
    return parseEntries(store.getItem(key));
  } catch {
    return [];
  }
}

// A malformed entry is dropped on its own; a value that is not a list, or not
// JSON at all, reads as an empty ledger.
function parseEntries(raw: string | null): PendingSend[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw ?? "[]");
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  return parsed.flatMap((value) => {
    const entry = PendingSendSchema.safeParse(value);
    return entry.success ? [entry.data] : [];
  });
}

function live(entries: readonly PendingSend[], nowMs: number): PendingSend[] {
  return entries.filter((entry) => nowMs - entry.submittedAtMs < PENDING_SEND_TTL_MS);
}

// This document's first read of a key. An entry still `sending` was owned by a
// document that is gone — its POST may or may not have landed — so it reads as
// unknown here, which is the state that offers a safe Resend.
function hydrate(scope: LedgerScope): readonly PendingSend[] {
  sweepExpired();
  return live(readStored(mirror(scope), ledgerKey(scope)), Date.now()).map((entry) =>
    entry.state === PENDING_SEND_STATE.SENDING ? { ...entry, state: PENDING_SEND_STATE.UNKNOWN } : entry,
  );
}

function read(scope: LedgerScope): readonly PendingSend[] {
  const key = ledgerKey(scope);
  const held = LEDGERS.get(key);
  if (held !== undefined) return held;
  const hydrated = hydrate(scope);
  LEDGERS.set(key, hydrated);
  if (mirror(scope) !== null) MIRRORED.add(key);
  listenToStorage();
  return hydrated;
}

// What storage says now, plus this document's own in-flight sends it has not
// seen yet. Entries other tabs removed stay removed. A `sending` this document
// did not start never overrides an ending it already knows: storage keeps the
// `sending` a closed tab left behind, and taking it back would hide that
// send's Resend for good — while showing Resend over a live send is safe,
// because it carries the same operation id.
function mergeIncoming(incoming: readonly PendingSend[], held: readonly PendingSend[]): PendingSend[] {
  const heldById = new Map(held.map((entry) => [entry.operationId, entry]));
  const merged = incoming.map((entry) => {
    const mine = heldById.get(entry.operationId);
    const staleSending = entry.state === PENDING_SEND_STATE.SENDING
      && mine !== undefined && mine.state !== PENDING_SEND_STATE.SENDING;
    return staleSending ? mine : entry;
  });
  const known = new Set(incoming.map((entry) => entry.operationId));
  const ownMissing = held.filter((entry) => OWN_IN_FLIGHT.has(entry.operationId) && !known.has(entry.operationId));
  return [...merged, ...ownMissing];
}

function mutate(scope: LedgerScope, change: (entries: readonly PendingSend[]) => PendingSend[]): void {
  const key = ledgerKey(scope);
  const held = read(scope);
  const store = mirror(scope);
  const base = store === null ? held : mergeIncoming(live(readStored(store, key), Date.now()), held);
  const next = capped(change(base));
  LEDGERS.set(key, next);
  if (store !== null) {
    try {
      if (next.length === 0) store.removeItem(key);
      else store.setItem(key, JSON.stringify(next));
    } catch {
      // Quota or a revoked permission: the ledger stays in memory for this tab.
    }
  }
  notify(key);
}

// The newest `MAX_PENDING_SENDS`, never dropping a send still in flight.
function capped(entries: PendingSend[]): PendingSend[] {
  const excess = entries.length - MAX_PENDING_SENDS;
  if (excess <= 0) return entries;
  const droppable = entries
    .filter((entry) => entry.state !== PENDING_SEND_STATE.SENDING)
    .slice(0, excess)
    .map((entry) => entry.operationId);
  const dropped = new Set(droppable);
  return entries.filter((entry) => !dropped.has(entry.operationId));
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
    LEDGERS.set(key, mergeIncoming(live(incoming, nowMs), held));
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
// fleets never revisited, and users who signed out on this browser.
function sweepExpired(): void {
  if (swept) return;
  swept = true;
  const nowMs = Date.now();
  removeMirrored((store, key) => live(readStored(store, key), nowMs).length === 0);
}

// Removes every mirrored ledger `doomed` names. Best-effort, like every other
// mirror operation.
function removeMirrored(doomed: (store: Storage, key: string) => boolean): void {
  const store = storage();
  if (store === null) return;
  try {
    const keys = Array.from({ length: store.length }, (_, index) => store.key(index))
      .filter((key): key is string => key !== null && key.startsWith(STORAGE_KEY_PREFIX));
    for (const key of keys) {
      if (doomed(store, key)) store.removeItem(key);
    }
  } catch {
    // A storage that stops answering mid-sweep keeps what it holds.
  }
}

export function getPendingSends(scope: LedgerScope): readonly PendingSend[] {
  return read(scope);
}

export function subscribePendingSends(scope: LedgerScope, listener: () => void): () => void {
  const key = ledgerKey(scope);
  const listeners = LISTENERS.get(key) ?? new Set<() => void>();
  listeners.add(listener);
  LISTENERS.set(key, listeners);
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) LISTENERS.delete(key);
  };
}

/** A send leaves: as `sending`, or back to `sending` when it is sent again. */
export function beginPendingSend(scope: LedgerScope, send: Omit<PendingSend, "state">): void {
  OWN_IN_FLIGHT.add(send.operationId);
  const entry: PendingSend = { ...send, state: PENDING_SEND_STATE.SENDING };
  mutate(scope, (entries) => [...entries.filter((held) => held.operationId !== send.operationId), entry]);
}

/** The daemon acknowledged it: nothing is left to recover. */
export function settlePendingSend(scope: LedgerScope, operationId: string): void {
  OWN_IN_FLIGHT.delete(operationId);
  mutate(scope, (entries) => entries.filter((entry) => entry.operationId !== operationId));
}

/** The operator gave up on it. */
export function dismissPendingSend(scope: LedgerScope, operationId: string): void {
  OWN_IN_FLIGHT.delete(operationId);
  mutate(scope, (entries) => entries.filter((entry) => entry.operationId !== operationId));
}

/** How the send ended without an acknowledgement. */
export function failPendingSend(scope: LedgerScope, operationId: string, state: PendingSendOutcome): void {
  // Still this document's own while the ending is written: a concurrent write
  // from another tab that never saw this send must not merge it away.
  mutate(scope, (entries) => entries.map((entry) => (entry.operationId === operationId ? { ...entry, state } : entry)));
  OWN_IN_FLIGHT.delete(operationId);
}

/** One entry by operation id — read from the ledger, not from a render. */
export function findPendingSend(scope: LedgerScope, operationId: string): PendingSend | undefined {
  return read(scope).find((entry) => entry.operationId === operationId);
}

/**
 * The failed send whose text is exactly `text`, if any. A send still in flight
 * never matches: the same words typed again while the first is out are a
 * second message, and must get their own operation id.
 */
export function findUnresolvedSendByText(scope: LedgerScope, text: string): PendingSend | undefined {
  return read(scope).find((entry) => entry.text === text && entry.state !== PENDING_SEND_STATE.SENDING);
}

/** The snapshot a server render reads: nothing, because no browser wrote here. */
export const NO_PENDING_SENDS = EMPTY;

// Test surface — vitest must reset between tests; nothing in production
// should call this. `keepStorage` simulates a fresh document over what the
// last one mirrored.
export function __resetPendingSendsForTests({ keepStorage = false }: { keepStorage?: boolean } = {}): void {
  LEDGERS.clear();
  OWN_IN_FLIGHT.clear();
  MIRRORED.clear();
  swept = false;
  if (!keepStorage) removeMirrored(() => true);
  for (const key of LISTENERS.keys()) notify(key);
}
