/**
 * `afterEach` teardown — kills any non-terminal zombies belonging to a
 * workspace. Tenant + billing-balance teardown is intentionally out of
 * scope (long-running PROD fixture deferral).
 */

import { TERMINAL_STATUSES } from "./constants.js";
import { runZombiectl } from "./cli.js";

export async function cleanWorkspaceZombies(env, workspaceId) {
  const listed = await runZombiectl(["list", "--json"], { env });
  if (listed.code !== 0) {
    throw new Error(`zombie list (teardown) exited ${listed.code}: ${listed.stderr.trim()}`);
  }
  const payload = JSON.parse(listed.stdout.trim() || "{}");
  const items = Array.isArray(payload.items) ? payload.items : [];
  const live = items.filter((z) => {
    if (workspaceId && z.workspace_id && z.workspace_id !== workspaceId) return false;
    return !TERMINAL_STATUSES.includes(z.status);
  });
  for (const zombie of live) {
    const killed = await runZombiectl(["kill", zombie.id, "--json"], { env });
    if (killed.code !== 0 && !/already.*killed|not.*found/i.test(killed.stderr)) {
      throw new Error(`teardown kill ${zombie.id} exited ${killed.code}: ${killed.stderr.trim()}`);
    }
  }
  return live.length;
}
