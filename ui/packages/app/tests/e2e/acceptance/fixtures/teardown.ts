/**
 * Per-spec teardown for fixture rows.
 *
 * Specs share the fixture workspace ACROSS PARALLEL WORKERS, so per-spec
 * cleanup must scope to the spec's own seed prefix — an unscoped call
 * deletes a sibling's fleet mid-test. Specs call cleanWorkspaceFleets in
 * test.afterEach with their prefix; omitting it deletes everything and is
 * reserved for single-workspace specs that own their workspace outright.
 *
 * Tenant/workspace itself is preserved across runs (idempotent bootstrap);
 * only fleets/credentials/events get torn down.
 *
 * global-teardown's backstop sweep (sweepLeakedFixtureFleets) lives here
 * too, next to the destructive-target guard every deletion must share.
 */
import { clientFor } from "./api-client";
import type { ClientHandle } from "./api-client";
import { listFleets, listWorkspaces } from "./seed";
import { AGENTSFLEET_STATUS, FIXTURE_KEYS } from "./constants";

/**
 * agentsfleetd enforces a state-machine transition before delete:
 * PATCH status=killed must run first, otherwise DELETE 409s with UZ-AGT-010.
 * Fleets in any non-killed state need to be killed before being deleted.
 *
 * Tolerates per-fleet failures so one stuck row doesn't block teardown of
 * the rest. Returns the count successfully removed.
 */
// The mass-delete below is destructive. It must only ever run against a
// disposable e2e target — a misconfigured NEXT_PUBLIC_API_URL pointing at a
// real environment, combined with real fixture credentials, would otherwise
// wipe live fleets. Refuse anything that isn't localhost or an explicit
// -dev / e2e host.
const SAFE_API_HOST = /(^|\.)localhost$|(^|\.)(api-dev|e2e)[.-]|(^|\.)dev\./;

function assertDestructiveTargetIsSafe(): void {
  const url = process.env.NEXT_PUBLIC_API_URL ?? "";
  let host = "";
  try {
    host = new URL(url).hostname;
  } catch {
    host = url;
  }
  if (!SAFE_API_HOST.test(host)) {
    throw new Error(
      `[e2e:teardown] refusing to mass-delete fleets against non-dev API host "${host}". ` +
        `Fleet teardown only runs against localhost / *-dev / e2e targets.`,
    );
  }
}

export async function cleanWorkspaceFleets(
  handle: ClientHandle,
  workspaceId: string,
  namePrefix?: string,
): Promise<number> {
  assertDestructiveTargetIsSafe();
  const c = clientFor(handle);
  const fleets = await listFleets(handle, workspaceId);
  let removed = 0;
  for (const z of fleets) {
    // Specs run in parallel workers against the shared fixture workspace;
    // an unscoped cleanup deletes a sibling spec's fleet mid-test. Callers
    // pass their seed prefix so each spec tears down only its own rows.
    if (namePrefix !== undefined && !z.name.startsWith(namePrefix)) continue;
    try {
      if (z.status !== AGENTSFLEET_STATUS.killed) {
        await c.patch(`/v1/workspaces/${workspaceId}/fleets/${z.id}`, {
          status: AGENTSFLEET_STATUS.killed,
        });
      }
      await c.delete(`/v1/workspaces/${workspaceId}/fleets/${z.id}`);
      removed++;
    } catch {
      // Swallow stale-state errors (fleets left over from interrupted runs).
      // Test assertions check the freshly-seeded row, not total count.
    }
  }
  return removed;
}

/**
 * Seed prefixes known to leak. Every spec that seeds one of these also
 * cleans it in afterEach, but afterEach never runs for a crashed run or an
 * interrupted CI job — and a leaked fleet is not inert: its seeded cron
 * trigger keeps waking runners until someone deletes the row. The
 * global-teardown sweep reaps them across every persistent fixture
 * workspace as the backstop.
 */
export const LEAKED_FLEET_PREFIXES = [
  "lifecycle-",
  "journey-fleet-",
  "steer-probe-",
  "login-lifecycle-",
  "thread-spec-",
  "thread-revisit-",
] as const;

// operator-journey mints these workspaces outright (a fresh pair per run),
// so every fleet inside one is a fixture row by construction — sweep them
// whole rather than by prefix.
export const JOURNEY_WORKSPACE_RE = /^journey-(primary|secondary)-[0-9a-f]{8}$/;

/**
 * Backstop sweep for global-teardown: reap leaked fleets across all
 * persistent fixture users. Per-fixture and per-workspace failures log and
 * continue — one dead tenant must not shield another tenant's leaks — and
 * the same destructive-target guard as cleanWorkspaceFleets runs before any
 * listing or deletion.
 */
export async function sweepLeakedFixtureFleets(): Promise<void> {
  assertDestructiveTargetIsSafe();
  let removed = 0;
  for (const key of FIXTURE_KEYS) {
    const workspaces = await listWorkspaces(key).catch((err: unknown) => {
      console.error(`[e2e:sweep] workspace listing failed for fixture '${key}':`, err);
      return [];
    });
    for (const workspace of workspaces) {
      try {
        if (JOURNEY_WORKSPACE_RE.test(workspace.name ?? "")) {
          removed += await cleanWorkspaceFleets(key, workspace.id);
          continue;
        }
        // One fleet listing per prefix — teardown cadence, so clarity beats
        // shaving round-trips.
        for (const prefix of LEAKED_FLEET_PREFIXES) {
          removed += await cleanWorkspaceFleets(key, workspace.id, prefix);
        }
      } catch (err) {
        console.error(
          `[e2e:sweep] fleet sweep failed in workspace ${workspace.id} ('${key}'):`,
          err,
        );
      }
    }
  }
  console.log(`[e2e:sweep] done — ${removed} leaked fixture fleet(s) removed`);
}
