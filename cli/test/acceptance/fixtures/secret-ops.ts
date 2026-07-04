/**
 * Owned teardown helpers for the secret-vault acceptance slice.
 *
 * The shared `teardown.ts` cleans agents, not workspace secrets, so this
 * slice carries its own prefix-scoped sweep. Deletes go straight to the
 * workspace secret endpoints (the same routes the CLI drives) so a
 * crashed run can never leave a named secret behind in the shared DEV tenant.
 *
 * Nothing here asserts global emptiness — every read filters to the caller's
 * `runPrefix` so concurrent runs against the same tenant don't collide.
 */

const WORKSPACE_SECRETS_PATH = (apiUrl: string, wsId: string): string =>
  `${apiUrl}/v1/workspaces/${encodeURIComponent(wsId)}/secrets`;

const WORKSPACE_SECRET_PATH = (apiUrl: string, wsId: string, name: string): string =>
  `${WORKSPACE_SECRETS_PATH(apiUrl, wsId)}/${encodeURIComponent(name)}`;

const METHOD_GET = "GET" as const;
const METHOD_DELETE = "DELETE" as const;
const CONTENT_TYPE_JSON = "application/json" as const;
const HEADER_CONTENT_TYPE = "Content-Type" as const;
const HEADER_AUTHORIZATION = "Authorization" as const;

export interface SecretOpsContext {
  readonly apiUrl: string;
  readonly token: string;
  readonly workspaceId: string;
}

export interface SweepOptions {
  readonly runPrefix: string;
}

interface RawSecretRow {
  readonly name?: unknown;
}

const authHeaders = (token: string): Record<string, string> => ({
  [HEADER_AUTHORIZATION]: `Bearer ${token}`,
  [HEADER_CONTENT_TYPE]: CONTENT_TYPE_JSON,
});

/** Names of every secret currently in the vault — unfiltered. */
export async function listSecretNames(ctx: SecretOpsContext): Promise<ReadonlyArray<string>> {
  const res = await fetch(WORKSPACE_SECRETS_PATH(ctx.apiUrl, ctx.workspaceId), {
    method: METHOD_GET,
    headers: authHeaders(ctx.token),
  });
  if (!res.ok) {
    const detail = await res.text().catch(() => "");
    throw new Error(`secret list ${res.status}: ${detail.slice(0, 200)}`);
  }
  const body = (await res.json()) as { secrets?: ReadonlyArray<RawSecretRow> };
  const rows = Array.isArray(body.secrets) ? body.secrets : [];
  return rows
    .map((row) => (typeof row.name === "string" ? row.name : null))
    .filter((name): name is string => name !== null);
}

/** Best-effort delete of a single named secret; swallows not-found. */
export async function deleteSecret(ctx: SecretOpsContext, name: string): Promise<void> {
  const res = await fetch(WORKSPACE_SECRET_PATH(ctx.apiUrl, ctx.workspaceId, name), {
    method: METHOD_DELETE,
    headers: authHeaders(ctx.token),
  });
  // 404 is acceptable — the row was already gone (the spec's own delete ran).
  if (!res.ok && res.status !== 404) {
    const detail = await res.text().catch(() => "");
    throw new Error(`secret delete ${res.status}: ${detail.slice(0, 200)}`);
  }
}

/**
 * Delete every secret whose name starts with `runPrefix`. Iterates a
 * fresh list so a partial failure on one row never strands the rest.
 */
export async function sweepSecrets(
  ctx: SecretOpsContext,
  opts: SweepOptions,
): Promise<void> {
  const names = await listSecretNames(ctx);
  const mine = names.filter((name) => name.startsWith(opts.runPrefix));
  for (const name of mine) {
    try {
      await deleteSecret(ctx, name);
    } catch {
      /* best-effort teardown — never throw out of afterAll */
    }
  }
}
