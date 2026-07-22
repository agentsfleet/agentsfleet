/**
 * Per-spec teardown for fixture rows.
 *
 * Specs share the fixture workspace ACROSS PARALLEL WORKERS, so per-spec
 * cleanup must scope to the spec's own seed prefix — an unscoped call
 * deletes a sibling's fleet mid-test. Specs call cleanWorkspaceFleets in
 * test.afterEach with their prefix; omitting it deletes everything and is
 * reserved for single-workspace specs that own their workspace outright.
 *
 * Tenant/workspace itself is preserved across runs (idempotent bootstrap);
 * only fleets/credentials/events get torn down.
 */
import { clientFor } from "./api-client";
import type { ClientHandle } from "./api-client";
import { listFleets } from "./seed";
import { AGENTSFLEET_STATUS } from "./constants";

/**
 * agentsfleetd enforces a state-machine transition before delete:
 * PATCH status=killed must run first, otherwise DELETE 409s with UZ-AGT-010.
 * Fleets in any non-killed state need to be killed before being deleted.
 *
 * Tolerates per-fleet failures so one stuck row doesn't block teardown of
 * the rest. Returns the count successfully removed.
 */
// The mass-delete below is destructive. It must only ever run against a
// disposable e2e target — a misconfigured NEXT_PUBLIC_API_URL pointing at a
// real environment, combined with real fixture credentials, would otherwise
// wipe live fleets. Refuse anything that isn't localhost or an explicit
// -dev / e2e host.
const SAFE_API_HOST = /(^|\.)localhost$|(^|\.)(api-dev|e2e)[.-]|(^|\.)dev\./;

function assertDestructiveTargetIsSafe(): void {
  const url = process.env.NEXT_PUBLIC_API_URL ?? "";
  let host = "";
  try {
    host = new URL(url).hostname;
  } catch {
    host = url;
  }
  if (!SAFE_API_HOST.test(host)) {
    throw new Error(
      `[e2e:teardown] refusing to mass-delete fleets against non-dev API host "${host}". ` +
        `Fleet teardown only runs against localhost / *-dev / e2e targets.`,
    );
  }
}

export async function cleanWorkspaceFleets(
  handle: ClientHandle,
  workspaceId: string,
  namePrefix?: string,
): Promise<number> {
  assertDestructiveTargetIsSafe();
  const c = clientFor(handle);
  const fleets = await listFleets(handle, workspaceId);
  let removed = 0;
  for (const z of fleets) {
    // Specs run in parallel workers against the shared fixture workspace;
    // an unscoped cleanup deletes a sibling spec's fleet mid-test. Callers
    // pass their seed prefix so each spec tears down only its own rows.
    if (namePrefix !== undefined && !z.name.startsWith(namePrefix)) continue;
    try {
      if (z.status !== AGENTSFLEET_STATUS.killed) {
        await c.patch(`/v1/workspaces/${workspaceId}/fleets/${z.id}`, {
          status: AGENTSFLEET_STATUS.killed,
        });
      }
      await c.delete(`/v1/workspaces/${workspaceId}/fleets/${z.id}`);
      removed++;
    } catch {
      // Swallow stale-state errors (fleets left over from interrupted runs).
      // Test assertions check the freshly-seeded row, not total count.
    }
  }
  return removed;
}
