/**
 * Authentication guard — blocks unauthenticated access to protected commands.
 */

export function requireAuth(ctx) {
  if (ctx.token || ctx.apiKey) {
    return { ok: true };
  }
  return { ok: false };
}

export const AUTH_FAIL_MESSAGE = "not authenticated \u2014 run `zombiectl login` first";
