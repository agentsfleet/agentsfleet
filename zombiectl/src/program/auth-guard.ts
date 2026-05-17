/**
 * Authentication guard — blocks unauthenticated access to protected commands.
 */

export interface AuthGuardCtx {
  token?: string | null;
  apiKey?: string | null;
}

export interface AuthGuardResult {
  ok: boolean;
}

export function requireAuth(ctx: AuthGuardCtx): AuthGuardResult {
  if (ctx.token || ctx.apiKey) {
    return { ok: true };
  }
  return { ok: false };
}

export const AUTH_FAIL_MESSAGE = "not authenticated — run `zombiectl login` first";
