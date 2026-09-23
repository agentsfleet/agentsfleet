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

/** How many entries one sweep pass asks for. */
const LIBRARY_SWEEP_PAGE_SIZE = 100;

/** How many passes the drain makes before giving up and reporting what it has. */
const LIBRARY_SWEEP_PASS_CEILING = 50;

/**
 * One workspace's own Fleet library entries, removed.
 *
 * The other half of what a leaked run leaves behind. A leaked FLEET is not
 * inert — its seeded cron trigger keeps waking runners until the row is gone —
 * and a leaked library ENTRY is inert but cumulative: it stays in the install
 * gallery forever, because until M204 there was no verb that could remove it.
 * The pile-up is what pushed the seeded card off the gallery's first page and
 * made `installViaUI` miss it.
 *
 * No prefix scoping, and no `namePrefix` parameter to pass one. The whole
 * shape of the acceptance suite is re-onboarding ONE stable name per run, so
 * every tenant entry in a fixture workspace is this run's or a previous run's,
 * and both should go. A prefix would also be the hand-maintained list that
 * `sweepLeakedFixtureFleets` records falling behind the specs.
 *
 * Only the workspace's OWN entries: the read is the owned collection, which
 * never carries a platform row, so the platform catalogue cannot be reached
 * from here even by mistake.
 */
export async function cleanWorkspaceLibraryEntries(
  handle: ClientHandle,
  workspaceId: string,
): Promise<SweepCounts> {
  assertDestructiveTargetIsSafe();
  const c = clientFor(handle);
  const counts: SweepCounts = { removed: 0, failed: 0 };
  // Drained, not read once. A single page would under-reap exactly the pile
  // this sweep exists to clear: entries accumulate without bound precisely
  // because nothing removed them before M204, so "one page" and "every entry"
  // stop being the same set the moment a workspace passes the page size.
  //
  // Re-reading the FIRST page each pass rather than following a cursor: the
  // rows are being deleted underneath the walk, so a keyset boundary would
  // seek past rows that shifted forward. Deleting from the front converges.
  for (let pass = 0; pass < LIBRARY_SWEEP_PASS_CEILING; pass += 1) {
    const page = await c.get<{ items?: Array<{ id: string; name?: string }> }>(
      `/v1/workspaces/${workspaceId}/library-entries?limit=${LIBRARY_SWEEP_PAGE_SIZE}`,
    );
    const entries = page.items ?? [];
    if (entries.length === 0) break;
    const removedBefore = counts.removed;
    for (const entry of entries) {
      try {
        await c.delete(`/v1/workspaces/${workspaceId}/library-entries/${entry.id}`);
        counts.removed++;
      } catch (err) {
        // Counted, not swallowed — the same reason the fleet sweep gives. A
        // removal that failed is a row still in the gallery, which is the whole
        // defect this sweep exists to prevent.
        counts.failed++;
        console.error(
          `[e2e:teardown] remove failed for library entry ${entry.id} in workspace ${workspaceId}:`,
          err,
        );
      }
    }
    // Every row on this page refused. Re-reading returns the same page, so
    // the loop would spin until the ceiling; stop and let the counts report it.
    if (counts.removed === removedBefore) break;
  }
  return counts;
}

/**
 * Backstop sweep for global-teardown: reap every tenant library entry in every
 * workspace a persistent fixture user owns.
 *
 * Deliberately the same shape, the same guard and the same blast radius as
 * [`sweepLeakedFixtureFleets`] — read its note, which applies here unchanged.
 * It runs beside that sweep rather than inside it because the two answer for
 * different rows and either can fail without the other needing to.
 */
export async function sweepLeakedFixtureLibraries(): Promise<SweepCounts> {
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
        const counts = await cleanWorkspaceLibraryEntries(key, workspace.id);
        total.removed += counts.removed;
        total.failed += counts.failed;
      } catch (err) {
        console.error(
          `[e2e:sweep] library sweep failed in workspace ${workspace.id} ('${key}'):`,
          err,
        );
        total.failed++;
      }
    }
  }
  const summary = `[e2e:sweep] done — ${total.removed} fixture library entr(ies) removed, ${total.failed} failed`;
  if (total.failed > 0) console.error(summary);
  else console.log(summary);
  return total;
}

/**
 * The prefix every CLI key this suite mints carries, so the sweep can find one
 * nobody deleted. Kept here beside the sweep that reads it (RULE UFS).
 */
export const CLI_KEY_PREFIX = "acc-cli-key-";

/**
 * Revoke and delete every API key this suite left behind.
 *
 * Unlike a leaked fleet or library entry, a leaked key is not inert: it is a
 * live `agt_t` tenant credential sitting in the account until somebody notices.
 * `workspace-library.spec.ts` deletes its own in a `finally`, which covers a
 * thrown assertion and does NOT cover the run being killed — a cancelled
 * workflow, a runner reclaimed mid-test, an `--exit-on-first-failure`. That gap
 * is the whole reason this exists.
 *
 * Prefix-matched, so it can never touch a key a human made.
 */
export async function sweepLeakedFixtureKeys(): Promise<SweepCounts> {
  assertDestructiveTargetIsSafe();
  const total: SweepCounts = { removed: 0, failed: 0 };
  for (const key of FIXTURE_KEYS) {
    const client = clientFor(key as ClientHandle);
    // Every page, not just the first. The listing defaults to a page size, so
    // a tenant that has accumulated keys hides the OLDEST ones — exactly the
    // leaked credentials this sweep exists to reach — behind a cursor.
    let cursor: string | null = null;
    do {
      // `starting_after`, not `cursor` (api_key.rs:124 — Stripe-style keyset
      // pagination). The wrong name is not an error the server reports: it is
      // ignored, page one is served again, and the loop never advances past
      // the newest keys — an unterminated teardown on a sweep whose whole job
      // is reaching the OLDER ones.
      const path: string = cursor
        ? `/v1/api-keys?starting_after=${encodeURIComponent(cursor)}`
        : "/v1/api-keys";
      let page: { items?: Array<{ id?: string; key_name?: string }>; next_cursor?: string | null };
      try {
        page = await client.get<typeof page>(path);
      } catch (err) {
        console.error(`[e2e:sweep] API-key listing failed for fixture '${key}':`, err);
        total.failed++;
        break;
      }
      for (const row of page.items ?? []) {
        if (!row.id || !row.key_name?.startsWith(CLI_KEY_PREFIX)) continue;
        try {
          await revokeAndDeleteKey(client, row.id);
          total.removed++;
        } catch (err) {
          console.error(`[e2e:sweep] API-key delete failed for ${row.key_name}:`, err);
          total.failed++;
        }
      }
      cursor = page.next_cursor ?? null;
    } while (cursor);
  }
  const summary = `[e2e:sweep] done — ${total.removed} fixture API key(s) removed, ${total.failed} failed`;
  if (total.failed > 0) console.error(summary);
  else console.log(summary);
  return total;
}

/**
 * Revoke, then delete. Both verbs live on the SAME path.
 *
 * Revocation is `PATCH /v1/api-keys/{id}` with `{"active": false}` — there is
 * no `POST .../revoke` route, and an earlier draft of this file called one.
 * It 404'd, the delete that followed refused a still-active key, neither status
 * was read, and the helper reported success over a live tenant credential. The
 * client throws on a non-2xx, so a failure is now loud by construction.
 */
async function revokeAndDeleteKey(
  client: ReturnType<typeof clientFor>,
  id: string,
): Promise<void> {
  await client.patch(`/v1/api-keys/${id}`, { active: false });
  await client.delete(`/v1/api-keys/${id}`);
}
