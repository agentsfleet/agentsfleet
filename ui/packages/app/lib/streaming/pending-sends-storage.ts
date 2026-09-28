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
} as const;

type PendingSendState = (typeof PENDING_SEND_STATE)[keyof typeof PENDING_SEND_STATE];

/** How a send ended without an acknowledgement. */
export type PendingSendOutcome = Exclude<PendingSendState, typeof PENDING_SEND_STATE.SENDING>;

// The stored shape, parsed rather than trusted.
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
export function readStored(store: Storage | null, key: string): PendingSend[] | null {
  if (store === null) return [];
  try {
    return parseEntries(store.getItem(key));
  } catch {
    return null;
  }
}

// A malformed entry is dropped on its own; a value that is not a list, or not
// JSON at all, reads as an empty ledger.
export function parseEntries(raw: string | null): PendingSend[] {
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
