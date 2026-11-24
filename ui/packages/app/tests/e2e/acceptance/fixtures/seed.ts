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
import { awaitFirstTurnSettled, createFixtureFleet } from "./fleet-create";
import { probeMessage } from "./probe";
import { clientFor } from "./api-client";

/** What a seeded fleet's first message asks: one line back and no tool, the cheapest first turn there is. */
const SEED_TOKEN = "ready";

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
  /** The first message; a fleet cannot exist without one. Defaults to a one-line probe. */
  message?: string;
}

/**
 * A fleet and its first turn, through `POST W/fleets`, returned once that turn
 * has settled: a seeded fleet is quiet when the spec starts, as one with no
 * history used to be. A sibling holding the name moves this one to `name-2`.
 */
export async function seedFleet(key: FixtureKey, workspaceId: string, opts: SeedFleetOpts): Promise<Fleet> {
  const {fleet, turnId} = await createFixtureFleet(key, workspaceId, {name: opts.name, message: opts.message ?? probeMessage(SEED_TOKEN)});
  await awaitFirstTurnSettled(key, workspaceId, fleet.id, turnId);
  return { id: fleet.id, name: fleet.name };
}

/**
 * A fleet is born `active`: its create commits the fleet and its first turn
 * together, so there is nothing to wait for. Anything else — archived, or a
 * status this suite does not know — fails loudly instead of letting a spec
 * walk a fleet that can never render a console.
 */
export async function expectFleetActive(
  handle: ClientHandle,
  workspaceId: string,
  fleetId: string,
): Promise<void> {
  const fleet = await clientFor(handle).get<{ status: string }>(
    `/v1/workspaces/${workspaceId}/fleets/${fleetId}`,
  );
  if (fleet.status !== "active") {
    throw new Error(`[e2e:seed] fleet ${fleetId} is "${fleet.status}", not active`);
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

/** The fleet's durable lifetime totals, read independently of rounded UI figures. */
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
