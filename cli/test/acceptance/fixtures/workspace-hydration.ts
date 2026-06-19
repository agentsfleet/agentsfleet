/**
 * Seed a minted-JWT session into a tmpdir-scoped `AGENTSFLEET_STATE_DIR` for
 * the live acceptance suites.
 *
 * The suites mint a Clerk JWT (`attachJwt`) and need the CLI to run as that
 * session. With the `AGENTSFLEET_TOKEN` env var removed, the only bearer
 * surfaces are the stored login (credentials.json, file slot) and the
 * service API key (`AGENTSFLEET_API_KEY`, env slot). The fixtures hold a
 * login-shaped JWT, not an `agt_t` key, so it belongs in the file slot — we
 * write it to `credentials.json` exactly as the login flow's persist step
 * would (`saveAccessToken` → src/lib/state.ts `Credentials` shape).
 *
 * We also hydrate `workspaces.json`: the CLI populates it only inside the
 * login post-success branch (`hydrateWorkspacesAfterLogin`), which the
 * direct-JWT path never walks — so without it the read-only sweep sees an
 * empty local list even though the tenant has workspaces. This helper hits
 * `/v1/tenants/me/workspaces` with the bearer and writes the normalised
 * list. Returns the picked current workspace id so callers can chain into
 * `workspace use` (idempotent) or pass `--workspace-id` per command.
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

  // Seed the login session (file slot) so commands authenticate without the
  // removed `AGENTSFLEET_TOKEN` env var. Matches src/lib/state.ts Credentials.
  const credentials = {
    token,
    saved_at: Date.now(),
    session_id: null,
    api_url: apiUrl,
  };
  const credentialsTarget = path.join(stateDir, "credentials.json");
  await fs.writeFile(credentialsTarget, `${JSON.stringify(credentials, null, 2)}\n`, { mode: 0o600 });

  return { currentWorkspaceId: current_workspace_id, workspaces: items };
}
