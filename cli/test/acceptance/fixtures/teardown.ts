/**
 * `afterEach` teardown — kills any non-terminal fleets belonging to a
 * workspace AND created by the current acceptance run (filtered by
 * `runPrefix`). Tenant + billing-balance teardown is intentionally out
 * of scope (long-running PROD fixture deferral).
 *
 * Run-prefix scoping is what makes the shared-DEV-tenant invariant
 * tractable: leftover fleets from other runs/fleets are skipped, and
 * the post-teardown empty-list assertion holds *for this run's names*
 * regardless of global tenant state.
 */

import { ACCEPTANCE_RUN_PREFIX, TERMINAL_STATUSES } from "./constants.ts";
import { runFleetctl } from "./cli.js";
import type { FleetRow } from "./lifecycle.ts";

type Env = Readonly<Record<string, string>>;

export interface TeardownOptions {
  readonly workspaceId?: string;
  // Defaults to the per-process `ACCEPTANCE_RUN_PREFIX`. Override only
  // when a spec needs to clean a separately-prefixed sub-namespace.
  readonly runPrefix?: string;
}

export async function cleanWorkspaceFleets(
  env: Env,
  optsOrWorkspaceId?: TeardownOptions | string,
): Promise<number> {
  const opts: TeardownOptions = typeof optsOrWorkspaceId === "string"
    ? { workspaceId: optsOrWorkspaceId }
    : (optsOrWorkspaceId ?? {});
  const runPrefix = opts.runPrefix ?? ACCEPTANCE_RUN_PREFIX;
  const listed = await runFleetctl(["list", "--json"], { env });
  if (listed.code !== 0) {
    throw new Error(`fleet list (teardown) exited ${listed.code}: ${listed.stderr.trim()}`);
  }
  const payload = JSON.parse(listed.stdout.trim() || "{}") as { items?: unknown };
  const items: FleetRow[] = Array.isArray(payload.items) ? (payload.items as FleetRow[]) : [];
  const live = items.filter((z) => {
    if (opts.workspaceId && z.workspace_id && z.workspace_id !== opts.workspaceId) return false;
    if (!z.name || !z.name.startsWith(runPrefix)) return false;
    return !TERMINAL_STATUSES.includes(z.status ?? "");
  });
  for (const fleet of live) {
    // List responses may carry `fleet_id` instead of `id`; lifecycle.ts
    // already guards both. Without the fallback, `kill undefined` trips
    // the uuidv7 validator and the error-tolerance regex misses it.
    const fleetId = fleet.id ?? fleet.fleet_id;
    if (!fleetId) continue;
    const killed = await runFleetctl(["kill", fleetId, "--json"], { env });
    if (killed.code !== 0 && !/already.*killed|already.*terminal|not.*found/i.test(killed.stderr)) {
      throw new Error(`teardown kill ${fleetId} exited ${killed.code}: ${killed.stderr.trim()}`);
    }
  }
  return live.length;
}
