/**
 * Idempotent fixture seeding helpers.
 *
 * Fixture rows are conceptually tagged with `x-test-fixture: true` for
 * cleanup discrimination. agentsfleetd does not
 * currently read that header, but each fixture user has its own dedicated
 * tenant + workspace — every zombie in that workspace is a fixture row by
 * construction. Per-spec cleanup deletes everything in the fixture user's
 * workspace; no extra discriminator needed today.
 */
import { clientFor, type ClientHandle } from "./api-client";
import type { FixtureKey, ZombieStatus } from "./constants";

export interface Workspace {
  id: string;
  name: string | null;
}

export interface Zombie {
  id: string;
  name: string;
  status?: ZombieStatus;
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
  // Minimum valid shape for create_zombie. Mirrors
  // samples/fixtures/frontmatter/bundles/name_mismatch/TRIGGER.md: `triggers`
  // is a list, and `type: api` is rejected by the parser (config_helpers.zig)
  // — use a single `cron` trigger, the smallest valid shape.
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
  // Mirrors samples/fixtures/frontmatter/bundles/name_mismatch/SKILL.md.
  return [
    "---",
    `name: ${name}`,
    "description: Fixture skill body for e2e tests; echoes inputs, no side effects.",
    "version: 0.1.0",
    "---",
    "",
    `# ${name}`,
    "",
    "Body for fixture zombie used by e2e harness.",
    "",
  ].join("\n");
}

export interface SeedZombieOpts {
  name: string;
}

interface CreateZombieResp {
  zombie_id: string;
  name: string;
  status: string;
}

export async function seedZombie(
  key: FixtureKey,
  workspaceId: string,
  opts: SeedZombieOpts,
): Promise<Zombie> {
  const c = clientFor(key);
  // create_zombie returns `zombie_id`; list_zombies items have `id`. Normalize
  // to the listing shape so callers can compare against listZombies output.
  const resp = await c.post<CreateZombieResp>(`/v1/workspaces/${workspaceId}/zombies`, {
    trigger_markdown: triggerMd(opts.name),
    source_markdown: skillMd(opts.name),
  });
  return { id: resp.zombie_id, name: resp.name };
}

export async function listZombies(handle: ClientHandle, workspaceId: string): Promise<Zombie[]> {
  const c = clientFor(handle);
  const res = await c.get<ListResp<Zombie>>(`/v1/workspaces/${workspaceId}/zombies`);
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
