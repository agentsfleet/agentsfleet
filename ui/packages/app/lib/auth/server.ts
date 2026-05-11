import { auth } from "@clerk/nextjs/server";

// Token B (the `api`-template JWT) is what zombied accepts as Bearer — it
// carries `metadata.tenant_id`/`metadata.role` from publicMetadata and the
// `aud=https://api.usezombie.com` claim the OIDC verifier strict-checks
// (see docs/AUTH.md "The two tokens at a glance"). Bare `getToken()`
// returns the default session token (Token A), which lacks both — fine
// for cookie validation by clerkMiddleware, wrong for cross-origin API
// calls. Every consumer of getServerToken/getServerAuth in the dashboard
// uses the result as Bearer to zombied, so the api template is correct
// for all of them.
const API_TEMPLATE = "api" as const;

export async function getServerToken(): Promise<string | null> {
  const { getToken } = await auth();
  return getToken({ template: API_TEMPLATE });
}

export async function getServerAuth(): Promise<{ token: string | null; userId: string | null }> {
  const { getToken, userId } = await auth();
  return { token: await getToken({ template: API_TEMPLATE }), userId: userId ?? null };
}

// Returns the session claims' metadata object if present. Used by
// `resolveActiveWorkspace` to read the `workspace_id` hint. Shape is
// provider-specific; callers must narrow the fields they read.
export async function getServerSessionMetadata(): Promise<Record<string, unknown> | null> {
  const { sessionClaims } = await auth();
  return (sessionClaims?.metadata ?? null) as Record<string, unknown> | null;
}
