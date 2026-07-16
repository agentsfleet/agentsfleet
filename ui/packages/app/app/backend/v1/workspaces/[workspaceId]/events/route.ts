// Same-origin backfill proxy for the workspace events list. The wall's
// reconnect gap-recovery fetch hits this Route Handler instead of the upstream
// list directly (the browser holds no bearer token). Mirror of the per-fleet
// backfill proxy (../fleets/[fleetId]/events/route.ts), differing in the
// upstream path (workspace-scoped) and one extra forwarded key: `fleet_id`, so
// the wall can page one tile's history or the whole workspace's.
//
// See docs/AUTH.md "UI · SSE stream" for the auth sequence.

import { auth } from "@clerk/nextjs/server";
import { API_ORIGIN } from "@/lib/api/client";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = {
  params: Promise<{ workspaceId: string }>;
};

// Forwarded upstream verbatim; anything else is dropped so the proxy never
// widens the upstream surface. `fleet_id` is the workspace list's drill-down
// filter — the wall uses it to backfill a single tile.
const FORWARDED_QUERY_KEYS = ["cursor", "since", "limit", "fleet_id"] as const;

const CONTENT_TYPE_JSON = "application/json";
const CONTENT_TYPE_TEXT = "text/plain";
const CACHE_CONTROL_NO_STORE = "no-store";
const DOT_ONLY_SEGMENT = /^\.+$/;

export async function GET(req: Request, { params }: Params) {
  const { workspaceId } = await params;
  if (DOT_ONLY_SEGMENT.test(workspaceId)) {
    return new Response(JSON.stringify({ error: "Invalid path parameter" }), {
      status: 400,
      headers: {
        "Content-Type": CONTENT_TYPE_JSON,
        "Cache-Control": CACHE_CONTROL_NO_STORE,
      },
    });
  }

  const { getToken } = await auth();
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
    `${API_ORIGIN}/v1/workspaces/${encodeURIComponent(workspaceId)}/events${qs.length > 0 ? `?${qs}` : ""}`;

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
    const upstreamType = upstream.headers.get("content-type") ?? CONTENT_TYPE_TEXT;
    return new Response(text || `Upstream error ${upstream.status}`, {
      status: upstream.status,
      headers: {
        "Content-Type": upstreamType.startsWith(CONTENT_TYPE_JSON) ? upstreamType : CONTENT_TYPE_TEXT,
        "Cache-Control": CACHE_CONTROL_NO_STORE,
      },
    });
  }

  // Buffered, not piped: the page is bounded by `limit` (upstream max 200), so a
  // long outage can never pull an unbounded body through this proxy.
  const body = await upstream.text();
  return new Response(body, {
    status: 200,
    headers: {
      "Content-Type": CONTENT_TYPE_JSON,
      "Cache-Control": CACHE_CONTROL_NO_STORE,
    },
  });
}
