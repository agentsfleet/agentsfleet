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
  return `---\nname: ${name}\ntrigger: schedule\n---\n# ${name}\n\nFixture trigger for e2e tests.\n`;
}

function skillMd(name: string): string {
  return `# ${name}\n\nFixture skill body for e2e tests. Echoes inputs; no side effects.\n`;
}

export interface SeedZombieOpts {
  name: string;
}

export async function seedZombie(
  key: FixtureKey,
  workspaceId: string,
  opts: SeedZombieOpts,
): Promise<Zombie> {
  const c = clientFor(key);
  return c.post<Zombie>(`/v1/workspaces/${workspaceId}/zombies`, {
    trigger_markdown: triggerMd(opts.name),
    source_markdown: skillMd(opts.name),
  });
}

export async function listZombies(key: FixtureKey, workspaceId: string): Promise<Zombie[]> {
  const c = clientFor(key);
  const res = await c.get<ListResp<Zombie>>(`/v1/workspaces/${workspaceId}/zombies`);
  return res.items;
}
