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

const CLERK_API_BASE = "https://api.clerk.com/v1";

export interface FixtureUserSpec {
  key: "regular" | "admin";
  email: string;
  password: string;
}

export interface MintedFixture {
  key: FixtureUserSpec["key"];
  email: string;
  clerkUserId: string;
  sessionJwt: string;
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
  return clerkRequest<ClerkUser>("POST", "/users", {
    email_address: [spec.email],
    password: spec.password,
    skip_password_checks: true,
    skip_password_requirement: false,
  });
}

async function ensureUser(spec: FixtureUserSpec): Promise<ClerkUser> {
  const existing = await findUserByEmail(spec.email);
  if (existing) return existing;
  return createUser(spec);
}

async function mintSessionJwt(userId: string): Promise<string> {
  const session = await clerkRequest<ClerkSession>("POST", "/sessions", { user_id: userId });
  const token = await clerkRequest<ClerkSessionToken>(
    "POST",
    `/sessions/${session.id}/tokens`,
    {},
  );
  return token.jwt;
}

export async function provisionFixture(spec: FixtureUserSpec): Promise<MintedFixture> {
  const user = await ensureUser(spec);
  const jwt = await mintSessionJwt(user.id);
  return { key: spec.key, email: spec.email, clerkUserId: user.id, sessionJwt: jwt };
}
