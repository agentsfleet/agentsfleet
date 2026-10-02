// Server-Sent Events proxy. EventSource cannot set headers, so the browser
// hits this same-origin Route Handler instead of the upstream API directly.
// We resolve the user's Clerk session, mint an API-audience JWT, and pipe
// the upstream stream body straight back to the client.
//
// See docs/AUTH.md "UI · SSE stream" for the full sequence.

import { credential } from "@/lib/auth/credential";
import { API_ORIGIN } from "@/lib/api/client";
import { accessRevokedResponse, eventStreamResponse, isAccessRefusal } from "@/lib/api/event-stream-proxy";
import { ERROR_CODE } from "@/lib/errors";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = {
  params: Promise<{ workspaceId: string; fleetId: string }>;
};

export async function GET(req: Request, { params }: Params) {
  const { workspaceId, fleetId } = await params;

  const token = await credential();
  if (!token) {
    return new Response(JSON.stringify({ error: "Unauthorized", code: ERROR_CODE.AUTH_401 }), {
      status: 401,
      headers: { "Content-Type": "application/json" },
    });
  }

  const upstreamUrl =
    `${API_ORIGIN}/v1/workspaces/${encodeURIComponent(workspaceId)}` +
    `/fleets/${encodeURIComponent(fleetId)}/events/stream`;

  const upstream = await fetch(upstreamUrl, {
    method: "GET",
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: "text/event-stream",
    },
    signal: req.signal,
  });

  if (!upstream.ok) {
    const text = await upstream.text().catch(() => "");
    // A member removed while their tab slept is refused here, at open, where
    // the browser would see only a bare `error` and reconnect forever.
    if (isAccessRefusal(upstream.status, text)) return accessRevokedResponse();
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

  return eventStreamResponse(upstream.body);
}
