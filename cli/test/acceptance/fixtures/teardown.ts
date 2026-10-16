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

import { Schema } from "effect";
import { nativeRequest, readAuthContext, type AuthContext } from "./native-api.ts";

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

/** Deletes only this run's explicitly published, workspace-owned entries. */
export async function cleanWorkspaceLibraryEntries(
  env: Env,
  options: TeardownOptions = {},
): Promise<number> {
  const ctx = await readAuthContext(env);
  if (options.workspaceId && options.workspaceId !== ctx.workspaceId) {
    throw new Error("Library cleanup workspace differs from the terminal session");
  }
  const prefix = options.runPrefix ?? ACCEPTANCE_RUN_PREFIX;
  const entries = await ownedLibraryEntries(ctx);
  const mine = entries.filter((entry) => entry.name.startsWith(prefix));
  for (const entry of mine) await nativeRequest(ctx, `/fleet-libraries/${encodeURIComponent(entry.id)}`, "DELETE");
  return mine.length;
}

const libraryPage = Schema.Struct({
  items: Schema.Array(Schema.Struct({id: Schema.String, name: Schema.String})),
  next_cursor: Schema.NullOr(Schema.String),
});
const CLEANUP_PAGES_MAX = 100;

async function ownedLibraryEntries(ctx: AuthContext): Promise<Array<{id: string; name: string}>> {
  const entries: Array<{id: string; name: string}> = [];
  const seen = new Set<string>();
  let cursor: string | null = null;
  for (let page = 0; page < CLEANUP_PAGES_MAX; page++) {
    const suffix = `/fleet-libraries/owned?limit=100${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ""}`;
    const result = Schema.decodeUnknownSync(libraryPage)(await nativeRequest(ctx, suffix));
    entries.push(...result.items);
    cursor = result.next_cursor;
    if (cursor === null) return entries;
    if (seen.has(cursor)) throw new Error("Library cleanup cursor did not advance");
    seen.add(cursor);
  }
  throw new Error("Library cleanup exceeded its page bound");
}
