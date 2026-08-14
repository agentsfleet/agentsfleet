// JWT claim decoding helpers.
//
// The CLI never verifies signatures — that's the server's job; here we only
// read public claims to populate the analytics distinct-id. Every extractor
// returns null when input shape is wrong, so callers can't trap on malformed
// tokens.

import { isString } from "../lib/guards.ts";

// Subset of Clerk-style claims the CLI consumes. Index signature carries
// namespaced URL keys as `unknown`, forcing callers to typeof-check
// before use.
export interface JwtMetadata {
  readonly tenant_id?: string;
  readonly [key: string]: unknown;
}

export interface JwtClaims {
  readonly iss?: string;
  readonly aud?: string | string[];
  readonly sub?: string;
  readonly exp?: number;
  readonly iat?: number;
  readonly nbf?: number;
  readonly tenant_id?: string;
  readonly metadata?: JwtMetadata;
  readonly [key: string]: unknown;
}

export function decodeTokenPayload(token: unknown): JwtClaims | null {
  if (!token || !isString(token)) return null;
  const parts = token.split(".");
  if (parts.length < 2 || !parts[1]) return null;
  try {
    const base64 = parts[1].replace(/-/g, "+").replace(/_/g, "/");
    const padded = base64 + "===".slice((base64.length + 3) % 4);
    return JSON.parse(
      Buffer.from(padded, "base64").toString("utf8"),
    ) as JwtClaims;
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
