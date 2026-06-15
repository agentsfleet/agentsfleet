/**
 * Per-spec teardown for fixture rows.
 *
 * Since each fixture user has its own dedicated tenant+workspace and no human
 * shares it, "delete all agents in the fixture workspace" IS per-spec
 * cleanup. Specs call cleanWorkspaceAgents in test.afterEach to keep state
 * isolated across runs.
 *
 * Tenant/workspace itself is preserved across runs (idempotent bootstrap);
 * only agents/credentials/events get torn down.
 */
import { clientFor } from "./api-client";
import type { ClientHandle } from "./api-client";
import { listAgents } from "./seed";
import { AGENTSFLEET_STATUS } from "./constants";

/**
 * agentsfleetd enforces a state-machine transition before delete:
 * PATCH status=killed must run first, otherwise DELETE 409s with UZ-AGT-010.
 * Agents in any non-killed state need to be killed before being deleted.
 *
 * Tolerates per-agent failures so one stuck row doesn't block teardown of
 * the rest. Returns the count successfully removed.
 */
export async function cleanWorkspaceAgents(
  handle: ClientHandle,
  workspaceId: string,
): Promise<number> {
  const c = clientFor(handle);
  const agents = await listAgents(handle, workspaceId);
  let removed = 0;
  for (const z of agents) {
    try {
      if (z.status !== AGENTSFLEET_STATUS.killed) {
        await c.patch(`/v1/workspaces/${workspaceId}/agents/${z.id}`, {
          status: AGENTSFLEET_STATUS.killed,
        });
      }
      await c.delete(`/v1/workspaces/${workspaceId}/agents/${z.id}`);
      removed++;
    } catch {
      // Swallow stale-state errors (agents left over from interrupted runs).
      // Test assertions check the freshly-seeded row, not total count.
    }
  }
  return removed;
}
