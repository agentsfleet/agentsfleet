import { ApiError } from "./errors";

// Full backend origin — used for display URLs (webhooks) and server-side fetches.
export const API_ORIGIN = process.env.NEXT_PUBLIC_API_URL ?? "https://api-dev.usezombie.com";

// BASE for fetch calls. On the server we hit the backend directly (no CORS).
// In the browser we go through the same-origin `/backend` proxy configured in
// next.config.ts `rewrites` — browser never sees a cross-origin request.
export const BASE = typeof window === "undefined" ? API_ORIGIN : "/backend";

/**
 * Parses a `Retry-After` header value into milliseconds. Honors the
 * delta-seconds form (e.g., `Retry-After: 30`); the HTTP-date form is
 * rare for our APIs and is ignored (callers fall back to exponential
 * backoff). Mirrors the CLI parser at `zombiectl/src/lib/http.js`.
 */
export function parseRetryAfterHeaderValue(headerVal: string | null): number | null {
  if (!headerVal) return null;
  const n = Number(headerVal);
  if (Number.isFinite(n) && n >= 0) return n * 1000;
  return null;
}

export async function request<T>(
  path: string,
  init: RequestInit,
  token: string,
): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
      ...init.headers,
    },
  });

  if (res.status === 204) return undefined as T;

  const body = await res.json().catch(() => ({ error: res.statusText }));

  if (!res.ok) {
    // Defensive access — test doubles and rare runtimes may omit
    // `headers`. The retry layer treats `null` as "fall back to
    // exponential backoff" rather than failing closed.
    const retryAfterMs = parseRetryAfterHeaderValue(
      typeof res.headers?.get === "function" ? res.headers.get("retry-after") : null,
    );
    throw new ApiError(
      body.error ?? res.statusText,
      res.status,
      body.code ?? "UZ-UNKNOWN",
      body.request_id,
      retryAfterMs,
    );
  }

  return body as T;
}
