/**
 * Minimal TS twin of `ui/packages/app/tests/e2e/acceptance/fixtures/clerk-admin.ts`.
 *
 * Chain: `provisionUser` → `ensureFixtureTenantBootstrapped` → `mintTokens`.
 * The CLI suite re-uses the same Clerk identity the dashboard suite uses, but
 * no longer depends on the dashboard's globalSetup having bootstrapped the
 * shared fixture first: in CI both suites only `needs: verify-dev` and run in
 * parallel, and a dev-DB reset wipes the workspace. `attachJwt` therefore
 * replays the `user.created` webhook itself (idempotent) before minting, so
 * the minted JWT lands on a tenant that already has its default workspace.
 *
 * JWT TTL is 900s (15 min, ~2× observed p95 suite wall-clock) — same posture
 * as the dashboard acceptance suite so a leaked .fixture-jwt is bounded by
 * the same window on both surfaces.
 */

import {
  CLERK_API_BASE,
  IS_TEST_FIXTURE_METADATA_KEY,
  JWT_TEMPLATE,
  SESSION_TOKEN_TTL_SECONDS,
} from "./constants.ts";
import { ensureFixtureTenantBootstrapped } from "./bootstrap.ts";

type ClerkMethod = "GET" | "POST";

interface ClerkUser {
  readonly id: string;
  readonly public_metadata?: Record<string, unknown> | undefined;
  readonly [key: string]: unknown;
}

interface ClerkSession {
  readonly id: string;
  readonly [key: string]: unknown;
}

interface ClerkToken {
  readonly jwt: string;
  readonly [key: string]: unknown;
}

interface ClerkSignInToken {
  readonly token: string;
  readonly [key: string]: unknown;
}

export interface MintedTokens {
  readonly sessionId: string;
  readonly sessionJwt: string;
  readonly cookieJwt: string;
}

export interface AttachedJwt extends MintedTokens {
  readonly clerkUserId: string;
  readonly email: string;
}

export interface ProvisionUserOptions {
  readonly email: string;
  readonly role?: string | undefined;
}

export interface MintTokensOptions {
  readonly ttlSeconds?: number | undefined;
}

export interface AttachJwtOptions {
  readonly email: string;
  readonly ttlSeconds?: number | undefined;
}

export interface EnsureFixtureTenantReadyOptions extends ProvisionUserOptions {}

type SessionOperationResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: unknown };

const FIXTURE_OWNER = "acceptance-e2e-suite";
const FIXTURE_OWNER_METADATA_KEY = "owner";
const FIXTURE_ROLE_METADATA_KEY = "role";
const FIXTURE_ROLE_REGULAR = "regular";
const REDACTED_VALUE = "[REDACTED]";
const S_CLERK_NOT_FOUND_RESPONSE = "\u2192 404:";
const ACTIVE_SESSION_STATUS = "active";
const SESSION_LIST_LIMIT = "10";
const mintedSessions = new Map<string, string>();

function authHeaders(clerkSecret: string): Record<string, string> {
  if (!clerkSecret) throw new Error("clerkSecret missing — pass CLERK_SECRET_KEY explicitly");
  return {
    Authorization: `Bearer ${clerkSecret}`,
    "Content-Type": "application/json",
  };
}

async function clerkRequest(
  clerkSecret: string,
  method: ClerkMethod,
  pathSuffix: string,
  body?: unknown,
  redactedValues: ReadonlyArray<string> = [],
): Promise<unknown> {
  const res = await fetch(`${CLERK_API_BASE}${pathSuffix}`, {
    method,
    headers: authHeaders(clerkSecret),
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    const detail = redactValues(await res.text(), redactedValues);
    throw new Error(`Clerk ${method} ${pathSuffix} → ${res.status}: ${detail}`);
  }
  return res.json();
}

function redactValues(input: string, values: ReadonlyArray<string>): string {
  return values.reduce(
    (redacted, value) => value ? redacted.replaceAll(value, REDACTED_VALUE) : redacted,
    input,
  );
}

async function findUserByEmail(clerkSecret: string, email: string): Promise<ClerkUser | null> {
  const params = new URLSearchParams({ email_address: email });
  const list = await clerkRequest(clerkSecret, "GET", `/users?${params.toString()}`);
  if (Array.isArray(list) && list.length > 0) {
    return list[0] as ClerkUser;
  }
  return null;
}

async function createUser(clerkSecret: string, opts: ProvisionUserOptions): Promise<ClerkUser> {
  const password = `${globalThis.crypto.randomUUID()}-${globalThis.crypto.randomUUID()}`;
  const result = await clerkRequest(clerkSecret, "POST", "/users", {
    email_address: [opts.email],
    password,
    skip_password_checks: true,
    skip_password_requirement: false,
    public_metadata: {
      [IS_TEST_FIXTURE_METADATA_KEY]: true,
      [FIXTURE_OWNER_METADATA_KEY]: FIXTURE_OWNER,
      [FIXTURE_ROLE_METADATA_KEY]: opts.role ?? FIXTURE_ROLE_REGULAR,
    },
  }, [password]);
  return result as ClerkUser;
}

function isOwnedFixtureUser(user: ClerkUser): boolean {
  const metadata = user.public_metadata;
  return metadata?.[IS_TEST_FIXTURE_METADATA_KEY] === true
    && metadata[FIXTURE_OWNER_METADATA_KEY] === FIXTURE_OWNER;
}

export async function provisionUser(
  clerkSecret: string,
  opts: ProvisionUserOptions,
): Promise<ClerkUser> {
  const existing = await findUserByEmail(clerkSecret, opts.email);
  if (existing) {
    if (!isOwnedFixtureUser(existing)) {
      throw new Error(
        `fixture ownership mismatch for ${opts.email}: refusing to adopt an existing Clerk identity`,
      );
    }
    return existing;
  }
  return createUser(clerkSecret, opts);
}

export async function mintTokens(
  clerkSecret: string,
  clerkUserId: string,
  opts?: MintTokensOptions,
): Promise<MintedTokens> {
  const session = await clerkRequest(clerkSecret, "POST", "/sessions", { user_id: clerkUserId }) as ClerkSession;
  mintedSessions.set(session.id, clerkSecret);
  const ttl = opts?.ttlSeconds ?? SESSION_TOKEN_TTL_SECONDS;
  // Two tokens per session: the template-minted JWT goes to the backend as
  // Bearer auth (carried via the env credential slot, AGENTSFLEET_API_KEY),
  // and the default (no-template) JWT goes into the `__session` cookie so
  // clerkMiddleware accepts the dashboard request.
  // Parallel mint matches the dashboard suite's posture verbatim.
  try {
    const [template, standard] = await Promise.all([
      clerkRequest(clerkSecret, "POST", `/sessions/${session.id}/tokens/${JWT_TEMPLATE}`,
        { expires_in_seconds: ttl }) as Promise<ClerkToken>,
      clerkRequest(clerkSecret, "POST", `/sessions/${session.id}/tokens`,
        { expires_in_seconds: ttl }) as Promise<ClerkToken>,
    ]);
    return { sessionId: session.id, sessionJwt: template.jwt, cookieJwt: standard.jwt };
  } catch (err: unknown) {
    try {
      await revokeSession(clerkSecret, session.id);
    } catch (cleanup_err: unknown) {
      throw new AggregateError([err, cleanup_err], "token minting and session revocation both failed");
    }
    throw err;
  }
}

export async function createSignInTicket(
  clerkSecret: string,
  clerkUserId: string,
): Promise<string> {
  const result = await clerkRequest(clerkSecret, "POST", "/sign_in_tokens", {
    user_id: clerkUserId,
    expires_in_seconds: 300,
  }) as ClerkSignInToken;
  return result.token;
}

// Clerk propagates publicMetadata (tenant_id/role) ASYNCHRONOUSLY after the
// bootstrap webhook's best-effort writeback (identity_events_clerk.zig writes it
// catch-and-warn, so the webhook 200 does NOT prove tenant_id has landed). The
// api-template JWT snapshots publicMetadata at mint time, so minting before
// tenant_id propagates yields a JWT agentsfleetd rejects with UZ-AUTH-001
// ("Tenant context required"). Poll until it appears — same posture as the
// dashboard suite's waitForTenantMetadata.
const CLERK_METADATA_POLL_MS = 500;
const CLERK_METADATA_TIMEOUT_MS = 15_000;
const TENANT_ID_METADATA_KEY = "tenant_id";

async function getUser(clerkSecret: string, userId: string): Promise<ClerkUser> {
  return await clerkRequest(clerkSecret, "GET", `/users/${userId}`) as ClerkUser;
}

function readTenantId(user: ClerkUser): string | null {
  const meta = user.public_metadata as Record<string, unknown> | undefined;
  const value = meta?.[TENANT_ID_METADATA_KEY];
  return typeof value === "string" ? value : null;
}

/**
 * Wait for Clerk to expose the tenant_id the CURRENT bootstrap produced.
 *
 * `stale` is the tenant_id present BEFORE this bootstrap. When the webhook
 * freshly created a tenant (`created === true`), the backend wrote a brand-new
 * tenant_id, so a poll that returns on `stale` would snapshot a JWT for the old
 * (now-deleted) tenant. Passing `stale` makes the poll wait until the value is
 * present AND different from it. On a replay (`stale` passed as null) the
 * existing value is already the correct tenant, so presence alone is enough.
 */
async function waitForTenantMetadata(clerkSecret: string, userId: string, stale: string | null): Promise<void> {
  const deadline = Date.now() + CLERK_METADATA_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const tenantId = readTenantId(await getUser(clerkSecret, userId));
    if (tenantId !== null && tenantId !== stale) return;
    await new Promise((resolve) => setTimeout(resolve, CLERK_METADATA_POLL_MS));
  }
  throw new Error(
    `Clerk user ${userId} public_metadata.${TENANT_ID_METADATA_KEY} never ` +
      `${stale === null ? "appeared" : "advanced past the stale value"} after ` +
      `${CLERK_METADATA_TIMEOUT_MS}ms — tenant bootstrap metadata did not propagate`,
  );
}

export async function ensureFixtureTenantReady(
  clerkSecret: string,
  opts: EnsureFixtureTenantReadyOptions,
): Promise<ClerkUser> {
  const user = await provisionUser(clerkSecret, opts);
  // Snapshot the tenant_id Clerk holds BEFORE this bootstrap — it may be stale
  // metadata from an older dev DB.
  const staleTenantId = readTenantId(await getUser(clerkSecret, user.id));
  // Replay user.created (idempotent); `created` distinguishes a fresh tenant from
  // an idempotent replay.
  const { created } = await ensureFixtureTenantBootstrapped({ clerkUserId: user.id, email: opts.email });
  // Wait for the tenant_id the backend writes back to propagate. On a fresh
  // create, REJECT the stale pre-bootstrap value so we don't mint a JWT for the
  // old tenant; on a replay the existing value is already correct. Minting before
  // the right tenant_id lands produces a JWT agentsfleetd rejects (UZ-AUTH-001).
  await waitForTenantMetadata(clerkSecret, user.id, created ? staleTenantId : null);
  return user;
}

export async function attachJwt(clerkSecret: string, opts: AttachJwtOptions): Promise<AttachedJwt> {
  const user = await ensureFixtureTenantReady(clerkSecret, { email: opts.email });
  const tokens = await mintTokens(clerkSecret, user.id, { ttlSeconds: opts.ttlSeconds });
  return { ...tokens, clerkUserId: user.id, email: opts.email };
}

export async function revokeSession(clerkSecret: string, sessionId: string): Promise<void> {
  try {
    await clerkRequest(clerkSecret, "POST", `/sessions/${sessionId}/revoke`);
  } catch (err: unknown) {
    if (!(err instanceof Error && err.message.includes(S_CLERK_NOT_FOUND_RESPONSE))) throw err;
  }
  mintedSessions.delete(sessionId);
}

export async function revokeMintedSessions(): Promise<void> {
  const sessions = [...mintedSessions.entries()];
  await Promise.all(sessions.map(([sessionId, clerkSecret]) =>
    revokeSession(clerkSecret, sessionId)
  ));
}

async function listClientSessionIds(clerkSecret: string, clientId: string): Promise<Set<string>> {
  const query = new URLSearchParams({ client_id: clientId, status: ACTIVE_SESSION_STATUS, limit: SESSION_LIST_LIMIT });
  const sessions = await clerkRequest(clerkSecret, "GET", `/sessions?${query.toString()}`);
  if (!Array.isArray(sessions)) throw new Error("Clerk session list returned a non-array");
  return new Set(sessions.map((session) => (session as ClerkSession).id));
}

export async function withClientSessionSweepOnFailure<T>(
  clerkSecret: string,
  clientId: string,
  operation: () => Promise<T>,
): Promise<T> {
  const previousIds = await listClientSessionIds(clerkSecret, clientId);
  try {
    return await operation();
  } catch (error: unknown) {
    try {
      const currentIds = await listClientSessionIds(clerkSecret, clientId);
      await Promise.all([...currentIds].filter((id) => !previousIds.has(id))
        .map((id) => revokeSession(clerkSecret, id)));
    } catch (cleanupError: unknown) {
      throw new AggregateError([error, cleanupError], "browser handoff and session sweep both failed");
    }
    throw error;
  }
}

export async function withSessionRevocation<T>(
  clerkSecret: string,
  sessionId: string,
  operation: () => Promise<T>,
): Promise<T> {
  let result: SessionOperationResult<T>;
  try {
    result = { ok: true, value: await operation() };
  } catch (error: unknown) {
    result = { ok: false, error };
  }
  try {
    await revokeSession(clerkSecret, sessionId);
  } catch (cleanupError: unknown) {
    if (!result.ok) {
      throw new AggregateError(
        [result.error, cleanupError],
        "browser handoff and session revocation both failed",
      );
    }
    throw cleanupError;
  }
  if (!result.ok) throw result.error;
  return result.value;
}
