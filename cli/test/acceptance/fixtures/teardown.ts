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
  // EVERY prefixed row, terminal or not: killed is only the first half of the
  // product's teardown state machine (DELETE 409s UZ-AGT-010 until a fleet is
  // killed), and stopping there is how ~100 killed acc-* rows accumulated in
  // the shared dev workspace — each rerun's fresh prefix and this filter's old
  // terminal-skip both looked away from the last run's leftovers.
  const mine = items.filter((z) => {
    if (opts.workspaceId && z.workspace_id && z.workspace_id !== opts.workspaceId) return false;
    return Boolean(z.name && z.name.startsWith(runPrefix));
  });
  for (const fleet of mine) {
    // List responses may carry `fleet_id` instead of `id`; lifecycle.ts
    // already guards both. Without the fallback, `kill undefined` trips
    // the uuidv7 validator and the error-tolerance regex misses it.
    const fleetId = fleet.id ?? fleet.fleet_id;
    if (!fleetId) continue;
    if (!TERMINAL_STATUSES.includes(fleet.status ?? "")) {
      const killed = await runFleetctl(["kill", fleetId, "--json"], { env });
      if (killed.code !== 0 && !/already.*killed|already.*terminal|not.*found/i.test(killed.stderr)) {
        throw new Error(`teardown kill ${fleetId} exited ${killed.code}: ${killed.stderr.trim()}`);
      }
    }
    const removed = await runFleetctl(["delete", fleetId, "--json"], { env });
    if (removed.code !== 0 && !/not.*found/i.test(removed.stderr)) {
      throw new Error(`teardown delete ${fleetId} exited ${removed.code}: ${removed.stderr.trim()}`);
    }
  }
  return mine.length;
}
