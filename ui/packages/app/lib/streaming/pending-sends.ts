// The ledger of a fleet's unresolved sends: every steer from the moment the
// operator presses Send until the daemon's 202 is reconciled, one entry per
// operation id. It is what makes a lost response recoverable — the entry
// carries the id and the text, so Resend posts the same operation and the
// daemon answers the first admission instead of running a second.
//
// Module state, keyed by workspace and fleet, the way `fleet-stream-registry`
// holds its entries: it outlives a composer remount inside a navigation. A
// `localStorage` mirror makes it outlive the document too — a reload or a
// second tab reads what this one wrote — and the `storage` event keeps open
// tabs in step. Storage is best-effort throughout: absent or throwing, the
// ledger is memory-only and nothing else changes.

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

export type PendingSend = {
  operationId: string;
  text: string;
  state: PendingSendState;
  submittedAtMs: number;
};

const STORAGE_KEY_PREFIX = "agentsfleet:pending-sends";
const KEY_SEPARATOR = ":";
const STORAGE_EVENT = "storage";
const STATES: ReadonlySet<string> = new Set(Object.values(PENDING_SEND_STATE));
const EMPTY: readonly PendingSend[] = Object.freeze([]);

const LEDGERS = new Map<string, readonly PendingSend[]>();
const LISTENERS = new Map<string, Set<() => void>>();
let storageListening = false;

function ledgerKey(workspaceId: string, fleetId: string): string {
  return [STORAGE_KEY_PREFIX, workspaceId, fleetId].join(KEY_SEPARATOR);
}

// `localStorage` can be absent or throw in locked-down privacy modes, so every
// mirror operation is best-effort despite the non-null lib.dom type.
function withStorage<T>(fn: (store: Storage) => T): T | null {
  if (typeof window === "undefined") return null;
  try {
    const store = (window as { localStorage?: Storage }).localStorage;
    return store ? fn(store) : null;
  } catch {
    return null;
  }
}

// Parsed, not trusted: another tab or an older build wrote the value.
function parseEntries(raw: string | null): PendingSend[] {
  if (raw === null) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter(isPendingSend) : [];
  } catch {
    return [];
  }
}

function isPendingSend(value: unknown): value is PendingSend {
  if (typeof value !== "object" || value === null) return false;
  const entry = value as Record<string, unknown>;
  return typeof entry.operationId === "string"
    && typeof entry.text === "string"
    && typeof entry.state === "string" && STATES.has(entry.state)
    && typeof entry.submittedAtMs === "number";
}

// This document's first read of a key. An entry still `sending` was owned by a
// document that is gone — its POST may or may not have landed — so it reads as
// unknown here, which is the state that offers a safe Resend.
function hydrate(key: string): readonly PendingSend[] {
  const stored = parseEntries(withStorage((store) => store.getItem(key)));
  return stored.map((entry) =>
    entry.state === PENDING_SEND_STATE.SENDING ? { ...entry, state: PENDING_SEND_STATE.UNKNOWN } : entry,
  );
}

function read(key: string): readonly PendingSend[] {
  const held = LEDGERS.get(key);
  if (held !== undefined) return held;
  const hydrated = hydrate(key);
  LEDGERS.set(key, hydrated);
  return hydrated;
}

function write(key: string, next: readonly PendingSend[]): void {
  LEDGERS.set(key, next);
  withStorage((store) => {
    if (next.length === 0) store.removeItem(key);
    else store.setItem(key, JSON.stringify(next));
  });
  notify(key);
}

function notify(key: string): void {
  for (const listener of LISTENERS.get(key) ?? []) listener();
}

// Another tab wrote a key this one reads: take its value as it stands. No
// unknown-coercion here — that tab is alive and its `sending` entry is in
// flight; its own settle or fail arrives the same way.
function onStorage(event: StorageEvent): void {
  const { key } = event;
  if (key === null || !key.startsWith(STORAGE_KEY_PREFIX) || !LISTENERS.has(key)) return;
  LEDGERS.set(key, parseEntries(event.newValue));
  notify(key);
}

function listenToStorage(listening: boolean): void {
  if (typeof window === "undefined" || storageListening === listening) return;
  storageListening = listening;
  if (listening) window.addEventListener(STORAGE_EVENT, onStorage);
  else window.removeEventListener(STORAGE_EVENT, onStorage);
}

export function getPendingSends(workspaceId: string, fleetId: string): readonly PendingSend[] {
  return read(ledgerKey(workspaceId, fleetId));
}

export function subscribePendingSends(workspaceId: string, fleetId: string, listener: () => void): () => void {
  const key = ledgerKey(workspaceId, fleetId);
  const listeners = LISTENERS.get(key) ?? new Set<() => void>();
  listeners.add(listener);
  LISTENERS.set(key, listeners);
  listenToStorage(true);
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) LISTENERS.delete(key);
    if (LISTENERS.size === 0) listenToStorage(false);
  };
}

/** A send leaves: as `sending`, or back to `sending` when it is sent again. */
export function beginPendingSend(
  workspaceId: string,
  fleetId: string,
  send: Omit<PendingSend, "state">,
): void {
  const key = ledgerKey(workspaceId, fleetId);
  const entry: PendingSend = { ...send, state: PENDING_SEND_STATE.SENDING };
  const others = read(key).filter((held) => held.operationId !== send.operationId);
  write(key, [...others, entry]);
}

/** The daemon acknowledged it: nothing is left to recover. */
export function settlePendingSend(workspaceId: string, fleetId: string, operationId: string): void {
  remove(ledgerKey(workspaceId, fleetId), operationId);
}

/** The operator gave up on it. */
export function dismissPendingSend(workspaceId: string, fleetId: string, operationId: string): void {
  remove(ledgerKey(workspaceId, fleetId), operationId);
}

/** How the send ended without an acknowledgement. */
export function failPendingSend(
  workspaceId: string,
  fleetId: string,
  operationId: string,
  state: Exclude<PendingSendState, typeof PENDING_SEND_STATE.SENDING>,
): void {
  const key = ledgerKey(workspaceId, fleetId);
  write(key, read(key).map((entry) => (entry.operationId === operationId ? { ...entry, state } : entry)));
}

/** The unresolved send whose text is exactly `text`, if any. */
export function findPendingSendByText(workspaceId: string, fleetId: string, text: string): PendingSend | undefined {
  return read(ledgerKey(workspaceId, fleetId)).find((entry) => entry.text === text);
}

function remove(key: string, operationId: string): void {
  const held = read(key);
  const next = held.filter((entry) => entry.operationId !== operationId);
  if (next.length !== held.length) write(key, next);
}

/** The snapshot a server render reads: nothing, because no browser wrote here. */
export const NO_PENDING_SENDS = EMPTY;

// Test surface — vitest must reset between tests; nothing in production
// should call this.
export function __resetPendingSendsForTests(): void {
  LEDGERS.clear();
  withStorage((store) => {
    for (const key of Array.from({ length: store.length }, (_, index) => store.key(index))) {
      if (key !== null && key.startsWith(STORAGE_KEY_PREFIX)) store.removeItem(key);
    }
  });
  for (const key of LISTENERS.keys()) notify(key);
}
