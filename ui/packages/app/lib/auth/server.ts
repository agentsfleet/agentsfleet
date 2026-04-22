import { auth } from "@clerk/nextjs/server";

export async function getServerToken(): Promise<string | null> {
  const { getToken } = await auth();
  return getToken();
}

export async function getServerAuth(): Promise<{ token: string | null; userId: string | null }> {
  const { getToken, userId } = await auth();
  return { token: await getToken(), userId: userId ?? null };
}

// Returns the session claims' metadata object if present. Used by
// `resolveActiveWorkspace` to read the `workspace_id` hint. Shape is
// provider-specific; callers must narrow the fields they read.
export async function getServerSessionMetadata(): Promise<Record<string, unknown> | null> {
  const { sessionClaims } = await auth();
  return (sessionClaims?.metadata ?? null) as Record<string, unknown> | null;
}
