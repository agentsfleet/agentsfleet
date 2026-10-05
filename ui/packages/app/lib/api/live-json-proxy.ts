// What every same-origin JSON read under app/live answers with: the backfill
// pages, one event's saved row, and one tool call in full. The browser holds
// no bearer, so each route names its upstream path and this mints the token,
// guards the path and passes errors through, once for all of them. The two
// stream routes use event-stream-proxy.ts instead, because an EventSource
// needs its own headers and refusal frame.
//
// See docs/AUTH.md "SSE stream — Next Route Handler injects Bearer" for the
// auth sequence.

import { credential } from "@/lib/auth/credential";
import { API_ORIGIN } from "@/lib/api/client";
import { ERROR_CODE } from "@/lib/errors";

const CONTENT_TYPE_JSON = "application/json";
const CONTENT_TYPE_TEXT = "text/plain";
const UPSTREAM_PATH_PREFIX = "/v1";

// Authed per-tenant JSON must never land in a shared cache: the URL varies by
// workspace, fleet and event, not by user.
const CACHE_CONTROL_NO_STORE = "no-store";

// encodeURIComponent leaves '.' intact, so a bare '..'/'.' path param would
// dot-normalize inside fetch and steer the minted token at an upstream path
// other than the one the route exists to reach.
const DOT_ONLY_SEGMENT = /^\.+$/;

const HTTP_STATUS = {
  OK: 200,
  BAD_REQUEST: 400,
  UNAUTHORIZED: 401,
  BAD_GATEWAY: 502,
} as const;

/**
 * GET `/v1/<segments>` upstream as the signed-in user and answer with its JSON.
 *
 * Segments arrive raw and each is percent-encoded here; a dot-only one is
 * refused before a token is minted. Only `forwardedKeys` of the request's
 * query reach upstream, so a route never widens the surface it proxies. The
 * body streams through: a call's full output and one event's bodies are the
 * largest things these routes carry, and none has a reason to hold them.
 */
export async function proxyJsonGet(
  req: Request,
  segments: readonly string[],
  forwardedKeys: readonly string[] = [],
): Promise<Response> {
  if (segments.some((segment) => DOT_ONLY_SEGMENT.test(segment))) {
    return jsonResponse(HTTP_STATUS.BAD_REQUEST, JSON.stringify({ error: "Invalid path parameter" }));
  }

  const token = await credential();
  if (!token) return jsonResponse(HTTP_STATUS.UNAUTHORIZED, JSON.stringify({ error: "Unauthorized", code: ERROR_CODE.AUTH_401 }));

  let upstream: Response;
  try {
    upstream = await fetch(upstreamUrl(req, segments, forwardedKeys), {
      method: "GET",
      headers: {
        Authorization: `Bearer ${token}`,
        Accept: CONTENT_TYPE_JSON,
      },
      signal: req.signal,
    });
  } catch {
    // Backend unreachable (or the browser aborted mid-flight): a pinned 502
    // envelope, not an unhandled framework 500.
    return jsonResponse(HTTP_STATUS.BAD_GATEWAY, JSON.stringify({ error: "Upstream unreachable" }));
  }

  if (!upstream.ok) return upstreamError(upstream);
  return jsonResponse(HTTP_STATUS.OK, upstream.body);
}

function upstreamUrl(req: Request, segments: readonly string[], forwardedKeys: readonly string[]): string {
  const path = segments.map((segment) => `/${encodeURIComponent(segment)}`).join("");
  const incoming = new URL(req.url).searchParams;
  const forwarded = new URLSearchParams();
  for (const key of forwardedKeys) {
    const value = incoming.get(key);
    if (value !== null) forwarded.set(key, value);
  }
  const query = forwarded.toString();
  return `${API_ORIGIN}${UPSTREAM_PATH_PREFIX}${path}${query.length > 0 ? `?${query}` : ""}`;
}

function jsonResponse(status: number, body: BodyInit | null): Response {
  return new Response(body, { status, headers: noStoreHeaders(CONTENT_TYPE_JSON) });
}

function noStoreHeaders(contentType: string): HeadersInit {
  return { "Content-Type": contentType, "Cache-Control": CACHE_CONTROL_NO_STORE };
}

// The upstream's own status and body. These GETs are directly navigable on the
// dashboard origin, so an upstream error is never reflected under a
// markup-capable content type.
async function upstreamError(upstream: Response): Promise<Response> {
  const text = await upstream.text().catch(() => "");
  const upstreamType = upstream.headers.get("content-type") ?? CONTENT_TYPE_TEXT;
  return new Response(text || `Upstream error ${upstream.status}`, {
    status: upstream.status,
    headers: noStoreHeaders(upstreamType.startsWith(CONTENT_TYPE_JSON) ? upstreamType : CONTENT_TYPE_TEXT),
  });
}
