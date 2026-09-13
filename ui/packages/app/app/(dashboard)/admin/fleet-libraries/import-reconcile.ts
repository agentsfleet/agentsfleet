import type { PlatformCatalogEntry } from "@/lib/types";

/*
 * A timeout is not an answer, and the dialog used to treat it as one.
 *
 * `ONBOARD_BUNDLE_TIMEOUT_MS` is the client's patience, not the daemon's
 * deadline. When it expires we stop listening; the import does not stop
 * running. It goes on to fetch the tarball, validate the bundle, write the
 * canonical tar to object storage and upsert the catalog row, and it does all
 * of that after the operator has already been told it failed. The next page
 * load shows the fleet that was supposed to have not been imported.
 *
 * So on a timeout — and only on a timeout — the catalog is asked, and the
 * answer comes from the row rather than from our own patience.
 *
 * Every comparison here is server value against server value. Both the bundle
 * hash and the row timestamp are written by the import and by nothing else, so
 * neither the browser's clock nor its idea of "now" can decide the outcome.
 */

/** What the catalog held for one repository at one moment. */
export type RepoImportState = {
  present: boolean;
  /** `null` is a row that exists but has never carried a bundle. */
  contentHash: string | null;
  /** Server-written millisecond stamp; `null` only when the row is absent. */
  updatedAt: number | null;
};

export const REPO_ABSENT: RepoImportState = {
  present: false,
  contentHash: null,
  updatedAt: null,
};

/**
 * How long to keep asking, and how often.
 *
 * One read was not enough: it fires the instant our patience expires, which is
 * exactly when the import is most likely to be inside its last few writes. A
 * read that arrives before the upsert reports a failure the catalog contradicts
 * a second later. Four reads spaced over roughly four and a half seconds cover
 * that window without holding the dialog open long enough to feel hung.
 */
export const RECONCILE_ATTEMPTS = 4;
export const RECONCILE_INTERVAL_MS = 1_500;

/**
 * The catalog's state for one repository.
 *
 * A row with no bundle reads as present with a null hash, which is a real and
 * distinct state: `content_hash IS NULL` is what a row looks like when an
 * earlier import created it and never finished. Collapsing it into absent
 * would let that stale row pass as this import's success.
 */
export function repoImportState(
  entries: readonly PlatformCatalogEntry[],
  sourceRepo: string,
): RepoImportState {
  const entry = entries.find((candidate) => candidate.source_repo === sourceRepo);
  if (!entry) return REPO_ABSENT;
  return { present: true, contentHash: entry.content_hash, updatedAt: entry.updated_at };
}

/**
 * Did the import land after we stopped waiting for it?
 *
 * Three ways to answer no, and each is a state the catalog can really be in:
 * no row for this repository, a row that still carries no bundle, and a row
 * that is byte-for-byte and stamp-for-stamp the one we submitted against.
 *
 * A changed bundle hash is the plain case. The hard one is a refetch of a
 * branch that has not moved: the import runs to completion and writes the same
 * hash it found, so the hash alone would call every such refetch a failure. The
 * row timestamp settles it, because `core.fleet_library`'s upsert sets
 * `updated_at = EXCLUDED.updated_at` on its DO UPDATE arm with no equality
 * guard — a landed import always advances it, an import that never reached the
 * upsert never does.
 *
 * The baseline must be read at submit time for this to hold. A snapshot taken
 * when the page rendered can be minutes stale, and any foreign write in between
 * would advance the stamp and read as this import landing; `AddFleetDialog`
 * takes its before state immediately before the onboard call for that reason.
 *
 * The residual gap is a foreign write during the import itself, which no
 * baseline can exclude. Closing that needs an operation id the onboard endpoint
 * does not yet return.
 */
export function importLanded(before: RepoImportState, after: RepoImportState): boolean {
  if (!after.present || after.contentHash === null) return false;
  if (!before.present || before.contentHash === null) return true;
  if (after.contentHash !== before.contentHash) return true;
  return (
    after.updatedAt !== null && before.updatedAt !== null && after.updatedAt > before.updatedAt
  );
}

/**
 * Ask the catalog until it answers or we run out of attempts.
 *
 * `readState` returns `null` for a read that itself failed — an unanswered
 * question is not a yes, but neither is it a no, so the poll carries on rather
 * than letting one bad response end the reconcile.
 */
export async function reconcileImport(
  before: RepoImportState,
  readState: () => Promise<RepoImportState | null>,
  sleep: (ms: number) => Promise<void>,
): Promise<boolean> {
  for (let attempt = 0; attempt < RECONCILE_ATTEMPTS; attempt += 1) {
    if (attempt > 0) await sleep(RECONCILE_INTERVAL_MS);
    const after = await readState();
    if (after && importLanded(before, after)) return true;
  }
  return false;
}
