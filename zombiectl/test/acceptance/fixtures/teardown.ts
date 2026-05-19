/**
 * `afterEach` teardown — kills any non-terminal zombies belonging to a
 * workspace AND created by the current acceptance run (filtered by
 * `runPrefix`). Tenant + billing-balance teardown is intentionally out
 * of scope (long-running PROD fixture deferral).
 *
 * Run-prefix scoping is what makes the shared-DEV-tenant invariant
 * tractable: leftover zombies from other runs/agents are skipped, and
 * the post-teardown empty-list assertion holds *for this run's names*
 * regardless of global tenant state.
 */

import { ACCEPTANCE_RUN_PREFIX, TERMINAL_STATUSES } from "./constants.ts";
import { runZombiectl } from "./cli.js";
import type { ZombieRow } from "./lifecycle.ts";

type Env = Readonly<Record<string, string>>;

export interface TeardownOptions {
  readonly workspaceId?: string;
  // Defaults to the per-process `ACCEPTANCE_RUN_PREFIX`. Override only
  // when a spec needs to clean a separately-prefixed sub-namespace.
  readonly runPrefix?: string;
}

export async function cleanWorkspaceZombies(
  env: Env,
  optsOrWorkspaceId?: TeardownOptions | string,
): Promise<number> {
  const opts: TeardownOptions = typeof optsOrWorkspaceId === "string"
    ? { workspaceId: optsOrWorkspaceId }
    : (optsOrWorkspaceId ?? {});
  const runPrefix = opts.runPrefix ?? ACCEPTANCE_RUN_PREFIX;
  const listed = await runZombiectl(["list", "--json"], { env });
  if (listed.code !== 0) {
    throw new Error(`zombie list (teardown) exited ${listed.code}: ${listed.stderr.trim()}`);
  }
  const payload = JSON.parse(listed.stdout.trim() || "{}") as { items?: unknown };
  const items: ZombieRow[] = Array.isArray(payload.items) ? (payload.items as ZombieRow[]) : [];
  const live = items.filter((z) => {
    if (opts.workspaceId && z.workspace_id && z.workspace_id !== opts.workspaceId) return false;
    if (!z.name || !z.name.startsWith(runPrefix)) return false;
    return !TERMINAL_STATUSES.includes(z.status ?? "");
  });
  for (const zombie of live) {
    // List responses may carry `zombie_id` instead of `id`; lifecycle.ts
    // already guards both. Without the fallback, `kill undefined` trips
    // the uuidv7 validator and the error-tolerance regex misses it.
    const zombieId = zombie.id ?? zombie.zombie_id;
    if (!zombieId) continue;
    const killed = await runZombiectl(["kill", zombieId, "--json"], { env });
    if (killed.code !== 0 && !/already.*killed|already.*terminal|not.*found/i.test(killed.stderr)) {
      throw new Error(`teardown kill ${zombieId} exited ${killed.code}: ${killed.stderr.trim()}`);
    }
  }
  return live.length;
}
