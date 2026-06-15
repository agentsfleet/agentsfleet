/**
 * Minimal TS twin of `ui/packages/app/tests/e2e/acceptance/fixtures/clerk-admin.ts`.
 *
 * Three-phase chain: `provisionUser` → (dashboard suite's bootstrap path
 * already ran for the shared `regular` fixture) → `attachJwt` → `mintTokens`.
 * The CLI suite re-uses the same Clerk identity the dashboard suite uses,
 * so only the JWT mint surface is needed here. Webhook user.created bootstrap
 * is implicit because the dashboard's globalSetup already ran it for the
 * shared fixture before this suite ever fires.
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
  // Bearer auth (AGENTSFLEET_TOKEN), and the default (no-template) JWT goes into
  // the `__session` cookie so clerkMiddleware accepts the dashboard request.
  // Parallel mint matches the dashboard suite's posture verbatim.
  const [template, standard] = await Promise.all([
    clerkRequest(clerkSecret, "POST", `/sessions/${session.id}/tokens/${JWT_TEMPLATE}`,
      { expires_in_seconds: ttl }) as Promise<ClerkToken>,
    clerkRequest(clerkSecret, "POST", `/sessions/${session.id}/tokens`,
      { expires_in_seconds: ttl }) as Promise<ClerkToken>,
  ]);
  return { sessionId: session.id, sessionJwt: template.jwt, cookieJwt: standard.jwt };
}

export async function attachJwt(clerkSecret: string, opts: AttachJwtOptions): Promise<AttachedJwt> {
  const user = await provisionUser(clerkSecret, { email: opts.email, password: opts.password });
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
