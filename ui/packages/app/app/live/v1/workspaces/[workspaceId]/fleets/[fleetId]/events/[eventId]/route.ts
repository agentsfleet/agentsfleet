// Same-origin read of one event's saved row. The chat reads it when a reply's
// ending never reached the stream. A Route Handler, not a Server Action: Next
// runs a tab's Server Actions one at a time, so the read would queue behind a
// send in flight and the next send behind the read. Mirrors the backfill proxy
// at ../route.ts (Clerk session → API-audience JWT → upstream GET, 401 and
// upstream-error handling), differing in the upstream path and in streaming
// the body through: one event's bodies are the largest thing this proxy
// carries, and it has no reason to hold them.
//
// The static `stream` sibling wins over this dynamic segment, so the stream
// proxy is unaffected; event ids are `<millis>-<seq>` and never collide.
//
// See docs/AUTH.md "SSE stream — Next Route Handler injects Bearer" for the auth sequence.

import { credential } from "@/lib/auth/credential";
import { API_ORIGIN } from "@/lib/api/client";
import { ERROR_CODE } from "@/lib/errors";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = {
  params: Promise<{ workspaceId: string; fleetId: string; eventId: string }>;
};

const CONTENT_TYPE_JSON = "application/json";
const CONTENT_TYPE_TEXT = "text/plain";

// Authed per-tenant JSON must never land in a shared cache — the URL varies
// by workspace, fleet and event, not by user.
const CACHE_CONTROL_NO_STORE = "no-store";

// encodeURIComponent leaves '.' intact, so a bare '..'/'.' path param would
// dot-normalize inside fetch and steer the minted token at an upstream path
// other than the one this proxy exists to reach.
const DOT_ONLY_SEGMENT = /^\.+$/;

const HTTP_STATUS = {
  OK: 200,
  BAD_REQUEST: 400,
  UNAUTHORIZED: 401,
  BAD_GATEWAY: 502,
} as const;

export async function GET(req: Request, { params }: Params) {
  const { workspaceId, fleetId, eventId } = await params;
  if ([workspaceId, fleetId, eventId].some((segment) => DOT_ONLY_SEGMENT.test(segment))) {
    return jsonResponse(HTTP_STATUS.BAD_REQUEST, JSON.stringify({ error: "Invalid path parameter" }));
  }

  const token = await credential();
  if (!token) return jsonResponse(HTTP_STATUS.UNAUTHORIZED, JSON.stringify({ error: "Unauthorized", code: ERROR_CODE.AUTH_401 }));

  const upstreamUrl =
    `${API_ORIGIN}/v1/workspaces/${encodeURIComponent(workspaceId)}` +
    `/fleets/${encodeURIComponent(fleetId)}/events/${encodeURIComponent(eventId)}`;

  let upstream: Response;
  try {
    upstream = await fetch(upstreamUrl, {
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

function jsonResponse(status: number, body: BodyInit | null): Response {
  return new Response(body, { status, headers: noStoreHeaders(CONTENT_TYPE_JSON) });
}

function noStoreHeaders(contentType: string): HeadersInit {
  return { "Content-Type": contentType, "Cache-Control": CACHE_CONTROL_NO_STORE };
}

// The upstream's own status and body. This GET is directly navigable on the
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
