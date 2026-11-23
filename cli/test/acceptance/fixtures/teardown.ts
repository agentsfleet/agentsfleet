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

import { ACCEPTANCE_RUN_PREFIX } from "./constants.ts";
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
  const listed = await runFleetctl(["fleet", "list", "--json"], { env });
  if (listed.code !== 0) {
    throw new Error(`fleet list (teardown) exited ${listed.code}: ${listed.stderr.trim()}`);
  }
  const payload = JSON.parse(listed.stdout.trim() || "{}") as { items?: unknown };
  const items: FleetRow[] = Array.isArray(payload.items) ? (payload.items as FleetRow[]) : [];
  // EVERY prefixed row, archived or not: delete removes a fleet in any
  // status, and skipping archived rows is how ~100 acc-* rows once
  // accumulated in the shared dev workspace.
  const mine = items.filter((z) => {
    if (opts.workspaceId && z.workspace_id && z.workspace_id !== opts.workspaceId) return false;
    return Boolean(z.name && z.name.startsWith(runPrefix));
  });
  for (const fleet of mine) {
    // List responses may carry `fleet_id` instead of `id`; lifecycle.ts
    // already guards both.
    const fleetId = fleet.id ?? fleet.fleet_id;
    if (!fleetId) continue;
    await deleteFixtureFleet(env, fleetId);
  }
  return mine.length;
}

/**
 * Deletes the one fleet a spec recorded when it saved it. Delete removes a
 * fleet in any status, and a fleet already gone is not a teardown failure.
 */
export async function deleteFixtureFleet(env: Env, fleetId: string): Promise<void> {
  const removed = await runFleetctl(["fleet", "delete", fleetId, "--json"], { env });
  if (removed.code !== 0 && !/not.*found/i.test(removed.stderr)) {
    throw new Error(`teardown delete ${fleetId} exited ${removed.code}: ${removed.stderr.trim()}`);
  }
}
