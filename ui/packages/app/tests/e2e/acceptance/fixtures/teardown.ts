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

/**
 * What a sweep actually did. `failed` is the point of the shape: a fleet the
 * sweep matched but could not delete used to vanish into a swallowed catch,
 * so a run could leak rows and still print a clean summary.
 */
export type SweepCounts = { removed: number; failed: number };

export async function cleanWorkspaceFleets(
  handle: ClientHandle,
  workspaceId: string,
  namePrefix?: string,
): Promise<SweepCounts> {
  assertDestructiveTargetIsSafe();
  const c = clientFor(handle);
  const fleets = await listFleets(handle, workspaceId);
  const counts: SweepCounts = { removed: 0, failed: 0 };
  for (const z of fleets) {
    // Specs run in parallel workers against the shared fixture workspace;
    // an unscoped cleanup deletes a sibling spec's fleet mid-test. Callers
    // pass their seed prefix so each spec tears down only its own rows.
    // The global-teardown sweep passes nothing: no spec is running then, so
    // the reason for scoping does not hold and keeping nothing is the rule.
    if (namePrefix !== undefined && !z.name.startsWith(namePrefix)) continue;
    try {
      if (z.status !== AGENTSFLEET_STATUS.killed) {
        await c.patch(`/v1/workspaces/${workspaceId}/fleets/${z.id}`, {
          status: AGENTSFLEET_STATUS.killed,
        });
      }
      await c.delete(`/v1/workspaces/${workspaceId}/fleets/${z.id}`);
      counts.removed++;
    } catch (err) {
      // Counted, not swallowed. A stale-state row from an interrupted run is
      // expected and must not fail the caller, but it is also exactly the row
      // that keeps waking runners — so it has to appear in the summary.
      counts.failed++;
      console.error(
        `[e2e:teardown] delete failed for fleet ${z.id} ('${z.name}') in workspace ${workspaceId}:`,
        err,
      );
    }
  }
  return counts;
}

/**
 * Backstop sweep for global-teardown: reap every fleet in every workspace a
 * persistent fixture user owns.
 *
 * It sweeps by OWNERSHIP, not by name. The predecessor matched a
 * hand-maintained list of six seed prefixes while the specs mint roughly
 * twenty-two, so most leaked fleets were never reaped and the list had to be
 * edited every time a spec was added — a check that silently covered less
 * than it appeared to. Nothing scopes the sweep now, so it cannot fall behind
 * the specs.
 *
 * Deleting everything visible is safe here by construction: the listing runs
 * through an authenticated fixture handle and returns only workspaces that
 * user owns, and `global-setup` seeds no persistent fleets — so every fleet
 * reachable from here is a test artifact. Prefix scoping remains load-bearing
 * in the per-spec `afterEach` path, where parallel workers share one
 * workspace; at global teardown no test is running and that reason is gone.
 *
 * Per-fixture and per-workspace failures log and continue — one dead tenant
 * must not shield another tenant's leaks — and the same destructive-target
 * guard as cleanWorkspaceFleets runs before any listing or deletion.
 *
 * KNOWN BLAST RADIUS, accepted deliberately: "every workspace the fixture user
 * owns" is only equivalent to "every workspace of test fixtures" while the
 * fixture users own nothing else. Add a fixture user to a shared or human-owned
 * workspace and this empties it, where the old prefix scoping would have spared
 * anything not matching a seed name. Two things bound that: the destructive
 * target guard refuses any host that is not localhost / *-dev / e2e, so the
 * reach is a disposable environment by construction; and the fixture users are
 * provisioned solely by `global-setup`. If a fixture user ever needs to join a
 * real workspace, this sweep has to be re-scoped first.
 */
export async function sweepLeakedFixtureFleets(): Promise<SweepCounts> {
  assertDestructiveTargetIsSafe();
  const total: SweepCounts = { removed: 0, failed: 0 };
  for (const key of FIXTURE_KEYS) {
    const workspaces = await listWorkspaces(key).catch((err: unknown) => {
      console.error(`[e2e:sweep] workspace listing failed for fixture '${key}':`, err);
      total.failed++;
      return [];
    });
    for (const workspace of workspaces) {
      try {
        const counts = await cleanWorkspaceFleets(key, workspace.id);
        total.removed += counts.removed;
        total.failed += counts.failed;
      } catch (err) {
        console.error(
          `[e2e:sweep] fleet sweep failed in workspace ${workspace.id} ('${key}'):`,
          err,
        );
        total.failed++;
      }
    }
  }
  const summary = `[e2e:sweep] done — ${total.removed} fixture fleet(s) removed, ${total.failed} failed`;
  if (total.failed > 0) console.error(summary);
  else console.log(summary);
  return total;
}
