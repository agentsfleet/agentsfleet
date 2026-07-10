// Same-origin backfill proxy. The browser holds no bearer token, so the
// stream registry's reconnect gap-recovery fetch hits this Route Handler
// instead of the upstream events list directly. Mirrors the SSE proxy at
// ./stream/route.ts (Clerk session → API-audience JWT → upstream GET,
// 401/upstream-error handling), differing only in the upstream path (the
// bounded list, not the stream) and the buffered JSON body.
//
// See docs/AUTH.md "UI · SSE stream" for the auth sequence.

import { auth } from "@clerk/nextjs/server";
import { API_ORIGIN } from "@/lib/api/client";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = {
  params: Promise<{ workspaceId: string; fleetId: string }>;
};

// Query keys forwarded upstream verbatim; anything else the caller sends is
// dropped so the proxy never widens the upstream surface.
const FORWARDED_QUERY_KEYS = ["cursor", "since", "limit"] as const;

const CONTENT_TYPE_JSON = "application/json";
const CONTENT_TYPE_TEXT = "text/plain";

// Authed per-tenant JSON must never land in a shared cache — the URL varies
// by workspace/fleet, not by user.
const CACHE_CONTROL_NO_STORE = "no-store";

// encodeURIComponent leaves '.' intact, so a bare '..'/'.' path param would
// dot-normalize inside fetch and steer the minted token at an upstream path
// other than the one this proxy exists to reach.
const DOT_ONLY_SEGMENT = /^\.+$/;

export async function GET(req: Request, { params }: Params) {
  const { workspaceId, fleetId } = await params;
  if (DOT_ONLY_SEGMENT.test(workspaceId) || DOT_ONLY_SEGMENT.test(fleetId)) {
    return new Response(JSON.stringify({ error: "Invalid path parameter" }), {
      status: 400,
      headers: {
        "Content-Type": CONTENT_TYPE_JSON,
        "Cache-Control": CACHE_CONTROL_NO_STORE,
      },
    });
  }

  const { getToken } = await auth();
  // Post-Stage-1: the customized default session token carries
  // `aud=https://api.agentsfleet.net` + tenant metadata, so bare `getToken()`
  // satisfies agentsfleetd's OIDC verifier (same as ./stream/route.ts).
  const token = await getToken();
  if (!token) {
    return new Response(JSON.stringify({ error: "Unauthorized", code: "UZ-401" }), {
      status: 401,
      headers: {
        "Content-Type": CONTENT_TYPE_JSON,
        "Cache-Control": CACHE_CONTROL_NO_STORE,
      },
    });
  }

  const incoming = new URL(req.url).searchParams;
  const forwarded = new URLSearchParams();
  for (const key of FORWARDED_QUERY_KEYS) {
    const value = incoming.get(key);
    if (value !== null) forwarded.set(key, value);
  }
  const qs = forwarded.toString();

  const upstreamUrl =
    `${API_ORIGIN}/v1/workspaces/${encodeURIComponent(workspaceId)}` +
    `/fleets/${encodeURIComponent(fleetId)}/events${qs.length > 0 ? `?${qs}` : ""}`;

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
    return new Response(JSON.stringify({ error: "Upstream unreachable" }), {
      status: 502,
      headers: {
        "Content-Type": CONTENT_TYPE_JSON,
        "Cache-Control": CACHE_CONTROL_NO_STORE,
      },
    });
  }

  if (!upstream.ok) {
    const text = await upstream.text().catch(() => "");
    // This GET is directly navigable on the dashboard origin with forwarded
    // query params — never reflect an upstream error under a markup-capable
    // content type.
    const upstreamType = upstream.headers.get("content-type") ?? CONTENT_TYPE_TEXT;
    return new Response(text || `Upstream error ${upstream.status}`, {
      status: upstream.status,
      headers: {
        "Content-Type": upstreamType.startsWith(CONTENT_TYPE_JSON) ? upstreamType : CONTENT_TYPE_TEXT,
        "Cache-Control": CACHE_CONTROL_NO_STORE,
      },
    });
  }

  // Buffered, not piped: the page is bounded by `limit` (upstream max 200),
  // so a long outage can never pull an unbounded body through this proxy.
  const body = await upstream.text();
  return new Response(body, {
    status: 200,
    headers: {
      "Content-Type": CONTENT_TYPE_JSON,
      "Cache-Control": CACHE_CONTROL_NO_STORE,
    },
  });
}
