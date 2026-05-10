/**
 * Idempotent fixture seeding helpers.
 *
 * Per the M64_005 spec, fixture rows are conceptually tagged with
 * `x-test-fixture: true` for cleanup discrimination. zombied does not
 * currently read that header, but each fixture user has its own dedicated
 * tenant + workspace — every zombie in that workspace is a fixture row by
 * construction. Per-spec cleanup deletes everything in the fixture user's
 * workspace; no extra discriminator needed today.
 */
import { clientFor } from "./api-client";
import type { FixtureKey } from "./auth";

export interface Workspace {
  id: string;
  name: string | null;
}

export interface Zombie {
  id: string;
  name: string;
  status?: string;
}

interface ListResp<T> {
  items: T[];
  total: number;
}

export async function getDefaultWorkspaceId(key: FixtureKey): Promise<string> {
  const c = clientFor(key);
  const res = await c.get<ListResp<Workspace>>("/v1/tenants/me/workspaces");
  if (res.items.length === 0) {
    throw new Error(`Fixture user '${key}' has no workspace; bootstrap step must have failed.`);
  }
  return res.items[0]!.id;
}

function triggerMd(name: string): string {
  // Minimum valid shape for create_zombie. Mirrors
  // samples/fixtures/frontmatter/bundles/name_mismatch/TRIGGER.md.
  return [
    "---",
    `name: ${name}`,
    "",
    "x-usezombie:",
    "  trigger:",
    "    type: api",
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

export async function listZombies(key: FixtureKey, workspaceId: string): Promise<Zombie[]> {
  const c = clientFor(key);
  const res = await c.get<ListResp<Zombie>>(`/v1/workspaces/${workspaceId}/zombies`);
  return res.items;
}
