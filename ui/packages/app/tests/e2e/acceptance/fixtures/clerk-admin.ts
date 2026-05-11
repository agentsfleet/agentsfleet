/**
 * Clerk backend-API helpers for the e2e harness.
 *
 * Idempotent fixture-user provisioning + session JWT mint via Clerk's
 * REST API. Same wire shape as `playbooks/012_usezombie_admin_bootstrap`
 * (lines 119–125): GET /v1/users → POST /v1/sessions → POST /v1/sessions/{id}/tokens.
 *
 * Uses fetch directly. No @clerk/backend SDK — the surface is small and
 * stable, and the SDK pulls in node:crypto-heavy deps we don't need.
 */

import { FixtureKey, JWT_TEMPLATE } from "./constants";

const CLERK_API_BASE = "https://api.clerk.com/v1";

// Session-token TTL for minted fixture JWTs. Default Clerk TTL is 60s — too
// short for a full suite run. The harness uses 15 min, which is ~2× the
// observed p95 wall-clock on CI (suites complete in ~5 min). Tighter than
// the historical 3600s posture to bound the impact of a leaked
// .fixture-jwts.json file. `clientFor` callers that exceed this window
// will fail loud with a 401 from zombied — re-mint if/when that happens.
const SESSION_TOKEN_TTL_SECONDS = 900;

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
  /** `api`-template JWT — Bearer auth on zombied; carries publicMetadata. */
  sessionJwt: string;
  /** Default (non-template) session JWT — `__session` cookie value;
   *  carries `sid`/`azp` claims that `clerkMiddleware()` requires. */
  cookieJwt: string;
}

interface ClerkUser {
  id: string;
  email_addresses: Array<{ email_address: string }>;
}

interface ClerkSession {
  id: string;
}

interface ClerkSessionToken {
  jwt: string;
}

function authHeaders(): Record<string, string> {
  const secret = process.env.CLERK_SECRET_KEY;
  if (!secret) throw new Error("CLERK_SECRET_KEY missing");
  return {
    Authorization: `Bearer ${secret}`,
    "Content-Type": "application/json",
  };
}

async function clerkRequest<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${CLERK_API_BASE}${path}`, {
    method,
    headers: authHeaders(),
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    const detail = await res.text();
    throw new Error(`Clerk ${method} ${path} → ${res.status}: ${detail}`);
  }
  return (await res.json()) as T;
}

async function findUserByEmail(email: string): Promise<ClerkUser | null> {
  const params = new URLSearchParams({ email_address: email });
  const list = await clerkRequest<ClerkUser[]>("GET", `/users?${params.toString()}`);
  return list[0] ?? null;
}

async function createUser(spec: FixtureUserSpec): Promise<ClerkUser> {
  // public_metadata.is_test_fixture lets prod ops dashboards filter these
  // identities out, and gives a future Clerk webhook handler a hook to
  // refuse unsafe operations against fixture users.
  return clerkRequest<ClerkUser>("POST", "/users", {
    email_address: [spec.email],
    password: spec.password,
    skip_password_checks: true,
    skip_password_requirement: false,
    public_metadata: {
      is_test_fixture: true,
      owner: "acceptance-e2e-suite",
      role: spec.key,
    },
  });
}

async function ensureUser(spec: FixtureUserSpec): Promise<ClerkUser> {
  const existing = await findUserByEmail(spec.email);
  if (existing) {
    // Backfill the metadata tag on pre-existing fixture users (one-time
    // migration cost when rolling out the tag for the first time).
    await clerkRequest<ClerkUser>("PATCH", `/users/${existing.id}/metadata`, {
      public_metadata: {
        is_test_fixture: true,
        owner: "acceptance-e2e-suite",
        role: spec.key,
      },
    }).catch(() => undefined);
    return existing;
  }
  return createUser(spec);
}

/**
 * Mints two session JWTs for the same Clerk session:
 *
 *   1. **Template (`api`) JWT** — carries `metadata.tenant_id` + `metadata.role`
 *      from publicMetadata; used as Bearer auth on zombied. Default session
 *      tokens omit publicMetadata, which lands at 403 UZ-AUTH-001 ("Tenant
 *      context required").
 *
 *   2. **Default JWT** — Clerk's standard session token (no template); carries
 *      the `sid` and `azp` claims `clerkMiddleware()` needs to validate the
 *      `__session` cookie. Template tokens omit `sid`, so they cannot be
 *      mounted as a cookie — the dashboard middleware redirects to /sign-in.
 *
 * Both share the same session, so they expire together. Default Clerk session-
 * token TTL is 60s, far shorter than a full e2e suite — `SESSION_TOKEN_TTL_SECONDS`
 * (declared at the top of this file) lifts it just enough to cover a suite
 * wall-clock with a 2× margin. Body, not URL: Clerk's Backend API takes mint
 * params in the JSON body for token endpoints.
 */
export async function mintTokens(
  userId: string,
): Promise<{ sessionId: string; sessionJwt: string; cookieJwt: string }> {
  const session = await clerkRequest<ClerkSession>("POST", "/sessions", { user_id: userId });
  const [template, standard] = await Promise.all([
    clerkRequest<ClerkSessionToken>(
      "POST",
      `/sessions/${session.id}/tokens/${JWT_TEMPLATE}`,
      { expires_in_seconds: SESSION_TOKEN_TTL_SECONDS },
    ),
    clerkRequest<ClerkSessionToken>(
      "POST",
      `/sessions/${session.id}/tokens`,
      { expires_in_seconds: SESSION_TOKEN_TTL_SECONDS },
    ),
  ]);
  return { sessionId: session.id, sessionJwt: template.jwt, cookieJwt: standard.jwt };
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
 * Phase 3: mint the session + cookie JWTs after bootstrapTenant has updated
 * publicMetadata. The order matters — the template JWT snapshots
 * publicMetadata at mint time; minting before bootstrap produces a JWT
 * without tenant_id, which zombied rejects with UZ-AUTH-001 ("Tenant context
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

export async function deleteUser(userId: string): Promise<void> {
  await clerkRequest<{ deleted: boolean }>("DELETE", `/users/${userId}`);
}
