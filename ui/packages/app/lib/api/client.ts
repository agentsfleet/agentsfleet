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

// Reads the ETag off a response. Typed with optional `headers` for the same
// reason as `retryAfterFrom` — header-less duck-typed responses (test doubles)
// read as "no ETag" rather than throwing.
function etagFrom(res: { headers?: Headers }): string | null {
  return res.headers ? res.headers.get("etag") : null;
}

export async function request<T>(
  path: string,
  init: RequestInit,
  token: string,
): Promise<T> {
  return (await requestWithEtag<T>(path, init, token)).data;
}

// Like `request`, but also surfaces the `ETag` response header. Used by the
// optimistic-concurrency surfaces (the fleet console's source editor, the
// catalog row editor): the caller holds the tag and sends it back as `If-Match`
// on the next write, so a concurrent edit is a 412 rather than a silent
// overwrite. `etag` is null when the endpoint sets no header.
export async function requestWithEtag<T>(
  path: string,
  init: RequestInit,
  token: string,
): Promise<{ data: T; etag: string | null }> {
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

  const etag = etagFrom(res);

  if (res.status === 204) return { data: undefined as T, etag };

  // Error bodies are RFC 7807 problem+json: `{ docs_uri, title, detail,
  // error_code, request_id, user_message?, etag? }` (see
  // src/agentsfleetd/http/handlers/common.zig errorResponse). `user_message`
  // (when present) is the curated dashboard-safe sentence for this code —
  // preferred over `detail`/`title`, which are written for the CLI/API
  // audience and often carry internal nouns a dashboard user can't act on.
  const body = await res.json().catch(() => ({ detail: res.statusText }));

  if (!res.ok) {
    const retryAfterMs = retryAfterFrom(res);
    throw new ApiError(
      body.user_message ?? body.detail ?? body.title ?? res.statusText,
      res.status,
      body.error_code ?? "UZ-UNKNOWN",
      body.request_id,
      retryAfterMs,
      // A 412 carries the resource's current etag in the body so the editor can
      // rebase without a second GET (REST guide §4).
      body.etag ?? etag,
    );
  }

  return { data: body as T, etag };
}
