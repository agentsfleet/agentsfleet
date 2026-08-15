/**
 * Seed a live CLI login into a tmpdir-scoped `AGENTSFLEET_STATE_DIR` for the
 * acceptance suites.
 *
 * The suites mint a short-lived Clerk session JWT (`attachJwt`). Production
 * login spends that JWT exactly once to mint a durable `afc_…` credential;
 * this fixture mirrors the same exchange and persists only the result. The
 * session JWT remains available to callers for direct browser/API setup.
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

import {
  CLI_CREDENTIAL_PATTERN,
  MAX_MACHINE_NAME_LEN,
  MACHINE_NAME_DISALLOWED,
  MACHINE_NAME_REPLACEMENT,
} from "../../../src/constants/cli-credential.ts";

const TENANT_WORKSPACES_PATH = "/v1/tenants/me/workspaces";
const CLI_CREDENTIALS_PATH = "/v1/cli-credentials";
const MACHINE_NAME_PREFIX = "acceptance-";
const HTTP_UNAUTHORIZED = 401;
const HTTP_NOT_FOUND = 404;

export interface MintedCliCredential {
  readonly id: string;
  readonly credential: string;
}

const liveCredentials = new Map<string, MintedCliCredential>();

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
  readonly cliCredential: MintedCliCredential;
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

function machineNameFor(stateDir: string): string {
  return `${MACHINE_NAME_PREFIX}${path.basename(stateDir)}`
    .replace(MACHINE_NAME_DISALLOWED, MACHINE_NAME_REPLACEMENT)
    .slice(0, MAX_MACHINE_NAME_LEN);
}

export async function mintCliCredential(
  apiUrl: string,
  sessionToken: string,
  machineName: string,
): Promise<MintedCliCredential> {
  const response = await fetch(`${apiUrl}${CLI_CREDENTIALS_PATH}`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${sessionToken}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ machine_name: machineName }),
  });
  if (!response.ok) {
    const detail = await response.text().catch(() => "");
    throw new Error(`CLI credential mint ${response.status}: ${detail.slice(0, 200)}`);
  }
  const body = await response.json() as { id?: unknown; credential?: unknown };
  if (
    typeof body.id !== "string" ||
    typeof body.credential !== "string" ||
    !CLI_CREDENTIAL_PATTERN.test(body.credential)
  ) {
    throw new Error("CLI credential mint returned no usable credential");
  }
  const minted = { id: body.id, credential: body.credential };
  liveCredentials.set(machineName, minted);
  return minted;
}

export async function revokeHydratedCliCredentials(apiUrl: string): Promise<void> {
  const credentials = [...liveCredentials.values()];
  liveCredentials.clear();
  await Promise.all(credentials.map(async (minted) => {
    const response = await fetch(
      `${apiUrl}${CLI_CREDENTIALS_PATH}/${encodeURIComponent(minted.id)}`,
      { method: "DELETE", headers: { Authorization: `Bearer ${minted.credential}` } },
    );
    // A test may exercise `logout`, which already revoked this exact row and
    // credential. Treat that completed cleanup as success here.
    if (
      !response.ok &&
      response.status !== HTTP_UNAUTHORIZED &&
      response.status !== HTTP_NOT_FOUND
    ) {
      const detail = await response.text().catch(() => "");
      throw new Error(`CLI credential revoke ${response.status}: ${detail.slice(0, 200)}`);
    }
  }));
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
  const body = await res.json() as { items?: unknown; tenant_id?: unknown };
  if (typeof body.tenant_id !== "string" || body.tenant_id.length === 0) {
    throw new Error("hydrateWorkspacesForToken: response missing tenant_id");
  }
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
  const payload = { tenant_id: body.tenant_id, current_workspace_id, items };
  const cliCredential = await mintCliCredential(
    apiUrl,
    token,
    machineNameFor(stateDir),
  );

  await fs.mkdir(stateDir, { recursive: true });
  const target = path.join(stateDir, "workspaces.json");
  await fs.writeFile(target, `${JSON.stringify(payload, null, 2)}\n`, { mode: 0o600 });

  // Persist the exchanged credential, never the short-lived session JWT.
  // Matches src/lib/state.ts Credentials and the production login flow.
  const credentials = {
    token: cliCredential.credential,
    saved_at: Date.now(),
    session_id: null,
    api_url: apiUrl,
    credential_id: cliCredential.id,
  };
  const credentialsTarget = path.join(stateDir, "credentials.json");
  await fs.writeFile(credentialsTarget, `${JSON.stringify(credentials, null, 2)}\n`, { mode: 0o600 });

  return { currentWorkspaceId: current_workspace_id, workspaces: items, cliCredential };
}
