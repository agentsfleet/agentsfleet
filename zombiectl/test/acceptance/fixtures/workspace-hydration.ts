/**
 * Hydrate workspaces.json for the ZOMBIE_TOKEN-injection suite.
 *
 * The CLI populates workspaces.json only inside `commandLogin`'s
 * post-success branch (`hydrateWorkspacesAfterLogin` in src/commands/core.ts).
 * §4 injects a minted JWT directly via `ZOMBIE_TOKEN`, so it never walks
 * that path — without help the read-only sweep sees an empty local list
 * even though the tenant has workspaces.
 *
 * This helper mirrors the hydrate flow: hits `/v1/tenants/me/workspaces`
 * with the bearer token, writes the normalised list to the suite's
 * tmpdir-scoped `ZOMBIE_STATE_DIR`. Returns the picked current workspace
 * id so callers can chain into `workspace use` (idempotent) or pass via
 * `--workspace-id` per command.
 *
 * Discovery: the CLI lacks a `workspace pull` / `workspace sync` command
 * that an operator running under explicit `ZOMBIE_TOKEN` could call to
 * hydrate. Follow-on PR can add one — at which point this fixture
 * collapses to one CLI invocation.
 */

import fs from "node:fs/promises";
import path from "node:path";

const TENANT_WORKSPACES_PATH = "/v1/tenants/me/workspaces";

export interface HydratedWorkspace {
  readonly workspace_id: string;
  readonly name: string | null;
  readonly created_at: number;
}

export interface HydrateOptions {
  readonly apiUrl: string;
  readonly token: string;
  readonly stateDir: string;
}

export interface HydrateResult {
  readonly currentWorkspaceId: string;
  readonly workspaces: ReadonlyArray<HydratedWorkspace>;
}

interface RawWorkspaceItem {
  workspace_id?: unknown;
  id?: unknown;
  name?: unknown;
  created_at?: unknown;
}

function normalizeWorkspace(
  item: RawWorkspaceItem | null | undefined,
  fallbackCreatedAt: number,
): HydratedWorkspace | null {
  if (!item || typeof item !== "object") return null;
  const workspaceId = typeof item.workspace_id === "string"
    ? item.workspace_id
    : typeof item.id === "string" ? item.id : null;
  if (!workspaceId) return null;
  return {
    workspace_id: workspaceId,
    name: typeof item.name === "string" ? item.name : null,
    created_at: Number.isFinite(item.created_at) ? item.created_at as number : fallbackCreatedAt,
  };
}

export async function hydrateWorkspacesForToken(opts: HydrateOptions): Promise<HydrateResult> {
  const { apiUrl, token, stateDir } = opts;
  if (!apiUrl) throw new Error("hydrateWorkspacesForToken: apiUrl required");
  if (!token) throw new Error("hydrateWorkspacesForToken: token required");
  if (!stateDir) throw new Error("hydrateWorkspacesForToken: stateDir required");

  const res = await fetch(`${apiUrl}${TENANT_WORKSPACES_PATH}`, {
    method: "GET",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
  });
  if (!res.ok) {
    const detail = await res.text().catch(() => "");
    throw new Error(`workspace hydrate ${res.status}: ${detail.slice(0, 200)}`);
  }
  const body = await res.json() as { items?: unknown };
  const fallbackCreatedAt = Date.now();
  const rawItems: RawWorkspaceItem[] = Array.isArray(body?.items) ? body.items as RawWorkspaceItem[] : [];
  const items: HydratedWorkspace[] = rawItems
    .map((item) => normalizeWorkspace(item, fallbackCreatedAt))
    .filter((w): w is HydratedWorkspace => w !== null);
  const first = items[0];
  if (!first) {
    throw new Error("hydrateWorkspacesForToken: tenant has no workspaces — fixture identity is mis-bootstrapped");
  }
  const current_workspace_id = first.workspace_id;
  const payload = { current_workspace_id, items };

  await fs.mkdir(stateDir, { recursive: true });
  const target = path.join(stateDir, "workspaces.json");
  await fs.writeFile(target, `${JSON.stringify(payload, null, 2)}\n`, { mode: 0o600 });
  return { currentWorkspaceId: current_workspace_id, workspaces: items };
}
