// JWT claim decoding helpers.
//
// The CLI never verifies signatures — that's the server's job; here we only
// read public claims to populate analytics distinct-id, role-gate UI hints,
// and the auth-status summary. Every extractor returns null when input shape
// is wrong, so callers can't trap on malformed tokens.

const ADMIN = "admin" as const;
const OPERATOR = "operator" as const;
const TYPE_STRING = "string" as const;
const USER = "user" as const;

const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

export type RoleClaim = typeof USER | typeof OPERATOR | typeof ADMIN;

// Subset of Clerk-style claims the CLI consumes. Index signature carries
// namespaced URL keys (`https://agentsfleet.net/role` etc.) as `unknown`,
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

const ROLE_NAMESPACE = "https://agentsfleet.net/role";

export function decodeTokenPayload(token: unknown): JwtClaims | null {
  if (!token || !isString(token)) return null;
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
  if (payload && isString(payload.sub) && payload.sub.trim().length > 0) {
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
    payload[ROLE_NAMESPACE],
    payload.metadata?.[ROLE_NAMESPACE],
  ];
  for (const raw of candidates) {
    if (!isString(raw)) continue;
    const value = raw.toLowerCase();
    if (value === USER || value === OPERATOR || value === ADMIN) return value;
  }
  return null;
}
