// The stored shape of a pending send, and the `localStorage` mirror that lets
// the ledger in `pending-sends.ts` outlive a document. Every operation here is
// best-effort: storage can be absent or throw in locked-down privacy modes, and
// what it holds was written by another tab or an older build.

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
  /** The server refused the operation id itself: it already names another
   * message, so no Resend under it can ever land. */
  CONFLICT: "conflict",
  /** The operator dismissed it. A tombstone, text dropped, that outranks every
   * older state for its id in every tab until it expires. */
  DISMISSED: "dismissed",
} as const;

/** How a send ended without an acknowledgement. */
export type PendingSendOutcome =
  | typeof PENDING_SEND_STATE.REFUSED
  | typeof PENDING_SEND_STATE.SESSION
  | typeof PENDING_SEND_STATE.UNKNOWN
  | typeof PENDING_SEND_STATE.CONFLICT;

// The stored shape, parsed rather than trusted.
const PendingSendSchema = z.object({
  operationId: z.string(),
  text: z.string(),
  state: z.enum([
    PENDING_SEND_STATE.SENDING,
    PENDING_SEND_STATE.REFUSED,
    PENDING_SEND_STATE.SESSION,
    PENDING_SEND_STATE.UNKNOWN,
    PENDING_SEND_STATE.CONFLICT,
    PENDING_SEND_STATE.DISMISSED,
  ]),
  submittedAtMs: z.number(),
});

export type PendingSend = z.infer<typeof PendingSendSchema>;

// An entry in the stored shape whose `state` this build does not know: a newer
// build wrote it. It is never shown, and every write puts it back as it was
// read, so the build that knows it finds it after this one wrote the key.
const KNOWN_STATES: ReadonlySet<string> = new Set(Object.values(PENDING_SEND_STATE));
const ForeignEntrySchema = z.object({
  operationId: z.string(),
  state: z.string().refine((state) => !KNOWN_STATES.has(state)),
  submittedAtMs: z.number(),
});
export type ForeignEntry = z.infer<typeof ForeignEntrySchema> & Readonly<Record<string, unknown>>;

/** What one stored key holds: the entries this build reads, and the ones it keeps. */
export type StoredLedger = { entries: PendingSend[]; foreign: ForeignEntry[] };

const MS_PER_HOUR = 3_600_000;
const HOURS_PER_DAY = 24;
/** How long an unresolved send stays recoverable. */
export const PENDING_SEND_TTL_MS = HOURS_PER_DAY * MS_PER_HOUR;
/** The most unresolved sends one fleet keeps; the oldest ended ones go first,
 * and tombstones only after every other ended send. */
export const MAX_PENDING_SENDS = 20;

export function live<T extends { submittedAtMs: number }>(entries: readonly T[], nowMs: number): T[] {
  return entries.filter((entry) => nowMs - entry.submittedAtMs < PENDING_SEND_TTL_MS);
}

// The newest `MAX_PENDING_SENDS`, never dropping a send still in flight. A
// tombstone goes last: dropped, it would let a dismissed send's late ending
// write the send back.
export function capped(entries: PendingSend[]): PendingSend[] {
  const excess = entries.length - MAX_PENDING_SENDS;
  if (excess <= 0) return entries;
  const ended = entries.filter((entry) => entry.state !== PENDING_SEND_STATE.SENDING);
  const byEviction = [
    ...ended.filter((entry) => entry.state !== PENDING_SEND_STATE.DISMISSED),
    ...ended.filter((entry) => entry.state === PENDING_SEND_STATE.DISMISSED),
  ];
  const dropped = new Set(byEviction.slice(0, excess).map((entry) => entry.operationId));
  return entries.filter((entry) => !dropped.has(entry.operationId));
}

/** Whose ledger, for which fleet. `subject` is null until the user is known. */
export type LedgerScope = {
  subject: string | null;
  workspaceId: string;
  fleetId: string;
};

const STORAGE_KEY_PREFIX = "agentsfleet:pending-sends";
const KEY_SEPARATOR = ":";

export function ledgerKey({ subject, workspaceId, fleetId }: LedgerScope): string {
  return [STORAGE_KEY_PREFIX, subject ?? "", workspaceId, fleetId].join(KEY_SEPARATOR);
}

// Best-effort despite the non-null lib.dom type.
export function storage(): Storage | null {
  try {
    return (globalThis as { window?: { localStorage?: Storage } }).window?.localStorage ?? null;
  } catch {
    return null;
  }
}

/** The mirror for this scope: none until the user is known. */
export function mirror(scope: LedgerScope): Storage | null {
  return scope.subject === null ? null : storage();
}

/** What storage holds for `key`, or null when the read itself threw. */
export function readLedger(store: Storage | null, key: string): StoredLedger | null {
  if (store === null) return { entries: [], foreign: [] };
  try {
    return parseLedger(store.getItem(key));
  } catch {
    return null;
  }
}

/** The entries this build reads at `key`, or null when the read threw. */
export function readStored(store: Storage | null, key: string): PendingSend[] | null {
  return readLedger(store, key)?.entries ?? null;
}

export function parseEntries(raw: string | null): PendingSend[] {
  return parseLedger(raw).entries;
}

// A malformed entry is dropped on its own; a value that is not a list, or not
// JSON at all, reads as an empty ledger. A foreign entry is kept as it was
// stored, not as the schema would rebuild it.
function parseLedger(raw: string | null): StoredLedger {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw ?? "[]");
  } catch {
    return { entries: [], foreign: [] };
  }
  const ledger: StoredLedger = { entries: [], foreign: [] };
  if (!Array.isArray(parsed)) return ledger;
  for (const value of parsed) {
    const entry = PendingSendSchema.safeParse(value);
    if (entry.success) ledger.entries.push(entry.data);
    else if (ForeignEntrySchema.safeParse(value).success) ledger.foreign.push(value as ForeignEntry);
  }
  return ledger;
}

/** The foreign entries a write keeps: live, and not the id it wrote or holds. */
export function keptForeign(foreign: readonly ForeignEntry[], written: readonly PendingSend[], operationId: string, nowMs: number): ForeignEntry[] {
  const ours = new Set(written.map((entry) => entry.operationId)).add(operationId);
  return live(foreign, nowMs).filter((entry) => !ours.has(entry.operationId));
}

// Removes every mirrored ledger `doomed` names.
export function removeMirrored(doomed: (store: Storage, key: string) => boolean): void {
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

// A shared browser: every ledger another user left here goes, so the unsent
// text of whoever signed in before is not in storage for this user to read.
// The key's trailing separator keeps `user_a` from matching `user_ab`.
export function purgeOtherUsers(subject: string): void {
  const own = [STORAGE_KEY_PREFIX, subject, ""].join(KEY_SEPARATOR);
  removeMirrored((_, key) => !key.startsWith(own));
}

// The user whose page last showed a ledger in this browser, shared by every
// tab. Outside `STORAGE_KEY_PREFIX` on purpose: a sweep or a purge reads every
// key under that prefix as a ledger, and this one is not. Clerk keeps one
// session per browser, so there is one such user at a time.
const READER_KEY = "agentsfleet:ledger-reader";

export function storedReader(): string | null {
  try {
    return storage()?.getItem(READER_KEY) ?? null;
  } catch {
    return null;
  }
}

export function claimReader(subject: string | null): void {
  try {
    if (subject === null) storage()?.removeItem(READER_KEY);
    else storage()?.setItem(READER_KEY, subject);
  } catch {
    // A storage that refuses the claim leaves each tab its own view.
  }
}

// A dismissal against any other state for one id: the later of the two by
// `submittedAtMs`, the tombstone on a tie. A tombstone therefore outranks every
// state its dismissal saw — a Resend another tab had in flight ends hidden —
// and only a send begun after it, which an operator started on purpose, takes
// the id back. `null` when neither is a tombstone.
export function tombstoneRank(incoming: PendingSend, mine: PendingSend): PendingSend | null {
  const dismissed = PENDING_SEND_STATE.DISMISSED;
  if (incoming.state !== dismissed && mine.state !== dismissed) return null;
  const [tombstone, other] = incoming.state === dismissed ? [incoming, mine] : [mine, incoming];
  return other.state !== dismissed && other.submittedAtMs > tombstone.submittedAtMs ? other : tombstone;
}
