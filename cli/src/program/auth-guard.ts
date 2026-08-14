/**
 * Authentication guard — blocks unauthenticated access to protected commands,
 * and refuses a stored credential against a deployment that never minted it.
 */

export interface AuthGuardCtx {
  token?: string | null;
  apiKey?: string | null;
}

export interface AuthGuardResult {
  ok: boolean;
}

export interface DeploymentGuardCtx extends AuthGuardCtx {
  apiUrl: string;
  storedApiUrl?: string | null;
  // True when this invocation NAMED its target — `--api` or
  // `AGENTSFLEET_API_URL`. False means the target was inferred, which is the
  // only condition under which an unbound credential is dangerous.
  targetIsExplicit?: boolean;
}

export const UNBOUND_FAIL_MESSAGE =
  "credential is not tied to a deployment and none was given — pass `--api <url>`, set AGENTSFLEET_API_URL, or run `agentsfleet login` again";

export function requireAuth(ctx: AuthGuardCtx): AuthGuardResult {
  if (ctx.token || ctx.apiKey) {
    return { ok: true };
  }
  return { ok: false };
}

export const AUTH_FAIL_MESSAGE = "not authenticated — run `agentsfleet login` first";

/**
 * Refuses the one case where the target would be a guess: a stored credential
 * whose deployment is unknown, dialled at a target nobody named. The ladder
 * would fall past both to the production default and reach it in silence,
 * presenting a credential that deployment never issued.
 *
 * Every other combination is already safe by construction, which is why this
 * is one check and not two:
 *
 *   - target named (`--api` / `AGENTSFLEET_API_URL`) — the operator said where
 *     to go, so going there is not a surprise. If the credential does not
 *     belong to that deployment the server refuses it, and every 401 on this
 *     milestone names the API URL it was presented to.
 *   - target inferred, deployment stored — the ladder resolves TO the stored
 *     deployment, so the two cannot disagree; there is nothing to compare.
 *   - env `AGENTSFLEET_API_KEY` — outranks the stored credential at the wire,
 *     so the stored one is not the credential in play.
 *
 * Pure, so the caller owns the exit: nothing has been sent when this refuses.
 */
export function unboundTarget(ctx: DeploymentGuardCtx): string | null {
  if (ctx.apiKey) return null;
  if (!ctx.token) return null;
  if (ctx.targetIsExplicit) return null;
  if (ctx.storedApiUrl) return null;
  return UNBOUND_FAIL_MESSAGE;
}

// `login` mints a credential, so it cannot require one.
const AUTH_EXEMPT: ReadonlySet<string> = new Set(["login"]);

// Exempt from the deployment question only — both still require a credential.
// `logout` must reach the deployment that minted the credential in order to
// revoke it, and `doctor` is the diagnostic an operator runs precisely BECAUSE
// something is wrong with their target; refusing it would withhold the tool
// that explains the refusal.
const DEPLOYMENT_EXEMPT: ReadonlySet<string> = new Set(["logout", "doctor"]);

export interface GuardRefusal {
  errorCode: string;
  commanderCode: string;
  message: string;
}

/**
 * The whole pre-action policy in one place: may this command run with the
 * credential and target the invocation resolved? Returns the refusal to emit,
 * or null to proceed. Pure — the caller owns writing and exiting.
 */
export function guardCommand(
  rootName: string,
  ctx: DeploymentGuardCtx,
): GuardRefusal | null {
  if (AUTH_EXEMPT.has(rootName)) return null;

  if (!requireAuth(ctx).ok) {
    return {
      errorCode: "AUTH_REQUIRED",
      commanderCode: "auth.required",
      message: AUTH_FAIL_MESSAGE,
    };
  }

  if (DEPLOYMENT_EXEMPT.has(rootName)) return null;

  const unbound = unboundTarget(ctx);
  return unbound
    ? {
        errorCode: "DEPLOYMENT_UNKNOWN",
        commanderCode: "deployment.unknown",
        message: unbound,
      }
    : null;
}
