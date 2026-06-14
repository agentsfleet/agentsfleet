/**
 * Per-spec teardown for fixture rows.
 *
 * Since each fixture user has its own dedicated tenant+workspace and no human
 * shares it, "delete all zombies in the fixture workspace" IS per-spec
 * cleanup. Specs call cleanWorkspaceZombies in test.afterEach to keep state
 * isolated across runs.
 *
 * Tenant/workspace itself is preserved across runs (idempotent bootstrap);
 * only zombies/credentials/events get torn down.
 */
import { clientFor } from "./api-client";
import type { ClientHandle } from "./api-client";
import { listZombies } from "./seed";
import { ZOMBIE_STATUS } from "./constants";

/**
 * agentsfleetd enforces a state-machine transition before delete:
 * PATCH status=killed must run first, otherwise DELETE 409s with UZ-ZMB-010.
 * Zombies in any non-killed state need to be killed before being deleted.
 *
 * Tolerates per-zombie failures so one stuck row doesn't block teardown of
 * the rest. Returns the count successfully removed.
 */
export async function cleanWorkspaceZombies(
  handle: ClientHandle,
  workspaceId: string,
): Promise<number> {
  const c = clientFor(handle);
  const zombies = await listZombies(handle, workspaceId);
  let removed = 0;
  for (const z of zombies) {
    try {
      if (z.status !== ZOMBIE_STATUS.killed) {
        await c.patch(`/v1/workspaces/${workspaceId}/zombies/${z.id}`, {
          status: ZOMBIE_STATUS.killed,
        });
      }
      await c.delete(`/v1/workspaces/${workspaceId}/zombies/${z.id}`);
      removed++;
    } catch {
      // Swallow stale-state errors (zombies left over from interrupted runs).
      // Test assertions check the freshly-seeded row, not total count.
    }
  }
  return removed;
}
