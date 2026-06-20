/**
 * Per-spec teardown for fixture rows.
 *
 * Since each fixture user has its own dedicated tenant+workspace and no human
 * shares it, "delete all fleets in the fixture workspace" IS per-spec
 * cleanup. Specs call cleanWorkspaceFleets in test.afterEach to keep state
 * isolated across runs.
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
export async function cleanWorkspaceFleets(
  handle: ClientHandle,
  workspaceId: string,
): Promise<number> {
  const c = clientFor(handle);
  const fleets = await listFleets(handle, workspaceId);
  let removed = 0;
  for (const z of fleets) {
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
