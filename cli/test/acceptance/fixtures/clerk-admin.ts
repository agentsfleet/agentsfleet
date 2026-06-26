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
  readonly password?: string | undefined;
  readonly role?: string | undefined;
}

export interface MintTokensOptions {
  readonly ttlSeconds?: number | undefined;
}

export interface AttachJwtOptions {
  readonly email: string;
  readonly password?: string | undefined;
  readonly ttlSeconds?: number | undefined;
}

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
): Promise<unknown> {
  const res = await fetch(`${CLERK_API_BASE}${pathSuffix}`, {
    method,
    headers: authHeaders(clerkSecret),
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    const detail = await res.text();
    throw new Error(`Clerk ${method} ${pathSuffix} → ${res.status}: ${detail}`);
  }
  return res.json();
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
  const result = await clerkRequest(clerkSecret, "POST", "/users", {
    email_address: [opts.email],
    password: opts.password,
    skip_password_checks: true,
    skip_password_requirement: false,
    public_metadata: {
      [IS_TEST_FIXTURE_METADATA_KEY]: true,
      owner: "acceptance-e2e-suite",
      role: opts.role ?? "regular",
    },
  });
  return result as ClerkUser;
}

export async function provisionUser(
  clerkSecret: string,
  opts: ProvisionUserOptions,
): Promise<ClerkUser> {
  const existing = await findUserByEmail(clerkSecret, opts.email);
  if (existing) return existing;
  if (!opts.password) {
    throw new Error(`fixture user ${opts.email} does not exist and no password supplied for create`);
  }
  return createUser(clerkSecret, opts);
}

export async function mintTokens(
  clerkSecret: string,
  clerkUserId: string,
  opts?: MintTokensOptions,
): Promise<MintedTokens> {
  const session = await clerkRequest(clerkSecret, "POST", "/sessions", { user_id: clerkUserId }) as ClerkSession;
  const ttl = opts?.ttlSeconds ?? SESSION_TOKEN_TTL_SECONDS;
  // Two tokens per session: the template-minted JWT goes to the backend as
  // Bearer auth (carried via the env credential slot, AGENTSFLEET_API_KEY),
  // and the default (no-template) JWT goes into the `__session` cookie so
  // clerkMiddleware accepts the dashboard request.
  // Parallel mint matches the dashboard suite's posture verbatim.
  const [template, standard] = await Promise.all([
    clerkRequest(clerkSecret, "POST", `/sessions/${session.id}/tokens/${JWT_TEMPLATE}`,
      { expires_in_seconds: ttl }) as Promise<ClerkToken>,
    clerkRequest(clerkSecret, "POST", `/sessions/${session.id}/tokens`,
      { expires_in_seconds: ttl }) as Promise<ClerkToken>,
  ]);
  return { sessionId: session.id, sessionJwt: template.jwt, cookieJwt: standard.jwt };
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

async function waitForTenantMetadata(clerkSecret: string, userId: string): Promise<void> {
  const deadline = Date.now() + CLERK_METADATA_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const user = await getUser(clerkSecret, userId);
    const meta = user.public_metadata as Record<string, unknown> | undefined;
    if (typeof meta?.[TENANT_ID_METADATA_KEY] === "string") return;
    await new Promise((resolve) => setTimeout(resolve, CLERK_METADATA_POLL_MS));
  }
  throw new Error(
    `Clerk user ${userId} missing public_metadata.${TENANT_ID_METADATA_KEY} after ` +
      `${CLERK_METADATA_TIMEOUT_MS}ms — tenant bootstrap metadata never propagated`,
  );
}

export async function attachJwt(clerkSecret: string, opts: AttachJwtOptions): Promise<AttachedJwt> {
  const user = await provisionUser(clerkSecret, { email: opts.email, password: opts.password });
  // Replay user.created (idempotent) so the tenant + default workspace exist,
  // THEN wait for Clerk to propagate the tenant_id metadata the backend writes
  // back, THEN mint — minting before tenant_id lands produces a JWT without it
  // that agentsfleetd rejects (UZ-AUTH-001).
  await ensureFixtureTenantBootstrapped({ clerkUserId: user.id, email: opts.email });
  await waitForTenantMetadata(clerkSecret, user.id);
  const tokens = await mintTokens(clerkSecret, user.id, { ttlSeconds: opts.ttlSeconds });
  return { ...tokens, clerkUserId: user.id, email: opts.email };
}

export async function revokeSession(clerkSecret: string, sessionId: string): Promise<void> {
  try {
    await clerkRequest(clerkSecret, "POST", `/sessions/${sessionId}/revoke`);
  } catch (err: unknown) {
    if (err instanceof Error && /4\d\d/.test(err.message)) return;
    throw err;
  }
}
