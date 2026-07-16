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
import { SOURCE_KIND_UPLOAD } from "@/lib/types";
import { clientFor } from "./api-client";

const FIXTURE_LIBRARY_NAME = "acceptance-seed";

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

function triggerMd(name: string): string {
  // Use cron here so seeded fleets keep a concrete wake rule.
  return [
    "---",
    `name: ${name}`,
    "",
    "x-agentsfleet:",
    "  triggers:",
    "    - type: cron",
    '      schedule: "0 0 * * *"',
    "  tools:",
    "    - agentmail",
    "  budget:",
    "    daily_dollars: 1.0",
    "---",
    "",
  ].join("\n");
}

function skillMd(name: string): string {
  // SKILL.md frontmatter requires name (kebab), description, version (semver).
  // Mirrors tests/fixtures/fleetbundle/skill/name_mismatch/SKILL.md.
  return [
    "---",
    `name: ${name}`,
    "description: Fixture skill body for e2e tests; echoes inputs, no side effects.",
    "version: 0.1.0",
    "---",
    "",
    `# ${name}`,
    "",
    "Body for fixture fleet used by e2e harness.",
    "",
  ].join("\n");
}

export interface SeedFleetOpts {
  name: string;
}

interface CreateFleetResp {
  fleet_id: string;
  name: string;
  status: string;
}

interface OnboardTemplateResp {
  id: string;
}

async function onboardFixtureLibrary(
  client: ReturnType<typeof clientFor>,
  workspaceId: string,
): Promise<string> {
  const resp = await client.post<OnboardTemplateResp>(
    `/v1/workspaces/${workspaceId}/fleet-libraries`,
    {
      source_kind: SOURCE_KIND_UPLOAD,
      skill_markdown: skillMd(FIXTURE_LIBRARY_NAME),
      trigger_markdown: triggerMd(FIXTURE_LIBRARY_NAME),
    },
  );
  return resp.id;
}

export async function seedFleet(
  key: FixtureKey,
  workspaceId: string,
  opts: SeedFleetOpts,
): Promise<Fleet> {
  const c = clientFor(key);
  const tenantLibraryId = await onboardFixtureLibrary(c, workspaceId);
  // create_fleet returns `fleet_id`; list_fleets items have `id`. Normalize
  // to the listing shape so callers can compare against listFleets output.
  const resp = await c.post<CreateFleetResp>(`/v1/workspaces/${workspaceId}/fleets`, {
    tenant_library_id: tenantLibraryId,
    name: opts.name,
  });
  return { id: resp.fleet_id, name: resp.name };
}

export async function listFleets(handle: ClientHandle, workspaceId: string): Promise<Fleet[]> {
  const c = clientFor(handle);
  const res = await c.get<ListResp<Fleet>>(`/v1/workspaces/${workspaceId}/fleets`);
  return res.items;
}

export async function listWorkspaces(key: FixtureKey): Promise<Workspace[]> {
  const c = clientFor(key);
  const res = await c.get<ListResp<Workspace>>("/v1/tenants/me/workspaces");
  return res.items;
}

interface CreateWorkspaceResp {
  workspace_id: string;
  name: string;
}

// POST /v1/workspaces — name is optional; server picks a Heroku-style name
// when omitted. Used by multi-workspace.spec.ts to ensure the fixture user
// has at least two workspaces for the WorkspaceSwitcher dropdown.
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
