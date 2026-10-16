/**
 * Idempotent fixture seeding helpers.
 *
 * Fixture rows are conceptually tagged with `x-test-fixture: true` for
 * cleanup discrimination. agentsfleetd does not
 * currently read that header, but each fixture user has its own dedicated
 * tenant + workspace — every fleet in that workspace is a fixture row by
 * construction. Per-spec cleanup deletes everything in the fixture user's
 * workspace; no extra discriminator needed today.
 */
import type { ClientHandle } from "./api-client";
import type { FixtureKey, FleetStatus } from "./constants";
import { fixtureDefinition } from "./definition";
import { saveFixtureFleet } from "./native-setup";
import { clientFor } from "./api-client";


export interface Workspace {
  id: string;
  name: string | null;
}

export interface Fleet {
  id: string;
  name: string;
  status?: FleetStatus;
}

interface ListResp<T> {
  items: T[];
  total: number;
  next_cursor?: string | null;
}

function handleLabel(handle: ClientHandle): string {
  return typeof handle === "string" ? handle : "ephemeral-jwt";
}

// Widened to ClientHandle so the ephemeral signup-flow user (whose JWT is
// minted mid-test and is NOT in the .fixture-jwts.json cache) can drive
// the lookup the same way persistent fixtures do.
export async function getDefaultWorkspaceId(handle: ClientHandle): Promise<string> {
  const c = clientFor(handle);
  const res = await c.get<ListResp<Workspace>>("/v1/tenants/me/workspaces");
  if (res.items.length === 0) {
    throw new Error(
      `Fixture user '${handleLabel(handle)}' has no workspace; bootstrap step must have failed.`,
    );
  }
  return res.items[0]!.id;
}

export interface SeedFleetOpts {
  name: string;
}

export async function seedFleet(key: FixtureKey, workspaceId: string, opts: SeedFleetOpts): Promise<Fleet> {
  const fleet = await saveFixtureFleet(key, workspaceId, fixtureDefinition(opts.name), opts.name);
  return { id: fleet.id, name: fleet.name };
}

/**
 * Poll until the fleet is `active`. `seedFleet` returns on the create response,
 * but the detail page hides its working surfaces behind the install gate until
 * the status flips — a spec that navigates immediately lands on the gate, not
 * the fleet. Waiting for `active` specifically (not merely "not installing")
 * means a fleet that FAILED, was killed, or paused fails the wait loudly
 * instead of passing the suite onto a fleet that can never render a console.
 */
export async function waitForFleetActive(
  handle: ClientHandle,
  workspaceId: string,
  fleetId: string,
  timeoutMs = 30_000,
): Promise<void> {
  const c = clientFor(handle);
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const fleet = await c.get<{ status: string }>(
      `/v1/workspaces/${workspaceId}/fleets/${fleetId}`,
    );
    if (fleet.status === "active") return;
    if (fleet.status !== "installing") {
      throw new Error(
        `[e2e:seed] fleet ${fleetId} reached terminal status "${fleet.status}" before active`,
      );
    }
    if (Date.now() > deadline) {
      throw new Error(
        `[e2e:seed] fleet ${fleetId} still installing after ${timeoutMs}ms`,
      );
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
}

/** The name the server stored for a fleet — the template's, or a suffixed one. */
export async function readFleetName(
  handle: ClientHandle,
  workspaceId: string,
  fleetId: string,
): Promise<string> {
  const fleet = await clientFor(handle).get<{ name: string }>(
    `/v1/workspaces/${workspaceId}/fleets/${fleetId}`,
  );
  return fleet.name;
}

/** The fleet's own lifetime counters, in the units the daemon stores them in.
 * The wall tile renders these two, spend rounded to cents — so a walk grading
 * the tile reads them here, in nanos, and lets the tile do its own rounding. */
export interface FleetCounters {
  budget_used_nanos: string;
  events_processed: string;
}

export async function readFleetCounters(
  handle: ClientHandle,
  workspaceId: string,
  fleetId: string,
): Promise<FleetCounters> {
  return clientFor(handle).get<FleetCounters>(
    `/v1/workspaces/${workspaceId}/fleets/${fleetId}`,
  );
}

export async function listFleets(handle: ClientHandle, workspaceId: string): Promise<Fleet[]> {
  const c = clientFor(handle);
  const res = await c.get<ListResp<Fleet>>(`/v1/workspaces/${workspaceId}/fleets`);
  return res.items;
}

export async function listWorkspaces(key: FixtureKey): Promise<Workspace[]> {
  const c = clientFor(key);
  const workspaces: Workspace[] = [];
  const seenCursors = new Set<string>();
  let startingAfter: string | null = null;
  do {
    const query = new URLSearchParams({ limit: "100" });
    if (startingAfter) query.set("starting_after", startingAfter);
    const page = await c.get<ListResp<Workspace>>(
      `/v1/tenants/me/workspaces?${query.toString()}`,
    );
    workspaces.push(...page.items);
    startingAfter = page.next_cursor ?? null;
    if (startingAfter !== null && seenCursors.has(startingAfter)) {
      throw new Error("Workspace pagination repeated a cursor");
    }
    if (startingAfter !== null) seenCursors.add(startingAfter);
  } while (startingAfter !== null);
  return workspaces;
}

interface CreateWorkspaceResp {
  workspace_id: string;
  name: string;
}

// POST /v1/workspaces — name is optional; server picks a Heroku-style name
// when omitted. Global setup and the workspace specs use it to ensure the
// fixture user has at least two workspaces for the WorkspaceSwitcher dropdown.
export async function ensureSecondWorkspace(
  key: FixtureKey,
  desiredName: string,
): Promise<Workspace> {
  const existing = await listWorkspaces(key);
  const match = existing.find((w) => (w.name ?? "") === desiredName);
  if (match) return match;
  const c = clientFor(key);
  const resp = await c.post<CreateWorkspaceResp>("/v1/workspaces", { name: desiredName });
  return { id: resp.workspace_id, name: resp.name };
}
