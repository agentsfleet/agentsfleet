// JWT claim decoding + TTY-aware env/file token resolution.
//
// Claim decoding: the CLI never verifies signatures — that's the server's
// job; here we only read public claims to populate analytics distinct-id,
// role-gate UI hints, and the auth-status summary. Every extractor
// returns null when input shape is wrong, so callers can't trap on
// malformed tokens.
//
// Resolution: whether `credentials.json` or `ZOMBIE_TOKEN` wins is
// TTY-dependent. Interactive shells prefer the env-var the operator just
// exported in the current session over a possibly-stale file; scripts
// (CI, cron, pipes) prefer the on-disk credential a previous `zombiectl
// login` wrote, falling through to env only if no file exists.

export type RoleClaim = "user" | "operator" | "admin";

// Subset of Clerk-style claims the CLI consumes. Index signature carries
// namespaced URL keys (`https://usezombie.dev/role` etc.) as `unknown`,
// forcing callers to typeof-check before use.
export interface JwtMetadata {
  readonly tenant_id?: string;
  readonly role?: string;
  readonly [key: string]: unknown;
}

export interface JwtRoleClaim {
  readonly role?: string;
  readonly [key: string]: unknown;
}

export interface JwtClaims {
  readonly iss?: string;
  readonly aud?: string | string[];
  readonly sub?: string;
  readonly exp?: number;
  readonly iat?: number;
  readonly nbf?: number;
  readonly role?: string;
  readonly tenant_id?: string;
  readonly metadata?: JwtMetadata;
  readonly custom_claims?: JwtRoleClaim;
  readonly app_metadata?: JwtRoleClaim;
  readonly [key: string]: unknown;
}

const ROLE_NAMESPACE_DEV = "https://usezombie.dev/role";
const ROLE_NAMESPACE_COM = "https://usezombie.com/role";

export type AuthTokenSource = "file" | "zombie_env" | "none";

export interface ResolvedAuthToken {
  readonly token: string | null;
  readonly source: AuthTokenSource;
}

export interface ResolveAuthTokenInput {
  readonly fileToken: string | null;
  readonly env: NodeJS.ProcessEnv;
  readonly isTty: boolean;
}

const trimOrNull = (raw: string | undefined | null): string | null => {
  if (typeof raw !== "string") return null;
  const trimmed = raw.trim();
  return trimmed.length > 0 ? trimmed : null;
};

// Pure resolver — no process.env or process.stdin reads. Callers pass
// the snapshot in so tests can pin TTY-ness and env shape without monkey-
// patching process. The order is intentional: env-var values are
// inspected before file, but only "win" in TTY mode; non-TTY callers
// fall through to file first.
export function resolveAuthTokenForCli(input: ResolveAuthTokenInput): ResolvedAuthToken {
  const zombie = trimOrNull(input.env["ZOMBIE_TOKEN"]);
  const file = trimOrNull(input.fileToken);
  const fileResolved: ResolvedAuthToken | null = file ? { token: file, source: "file" } : null;
  const zombieResolved: ResolvedAuthToken | null = zombie
    ? { token: zombie, source: "zombie_env" }
    : null;
  const order: ReadonlyArray<ResolvedAuthToken | null> = input.isTty
    ? [zombieResolved, fileResolved]
    : [fileResolved, zombieResolved];
  for (const candidate of order) if (candidate) return candidate;
  return { token: null, source: "none" };
}

export function decodeTokenPayload(token: unknown): JwtClaims | null {
  if (!token || typeof token !== "string") return null;
  const parts = token.split(".");
  if (parts.length < 2 || !parts[1]) return null;
  try {
    const base64 = parts[1].replace(/-/g, "+").replace(/_/g, "/");
    const padded = base64 + "===".slice((base64.length + 3) % 4);
    return JSON.parse(Buffer.from(padded, "base64").toString("utf8")) as JwtClaims;
  } catch {
    return null;
  }
}

export function extractDistinctIdFromToken(token: unknown): string | null {
  const payload = decodeTokenPayload(token);
  if (payload && typeof payload.sub === "string" && payload.sub.trim().length > 0) {
    return payload.sub.trim();
  }
  return null;
}

export function extractRoleFromToken(token: unknown): RoleClaim | null {
  const payload = decodeTokenPayload(token);
  if (!payload) return null;

  const candidates: ReadonlyArray<unknown> = [
    payload.role,
    payload.metadata?.role,
    payload.custom_claims?.role,
    payload.app_metadata?.role,
    payload[ROLE_NAMESPACE_DEV],
    payload[ROLE_NAMESPACE_COM],
    payload.metadata?.[ROLE_NAMESPACE_DEV],
    payload.metadata?.[ROLE_NAMESPACE_COM],
  ];
  for (const raw of candidates) {
    if (typeof raw !== "string") continue;
    const value = raw.toLowerCase();
    if (value === "user" || value === "operator" || value === "admin") return value;
  }
  return null;
}
