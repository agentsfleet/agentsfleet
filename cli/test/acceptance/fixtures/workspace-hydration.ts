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
 * `workspace use` (idempotent) or pass `--workspace` per command.
 */

import { isInstant } from "../../../src/lib/instant.ts";
import fs from "node:fs/promises";
import path from "node:path";

import {
  CLI_CREDENTIAL_PATTERN,
  MAX_MACHINE_NAME_LEN,
  MACHINE_NAME_DISALLOWED,
  MACHINE_NAME_REPLACEMENT,
} from "../../../src/constants/cli-credential.ts";
import {
  CLI_CREDENTIALS_PATH,
  type MintedCliCredential,
  REVOKE_LABEL,
  revokeCliCredential,
} from "./cli-credential-revoke.ts";
import {
  ATTEMPT_TIMEOUT_MS,
  type RetryFault,
  type RetryOptions,
  retryAfterMs,
  UNPROCESSED_STATUSES,
  withRetries,
} from "./retryable.ts";

const TENANT_WORKSPACES_PATH = "/v1/tenants/me/workspaces";
const MACHINE_NAME_PREFIX = "acceptance-";
const ERROR_DETAIL_MAX_CHARS = 200;

const liveCredentials = new Map<string, MintedCliCredential>();

export interface HydratedWorkspace {
  readonly workspace_id: string;
  readonly name: string | null;
  readonly created_at: string;
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
  fallbackCreatedAt: string,
): HydratedWorkspace | null {
  if (!item || typeof item !== "object") return null;
  const workspaceId = typeof item.workspace_id === "string"
    ? item.workspace_id
    : typeof item.id === "string" ? item.id : null;
  if (!workspaceId) return null;
  return {
    workspace_id: workspaceId,
    name: typeof item.name === "string" ? item.name : null,
    created_at: isInstant(item.created_at) ? item.created_at : fallbackCreatedAt,
  };
}

function machineNameFor(stateDir: string): string {
  return `${MACHINE_NAME_PREFIX}${path.basename(stateDir)}`
    .replace(MACHINE_NAME_DISALLOWED, MACHINE_NAME_REPLACEMENT)
    .slice(0, MAX_MACHINE_NAME_LEN);
}

/** One mint attempt that did not answer a credential; never thrown by the attempt. */
interface MintFault extends RetryFault {
  readonly message: string;
}

/** One POST. Answers the response, or the fault the retry loop decides on. */
async function mintOnce(
  apiUrl: string,
  sessionToken: string,
  machineName: string,
): Promise<{ readonly ok: Response } | { readonly fault: MintFault }> {
  let response: Response;
  try {
    response = await fetch(`${apiUrl}${CLI_CREDENTIALS_PATH}`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${sessionToken}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ machine_name: machineName }),
      signal: AbortSignal.timeout(ATTEMPT_TIMEOUT_MS),
    });
  } catch (cause: unknown) {
    // A connection that never opened says nothing landed; a timeout may
    // follow a mint that did, and a second mint would leave the first live.
    const message = `CLI credential mint never reached the API: ${String(cause)}`;
    return { fault: { message, retryable: cause instanceof TypeError, retryAfterMs: null } };
  }
  if (response.ok) return { ok: response };
  const detail = await response.text().catch(() => "");
  const message = `CLI credential mint ${response.status}: ${detail.slice(0, ERROR_DETAIL_MAX_CHARS)}`;
  return {
    fault: {
      message,
      retryable: UNPROCESSED_STATUSES.has(response.status),
      retryAfterMs: retryAfterMs(response),
    },
  };
}

/**
 * Mints a durable CLI credential for `machineName`, retrying only the answers
 * that prove nothing was minted (`UNPROCESSED_STATUSES`), so a retry never
 * leaves a second live credential behind the first.
 */
export async function mintCliCredential(
  apiUrl: string,
  sessionToken: string,
  machineName: string,
  options: RetryOptions = {},
): Promise<MintedCliCredential> {
  const retried = await withRetries(() => mintOnce(apiUrl, sessionToken, machineName), options);
  if (!("ok" in retried)) {
    const { last, faults } = retried;
    if (faults.length === 1) throw new Error(last.message);
    throw new Error(`${last.message} (gave up after ${faults.length} attempts)`, {
      cause: new AggregateError(faults.map((fault) => new Error(fault.message))),
    });
  }
  const body = await retried.ok.json() as { id?: unknown; credential?: unknown };
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

/**
 * Revokes every credential this process minted, waiting for all of them. One
 * the API no longer has is forgotten; one whose revoke gave up stays
 * registered and is named in the error.
 */
export async function revokeHydratedCliCredentials(
  apiUrl: string,
  options: RetryOptions = {},
): Promise<void> {
  const credentials = [...liveCredentials.entries()];
  const failures: unknown[] = [];
  await Promise.all(credentials.map(async ([machineName, minted]) => {
    try {
      await revokeCliCredential(apiUrl, minted, options);
    } catch (error: unknown) {
      failures.push(error);
      return;
    }
    if (liveCredentials.get(machineName)?.id === minted.id) {
      liveCredentials.delete(machineName);
    }
  }));
  if (failures.length === 1) throw failures[0];
  if (failures.length > 1) {
    throw new AggregateError(failures, `${REVOKE_LABEL}: ${failures.length} credentials are still live`);
  }
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
    throw new Error(`workspace hydrate ${res.status}: ${detail.slice(0, ERROR_DETAIL_MAX_CHARS)}`);
  }
  const body = await res.json() as { items?: unknown; tenant_id?: unknown };
  if (typeof body.tenant_id !== "string" || body.tenant_id.length === 0) {
    throw new Error("hydrateWorkspacesForToken: response missing tenant_id");
  }
  const fallbackCreatedAt = new Date().toISOString();
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
