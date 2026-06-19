import { ApiError } from "./errors";
import { recordWorkspaceFetchForAcceptance } from "../acceptance/workspace-fetch-audit";

// Full backend origin — used for display URLs (webhooks) and server-side fetches.
export const API_ORIGIN = process.env.NEXT_PUBLIC_API_URL ?? "https://api-dev.agentsfleet.net";

// BASE for fetch calls. On the server we hit the backend directly (no CORS).
// In the browser we go through the same-origin `/backend` proxy configured in
// next.config.ts `rewrites` — browser never sees a cross-origin request.
export const BASE = typeof window === "undefined" ? API_ORIGIN : "/backend";

/**
 * Parses a `Retry-After` header value into milliseconds. Honors the
 * delta-seconds form (e.g., `Retry-After: 30`); the HTTP-date form is
 * rare for our APIs and is ignored (callers fall back to exponential
 * backoff). Mirrors the CLI parser at `cli/src/lib/http.js`.
 */
const MS_PER_SECOND = 1000;

export function parseRetryAfterHeaderValue(headerVal: string | null): number | null {
  if (!headerVal) return null;
  const n = Number(headerVal);
  if (Number.isFinite(n) && n >= 0) return n * MS_PER_SECOND;
  return null;
}

// Reads Retry-After off a response. Typed to need only an optional `Headers`
// so it tolerates header-less duck-typed responses (test doubles, exotic
// runtimes); a missing Headers reads as "no Retry-After" and the retry layer
// falls back to exponential backoff rather than throwing.
function retryAfterFrom(res: { headers?: Headers }): number | null {
  return res.headers ? parseRetryAfterHeaderValue(res.headers.get("retry-after")) : null;
}

export async function request<T>(
  path: string,
  init: RequestInit,
  token: string,
): Promise<T> {
  if ((init.method ?? "GET").toUpperCase() === "GET") {
    recordWorkspaceFetchForAcceptance(path);
  }

  const res = await fetch(`${BASE}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
      ...init.headers,
    },
  });

  if (res.status === 204) return undefined as T;

  // Error bodies are RFC 7807 problem+json: `{ docs_uri, title, detail,
  // error_code, request_id }` (see src/http/handlers/common.zig errorResponse).
  // The human-facing message is `detail` (instance-specific), falling back to
  // `title` (the short label) then the HTTP reason phrase.
  const body = await res.json().catch(() => ({ detail: res.statusText }));

  if (!res.ok) {
    const retryAfterMs = retryAfterFrom(res);
    throw new ApiError(
      body.detail ?? body.title ?? res.statusText,
      res.status,
      body.error_code ?? "UZ-UNKNOWN",
      body.request_id,
      retryAfterMs,
    );
  }

  return body as T;
}
