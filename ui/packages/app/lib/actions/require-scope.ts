import { hasScope } from "@/lib/auth/platform";
import { ERROR_CODE } from "@/lib/errors";
import type { ActionResult } from "@/lib/actions/with-token";

/**
 * Defence-in-depth: gate a server action on the specific operator scope its
 * backend route enforces (`route_scopes.zig`) before the round-trip. The
 * backend independently 403s a token missing the scope (`UZ-AUTH-022`) — this
 * just fails fast so the UI never round-trips a request the token can't
 * satisfy. Shared by the runners and admin-models operator actions so the gate
 * shape (error copy, status, error code) stays defined in exactly one place.
 */
export async function requireScope<T>(
  scope: string,
  fn: () => Promise<ActionResult<T>>,
): Promise<ActionResult<T>> {
  if (!(await hasScope(scope))) {
    return {
      ok: false,
      error: `Operator scope required: ${scope}`,
      status: 403,
      errorCode: ERROR_CODE.INSUFFICIENT_SCOPE,
    };
  }
  return fn();
}
