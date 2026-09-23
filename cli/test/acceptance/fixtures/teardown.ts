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

/**
 * Remove the library entries a run onboarded.
 *
 * `cleanWorkspaceFleets` was this lane's entire teardown, and a fleet is not
 * the only row a spec creates: `library-onboard-live` onboards two entries per
 * run and deleted neither. They are inert on their own and they accumulate —
 * the gallery listing grows every run until somebody clears it by hand, and a
 * listing large enough to be truncated is what made `library --json` report a
 * row it had just printed as missing. The mess and the mis-parse were the same
 * mess.
 *
 * Prefix-scoped like the fleet sweep beside it, for the same reason: specs run
 * against a shared fixture workspace, and an unscoped delete takes a sibling's
 * row out from under it mid-test.
 */
export async function cleanWorkspaceLibraryEntries(
  env: Env,
  options: TeardownOptions = {},
): Promise<number> {
  // `workspaceId` rides in `TeardownOptions` for symmetry with the fleet
  // sweep; the CLI resolves the workspace from its own state, so nothing here
  // needs to read it.
  const runPrefix = options.runPrefix ?? ACCEPTANCE_RUN_PREFIX;
  let removed = 0;
  const listed = await runFleetctl(["library", "--json"], { env });
  if (listed.code !== 0) return removed;
  let rows: Array<{ id?: string; name?: string }>;
  try {
    const open = listed.stdout.indexOf("{");
    if (open < 0) return removed;
    rows = (JSON.parse(listed.stdout.slice(open)) as { items?: typeof rows }).items ?? [];
  } catch {
    // A listing this teardown cannot read is not a reason to fail a run that
    // already passed; the e2e lane's ownership sweep is the backstop.
    return removed;
  }
  for (const row of rows) {
    if (!row.id || !row.name?.startsWith(runPrefix)) continue;
    const deleted = await runFleetctl(["library", "delete", row.id], { env });
    if (deleted.code === 0) removed++;
  }
  return removed;
}
