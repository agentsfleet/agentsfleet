import { auth } from "@clerk/nextjs/server";

/**
 * Reads the `platform_admin` claim from the Clerk session metadata.
 *
 * `platform_admin` is a boolean claim set by a manual Clerk `publicMetadata`
 * flip on agentsfleet's operator user (docs/AUTH.md). It gates the runner
 * operator plane — enrollment (`POST /v1/runners`) and the fleet read
 * (`GET /v1/fleets/runners`). This is the frontend's defence-in-depth check:
 * the dashboard hides the surface for non-admins, and the backend independently
 * rejects a non-admin principal `403 UZ-AUTH-021` regardless.
 *
 * Returns `false` when the auth provider isn't available, the session is
 * anonymous, or the claim is absent (fail-closed) — every caller treats a
 * `false` as "not a platform admin" and redirects.
 */
export async function readPlatformAdminClaim(): Promise<boolean> {
  try {
    const { sessionClaims } = await auth();
    const metadata = (sessionClaims?.metadata ?? null) as Record<string, unknown> | null;
    return metadata !== null && metadata.platform_admin === true;
  } catch {
    return false;
  }
}
