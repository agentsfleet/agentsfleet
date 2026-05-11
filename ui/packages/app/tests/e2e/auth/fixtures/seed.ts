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
import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
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

// Worktree-root resolution mirrors install-zombie-cli.spec.ts:WORKTREE_ROOT,
// adjusted for the extra fixtures/ nesting layer.
const SEED_DIR = path.dirname(fileURLToPath(import.meta.url));
const WORKTREE_ROOT = path.resolve(SEED_DIR, "../../../../../../..");
const PLATFORM_OPS_DIR = path.join(WORKTREE_ROOT, "samples/platform-ops");

// Seeds the canonical samples/platform-ops bundle byte-for-byte (same content
// the CLI install path consumes). Both lifecycle scenarios route their install
// step here so the post-install state is identical across signup and
// existing-fixture flows. Accepts ClientHandle so the mid-test-minted JWT
// path (ephemeral signup user) drives the seed the same way persistent
// fixtures do.
export async function seedPlatformOpsZombie(
  handle: ClientHandle,
  workspaceId: string,
): Promise<Zombie> {
  const triggerPath = path.join(PLATFORM_OPS_DIR, "TRIGGER.md");
  const skillPath = path.join(PLATFORM_OPS_DIR, "SKILL.md");
  if (!fs.existsSync(triggerPath) || !fs.existsSync(skillPath)) {
    throw new Error(
      `samples/platform-ops bundle missing: expected ${triggerPath} and ${skillPath}`,
    );
  }
  const c = clientFor(handle);
  const resp = await c.post<CreateZombieResp>(`/v1/workspaces/${workspaceId}/zombies`, {
    trigger_markdown: fs.readFileSync(triggerPath, "utf8"),
    source_markdown: fs.readFileSync(skillPath, "utf8"),
  });
  return { id: resp.zombie_id, name: resp.name };
}

export async function listZombies(key: FixtureKey, workspaceId: string): Promise<Zombie[]> {
  const c = clientFor(key);
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
