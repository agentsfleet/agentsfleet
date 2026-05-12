/**
 * Minimal JS twin of `ui/packages/app/tests/e2e/acceptance/fixtures/clerk-admin.ts`.
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
} from "./constants.js";

function authHeaders(clerkSecret) {
  if (!clerkSecret) throw new Error("clerkSecret missing — pass CLERK_SECRET_KEY explicitly");
  return {
    Authorization: `Bearer ${clerkSecret}`,
    "Content-Type": "application/json",
  };
}

async function clerkRequest(clerkSecret, method, pathSuffix, body) {
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

async function findUserByEmail(clerkSecret, email) {
  const params = new URLSearchParams({ email_address: email });
  const list = await clerkRequest(clerkSecret, "GET", `/users?${params.toString()}`);
  return Array.isArray(list) && list.length > 0 ? list[0] : null;
}

async function createUser(clerkSecret, opts) {
  return clerkRequest(clerkSecret, "POST", "/users", {
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
}

export async function provisionUser(clerkSecret, opts) {
  const existing = await findUserByEmail(clerkSecret, opts.email);
  if (existing) return existing;
  if (!opts.password) {
    throw new Error(`fixture user ${opts.email} does not exist and no password supplied for create`);
  }
  return createUser(clerkSecret, opts);
}

export async function mintTokens(clerkSecret, clerkUserId, opts) {
  const session = await clerkRequest(clerkSecret, "POST", "/sessions", { user_id: clerkUserId });
  const ttl = opts?.ttlSeconds ?? SESSION_TOKEN_TTL_SECONDS;
  // Two tokens per session: the template-minted JWT goes to the backend as
  // Bearer auth (ZOMBIE_TOKEN), and the default (no-template) JWT goes into
  // the `__session` cookie so clerkMiddleware accepts the dashboard request.
  // Parallel mint matches the dashboard suite's posture verbatim.
  const [template, standard] = await Promise.all([
    clerkRequest(clerkSecret, "POST", `/sessions/${session.id}/tokens/${JWT_TEMPLATE}`,
      { expires_in_seconds: ttl }),
    clerkRequest(clerkSecret, "POST", `/sessions/${session.id}/tokens`,
      { expires_in_seconds: ttl }),
  ]);
  return { sessionId: session.id, sessionJwt: template.jwt, cookieJwt: standard.jwt };
}

export async function attachJwt(clerkSecret, opts) {
  const user = await provisionUser(clerkSecret, { email: opts.email, password: opts.password });
  const tokens = await mintTokens(clerkSecret, user.id, { ttlSeconds: opts.ttlSeconds });
  return { ...tokens, clerkUserId: user.id, email: opts.email };
}

export async function revokeSession(clerkSecret, sessionId) {
  try {
    await clerkRequest(clerkSecret, "POST", `/sessions/${sessionId}/revoke`);
  } catch (err) {
    if (err instanceof Error && /4\d\d/.test(err.message)) return;
    throw err;
  }
}
