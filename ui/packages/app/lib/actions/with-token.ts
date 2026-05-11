import { getServerToken } from "@/lib/auth/server";
import { ApiError } from "@/lib/api/errors";

// Discriminated union every server action returns. Server Actions can't
// throw across the RSC boundary with custom fields intact (`.status`,
// `.code` from ApiError don't survive serialisation), so every consumer
// branches on `ok`.
export type ActionResult<T> =
  | { ok: true; data: T }
  | { ok: false; error: string; status?: number };

// Resolves the Bearer token server-side via clerkMiddleware (Token A in
// docs/AUTH.md), then mints the api-template Bearer for zombied. Wraps the
// API call in a try/catch and normalises ApiError → status field so callers
// can branch on 401/404/409 etc. without re-importing ApiError.
export async function withToken<T>(
  fn: (token: string) => Promise<T>,
): Promise<ActionResult<T>> {
  const token = await getServerToken();
  if (!token) return { ok: false, error: "Not authenticated", status: 401 };
  try {
    return { ok: true, data: await fn(token) };
  } catch (e) {
    if (e instanceof ApiError) return { ok: false, error: e.message, status: e.status };
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
}
