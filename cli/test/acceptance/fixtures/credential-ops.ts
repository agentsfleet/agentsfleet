/**
 * Owned teardown helpers for the credential-vault acceptance slice.
 *
 * The shared `teardown.ts` cleans agents, not workspace credentials, so this
 * slice carries its own prefix-scoped sweep. Deletes go straight to the
 * workspace credential endpoints (the same routes the CLI drives) so a
 * crashed run can never leave a named secret behind in the shared DEV tenant.
 *
 * Nothing here asserts global emptiness — every read filters to the caller's
 * `runPrefix` so concurrent runs against the same tenant don't collide.
 */

const WORKSPACE_CREDENTIALS_PATH = (apiUrl: string, wsId: string): string =>
  `${apiUrl}/v1/workspaces/${encodeURIComponent(wsId)}/credentials`;

const WORKSPACE_CREDENTIAL_PATH = (apiUrl: string, wsId: string, name: string): string =>
  `${WORKSPACE_CREDENTIALS_PATH(apiUrl, wsId)}/${encodeURIComponent(name)}`;

const METHOD_GET = "GET" as const;
const METHOD_DELETE = "DELETE" as const;
const CONTENT_TYPE_JSON = "application/json" as const;
const HEADER_CONTENT_TYPE = "Content-Type" as const;
const HEADER_AUTHORIZATION = "Authorization" as const;

export interface CredentialOpsContext {
  readonly apiUrl: string;
  readonly token: string;
  readonly workspaceId: string;
}

export interface SweepOptions {
  readonly runPrefix: string;
}

interface RawCredentialRow {
  readonly name?: unknown;
}

const authHeaders = (token: string): Record<string, string> => ({
  [HEADER_AUTHORIZATION]: `Bearer ${token}`,
  [HEADER_CONTENT_TYPE]: CONTENT_TYPE_JSON,
});

/** Names of every credential currently in the vault — unfiltered. */
export async function listCredentialNames(ctx: CredentialOpsContext): Promise<ReadonlyArray<string>> {
  const res = await fetch(WORKSPACE_CREDENTIALS_PATH(ctx.apiUrl, ctx.workspaceId), {
    method: METHOD_GET,
    headers: authHeaders(ctx.token),
  });
  if (!res.ok) {
    const detail = await res.text().catch(() => "");
    throw new Error(`credential list ${res.status}: ${detail.slice(0, 200)}`);
  }
  const body = (await res.json()) as { credentials?: ReadonlyArray<RawCredentialRow> };
  const rows = Array.isArray(body.credentials) ? body.credentials : [];
  return rows
    .map((row) => (typeof row.name === "string" ? row.name : null))
    .filter((name): name is string => name !== null);
}

/** Best-effort delete of a single named credential; swallows not-found. */
export async function deleteCredential(ctx: CredentialOpsContext, name: string): Promise<void> {
  const res = await fetch(WORKSPACE_CREDENTIAL_PATH(ctx.apiUrl, ctx.workspaceId, name), {
    method: METHOD_DELETE,
    headers: authHeaders(ctx.token),
  });
  // 404 is acceptable — the row was already gone (the spec's own delete ran).
  if (!res.ok && res.status !== 404) {
    const detail = await res.text().catch(() => "");
    throw new Error(`credential delete ${res.status}: ${detail.slice(0, 200)}`);
  }
}

/**
 * Delete every credential whose name starts with `runPrefix`. Iterates a
 * fresh list so a partial failure on one row never strands the rest.
 */
export async function sweepCredentials(
  ctx: CredentialOpsContext,
  opts: SweepOptions,
): Promise<void> {
  const names = await listCredentialNames(ctx);
  const mine = names.filter((name) => name.startsWith(opts.runPrefix));
  for (const name of mine) {
    try {
      await deleteCredential(ctx, name);
    } catch {
      /* best-effort teardown — never throw out of afterAll */
    }
  }
}
