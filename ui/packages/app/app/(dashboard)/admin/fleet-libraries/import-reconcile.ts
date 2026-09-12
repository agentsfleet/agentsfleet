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
 * So on a timeout — and only on a timeout — the catalog gets one more read,
 * and the answer comes from the row rather than from our own patience.
 *
 * The comparison is server value against server value. `content_hash` is
 * written by the import and by nothing else, so a hash that changed is proof
 * the import reached its last step. Comparing timestamps against the browser's
 * clock would have made skew the deciding factor; comparing hashes makes the
 * bundle the deciding factor.
 */

/** What the catalog held for one repository at one moment. */
export type RepoImportState = {
  present: boolean;
  /** `null` is a row that exists but has never carried a bundle. */
  contentHash: string | null;
};

export const REPO_ABSENT: RepoImportState = { present: false, contentHash: null };

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
  return { present: true, contentHash: entry.content_hash };
}

/**
 * Did the import land after we stopped waiting for it?
 *
 * Three ways to answer no, and each is a state the catalog can really be in:
 * no row for this repository, a row that still carries no bundle, and a row
 * whose bundle is the same one it had before we submitted. That last case is
 * the refetch path's whole difficulty — the row was already there and already
 * had a hash, so presence proves nothing and only a CHANGED hash does.
 *
 * Answering yes on an unchanged hash would report a refetch that timed out and
 * genuinely failed as a success, which is the same lie in the other direction.
 */
export function importLanded(before: RepoImportState, after: RepoImportState): boolean {
  if (!after.present || after.contentHash === null) return false;
  if (!before.present || before.contentHash === null) return true;
  return after.contentHash !== before.contentHash;
}
