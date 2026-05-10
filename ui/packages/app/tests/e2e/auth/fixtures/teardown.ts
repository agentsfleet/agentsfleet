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
import { listZombies } from "./seed";
import type { FixtureKey } from "./auth";

export async function cleanWorkspaceZombies(
  key: FixtureKey,
  workspaceId: string,
): Promise<number> {
  const c = clientFor(key);
  const zombies = await listZombies(key, workspaceId);
  for (const z of zombies) {
    await c.delete(`/v1/workspaces/${workspaceId}/zombies/${z.id}`);
  }
  return zombies.length;
}
