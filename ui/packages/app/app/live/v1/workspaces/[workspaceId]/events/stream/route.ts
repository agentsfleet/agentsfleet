// Same-origin token-minting proxy for the ONE multiplexed workspace SSE stream.
// EventSource cannot set headers, so the browser hits this Route Handler; it
// resolves the Clerk session, mints an API-audience JWT, and pipes the upstream
// stream body straight back. Mirror of the per-fleet SSE proxy
// (../../fleets/[fleetId]/events/stream/route.ts), differing only in the
// upstream path — one workspace-scoped stream instead of one per fleet.
//
// See docs/AUTH.md "UI · SSE stream" for the full sequence.

import { auth } from "@clerk/nextjs/server";
import { API_ORIGIN } from "@/lib/api/client";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = {
  params: Promise<{ workspaceId: string }>;
};

// A bare '..'/'.' path param would dot-normalize inside fetch and steer the
// minted token at an upstream path other than the one intended.
const DOT_ONLY_SEGMENT = /^\.+$/;

export async function GET(req: Request, { params }: Params) {
  const { workspaceId } = await params;
  if (DOT_ONLY_SEGMENT.test(workspaceId)) {
    return new Response(JSON.stringify({ error: "Invalid path parameter" }), {
      status: 400,
      headers: { "Content-Type": "application/json" },
    });
  }

  const { getToken } = await auth();
  const token = await getToken();
  if (!token) {
    return new Response(JSON.stringify({ error: "Unauthorized", code: "UZ-401" }), {
      status: 401,
      headers: { "Content-Type": "application/json" },
    });
  }

  const upstreamUrl =
    `${API_ORIGIN}/v1/workspaces/${encodeURIComponent(workspaceId)}/events/stream`;

  let upstream: Response;
  try {
    upstream = await fetch(upstreamUrl, {
      method: "GET",
      headers: {
        Authorization: `Bearer ${token}`,
        Accept: "text/event-stream",
      },
      signal: req.signal,
    });
  } catch {
    return new Response("Upstream unreachable", {
      status: 502,
      headers: { "Content-Type": "text/plain" },
    });
  }

  if (!upstream.ok) {
    const text = await upstream.text().catch(() => "");
    return new Response(text || `Upstream error ${upstream.status}`, {
      status: upstream.status,
      headers: {
        "Content-Type": upstream.headers.get("content-type") ?? "text/plain",
      },
    });
  }
  if (!upstream.body) {
    return new Response("Upstream returned no body", {
      status: 502,
      headers: { "Content-Type": "text/plain" },
    });
  }

  return new Response(upstream.body, {
    status: 200,
    headers: {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache, no-transform",
      Connection: "keep-alive",
      // Defend against intermediary buffering (nginx, etc.) that would bunch
      // frames and defeat the live-tail UX.
      "X-Accel-Buffering": "no",
    },
  });
}
