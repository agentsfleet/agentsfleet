/**
 * Clerk backend-API helpers for the e2e harness.
 *
 * Idempotent fixture-user provisioning + customized-session JWT mint via
 * Clerk's REST API. Wire shape: GET /v1/users → POST /v1/sessions → POST
 * /v1/sessions/{id}/tokens. Post-the mint uses the bare
 * `/tokens` endpoint (no template suffix) — the customized default
 * session token carries `aud=https://api.agentsfleet.net` +
 * `metadata.tenant_id` + `metadata.role`, which is exactly what agentsfleetd's
 * OIDC verifier and tenant-scope check rely on.
 *
 * The cookie-side of "sign in" is handled by `clerk.signIn` from
 * `@clerk/testing/playwright` (see `fixtures/auth.ts`) — that helper mints
 * a single-use ticket via Backend API + drives `Clerk.signIn.create` in
 * the page, so clerk-js owns every session cookie. The harness only owns
 * the customized-session Bearer JWT that direct agentsfleetd calls (seed/
 * teardown) use; clerkMiddleware never sees it.
 *
 * Uses fetch directly. No @clerk/backend SDK — the surface is small and
 * stable, and the SDK pulls in node:crypto-heavy deps we don't need.
 */

import { FIXTURE_KEY, OPERATOR_FIXTURE_SCOPES, type FixtureKey } from "./constants";
import { clerkRequest } from "./clerk-request";

// Session-token TTL for minted fixture JWTs. Default Clerk TTL is 60s — too
// short for a full suite run. The harness uses 15 min, which is ~2× the
// observed p95 wall-clock on CI (suites complete in ~5 min). Tighter than
// the historical 3600s posture to bound the impact of a leaked
// .fixture-jwts.json file. `clientFor` callers that exceed this window
// will fail loud with a 401 from agentsfleetd — re-mint if/when that happens.
const SESSION_TOKEN_TTL_SECONDS = 900;
const CLERK_METADATA_POLL_MS = 500;
const CLERK_METADATA_TIMEOUT_MS = 15_000;
const TENANT_ID_METADATA_KEY = "tenant_id";
const TENANT_SCOPE_SENTINEL = "workspace:admin";

export interface FixtureUserSpec {
  key: FixtureKey;
  email: string;
  password: string;
}

export interface MintedFixture {
  key: FixtureKey;
  email: string;
  password: string;
  clerkUserId: string;
  /** Clerk session id — used by globalTeardown to revoke. */
  sessionId: string;
  /**
   * Customized default session JWT — Bearer auth on agentsfleetd. Carries
   * `aud=https://api.agentsfleet.net` + `metadata.tenant_id` + `metadata.role`
   * via Clerk's Session Token Customization, not via the
   * legacy `api` JWT template.
   */
  sessionJwt: string;
}

interface ClerkUser {
  id: string;
  email_addresses: Array<{ email_address: string }>;
  public_metadata?: Record<string, unknown>;
}

interface ClerkSession {
  id: string;
}

interface ClerkSessionToken {
  jwt: string;
}

interface ClerkSignInToken {
  token: string;
}

async function findUserByEmail(email: string): Promise<ClerkUser | null> {
  const params = new URLSearchParams({ email_address: email });
  const list = await clerkRequest<ClerkUser[]>("GET", `/users?${params.toString()}`);
  return list[0] ?? null;
}

async function getUser(userId: string): Promise<ClerkUser> {
  return clerkRequest<ClerkUser>("GET", `/users/${userId}`);
}

// public_metadata.is_test_fixture lets prod ops dashboards filter these
// identities out, and gives a future Clerk webhook handler a hook to refuse
// unsafe operations against fixture users.
//
// `scopes` is present only on the operator fixture. The session-token template
// projects it to the top-level `scopes` claim that agentsfleetd's requireScope
// and the dashboard's readSessionScopes both read; every other fixture is
// deliberately scope-free, so a spec asserting an operator surface is hidden
// stays meaningful. Built once and shared by the create and backfill paths so
// the two can never drift.
function fixtureMetadata(spec: FixtureUserSpec): Record<string, unknown> {
  const metadata: Record<string, unknown> = {
    is_test_fixture: true,
    owner: "acceptance-e2e-suite",
    role: spec.key,
  };
  if (spec.key === FIXTURE_KEY.operator) {
    metadata.scopes = [...OPERATOR_FIXTURE_SCOPES];
  }
  return metadata;
}

async function createUser(spec: FixtureUserSpec): Promise<ClerkUser> {
  return clerkRequest<ClerkUser>("POST", "/users", {
    email_address: [spec.email],
    password: spec.password,
    skip_password_checks: true,
    skip_password_requirement: false,
    public_metadata: fixtureMetadata(spec),
  });
}

async function ensureUser(spec: FixtureUserSpec): Promise<ClerkUser> {
  const existing = await findUserByEmail(spec.email);
  if (existing) {
    // Backfill the metadata on pre-existing fixture users — this is also what
    // grants the operator scope to an operator fixture created before the scope
    // existed.
    return clerkRequest<ClerkUser>("PATCH", `/users/${existing.id}/metadata`, {
      public_metadata: fixtureMetadata(spec),
    });
  }
  return createUser(spec);
}

function scopeValues(raw: unknown): string[] {
  if (typeof raw === "string") return raw.split(/\s+/).filter(Boolean);
  if (Array.isArray(raw)) return raw.filter((value): value is string => typeof value === "string");
  return [];
}

export async function finalizeFixtureMetadata(user: ProvisionedUser): Promise<void> {
  await waitForTenantMetadata(user.clerkUserId);
  const current = await getUser(user.clerkUserId);
  const publicMetadata = fixtureMetadata(user);
  if (user.key === FIXTURE_KEY.operator) {
    publicMetadata.scopes = [
      ...new Set([
        ...scopeValues(current.public_metadata?.scopes),
        ...OPERATOR_FIXTURE_SCOPES,
      ]),
    ].join(" ");
  }
  await clerkRequest<ClerkUser>("PATCH", `/users/${user.clerkUserId}/metadata`, {
    public_metadata: publicMetadata,
  });
}

/**
 * Mint an api-template session JWT for direct agentsfleetd calls.
 *
 * Carries `metadata.tenant_id` + `metadata.role` from publicMetadata; used
 * as Bearer auth on agentsfleetd. Default (non-template) session tokens omit
 * publicMetadata, which would land at 403 UZ-AUTH-001 ("Tenant context
 * required"). The cookie-side of "sign in" is owned by clerk-js via
 * `clerk.signIn` in `fixtures/auth.ts`; this helper mints only the
 * out-of-band Bearer the harness uses for direct API calls (seed,
 * teardown, listing).
 *
 * Default Clerk session-token TTL is 60s, far shorter than a full e2e
 * suite — `SESSION_TOKEN_TTL_SECONDS` (declared at the top of this file)
 * lifts it just enough to cover a suite wall-clock with a 2× margin.
 * Body, not URL: Clerk's Backend API takes mint params in the JSON body
 * for token endpoints.
 */
export async function mintTokens(
  userId: string,
): Promise<{ sessionId: string; sessionJwt: string }> {
  const session = await clerkRequest<ClerkSession>("POST", "/sessions", { user_id: userId });
  return { sessionId: session.id, sessionJwt: await refreshSessionToken(session.id) };
}

/**
 * Mint a fresh customized-session JWT on an *existing* session. The token
 * carries the same `aud` + `tenant_id` + `role` claims (Session Token
 * Customization applies per-session), so the api-client can recover from a
 * 401 when the cached Bearer outlives its TTL on a long suite run —
 * AUTH.md's documented "re-mint on 401" posture. Bare `/tokens` (no template
 * suffix); Clerk's Backend API takes mint params in the JSON body.
 */
export async function refreshSessionToken(sessionId: string): Promise<string> {
  const minted = await clerkRequest<ClerkSessionToken>(
    "POST",
    `/sessions/${sessionId}/tokens`,
    { expires_in_seconds: SESSION_TOKEN_TTL_SECONDS },
  );
  return minted.jwt;
}

/**
 * Revoke a previously-minted session. Used by globalTeardown to keep the
 * Clerk DEV session list bounded across suite runs. Tolerates "already
 * revoked" / "session not found" so a teardown after a partial setup
 * cannot mask a real test failure.
 */
export async function revokeSession(sessionId: string): Promise<void> {
  try {
    await clerkRequest<unknown>("POST", `/sessions/${sessionId}/revoke`);
  } catch (err) {
    if (err instanceof Error && /4\d\d/.test(err.message)) return;
    throw err;
  }
}

export interface ProvisionedUser {
  key: FixtureKey;
  email: string;
  password: string;
  clerkUserId: string;
}

/**
 * Phase 1: ensure the Clerk user exists. Returns identity only — JWT mint is
 * deferred until AFTER bootstrapTenant has updated publicMetadata, so the
 * minted JWT carries tenant_id and role claims.
 */
export async function provisionUser(spec: FixtureUserSpec): Promise<ProvisionedUser> {
  const user = await ensureUser(spec);
  return { key: spec.key, email: spec.email, password: spec.password, clerkUserId: user.id };
}

/**
 * Phase 3: mint the api-template session JWT after bootstrapTenant has
 * updated publicMetadata. The order matters — the template JWT snapshots
 * publicMetadata at mint time; minting before bootstrap produces a JWT
 * without tenant_id, which agentsfleetd rejects with UZ-AUTH-001 ("Tenant context
 * required").
 */
export async function attachJwt(user: ProvisionedUser): Promise<MintedFixture> {
  const tokens = await mintTokens(user.clerkUserId);
  return { ...user, ...tokens };
}

export async function findUserIdByEmail(email: string): Promise<string | null> {
  const user = await findUserByEmail(email);
  return user?.id ?? null;
}

export async function createSignInTicket(userId: string): Promise<string> {
  const result = await clerkRequest<ClerkSignInToken>("POST", "/sign_in_tokens", {
    user_id: userId,
    expires_in_seconds: 300,
  });
  return result.token;
}

export async function waitForTenantMetadata(userId: string): Promise<void> {
  const deadline = Date.now() + CLERK_METADATA_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const user = await getUser(userId);
    const hasTenantId = typeof user.public_metadata?.[TENANT_ID_METADATA_KEY] === "string";
    const scopes = scopeValues(user.public_metadata?.scopes);
    if (hasTenantId && scopes.includes(TENANT_SCOPE_SENTINEL)) return;
    await new Promise((resolve) => setTimeout(resolve, CLERK_METADATA_POLL_MS));
  }
  throw new Error(
    `Clerk user ${userId} missing tenant metadata or ${TENANT_SCOPE_SENTINEL}`,
  );
}

export async function deleteUser(userId: string): Promise<void> {
  await clerkRequest<{ deleted: boolean }>("DELETE", `/users/${userId}`);
}

export interface ClerkUserSummary {
  id: string;
  email: string;
  createdAtMs: number;
}

/** Users whose primary email matches `query` (Clerk substring search), newest first. */
export async function listUsersByQuery(query: string): Promise<ClerkUserSummary[]> {
  const users = await clerkRequest<
    Array<{ id: string; created_at: number; email_addresses: Array<{ email_address: string }> }>
  >("GET", `/users?query=${encodeURIComponent(query)}&limit=100&order_by=-created_at`);
  return users.map((u) => ({
    id: u.id,
    email: u.email_addresses[0]?.email_address ?? "",
    createdAtMs: u.created_at,
  }));
}
