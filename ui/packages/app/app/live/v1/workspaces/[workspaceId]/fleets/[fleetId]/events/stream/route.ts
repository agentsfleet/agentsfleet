// Server-Sent Events proxy. EventSource cannot set headers, so the browser
// hits this same-origin Route Handler instead of the upstream API directly.
// We resolve the user's Clerk session, mint an API-audience JWT, and pipe
// the upstream stream body straight back to the client.
//
// See docs/AUTH.md "UI · SSE stream" for the full sequence.

import { credential } from "@/lib/auth/credential";
import { API_ORIGIN } from "@/lib/api/client";
import { proxyEventStream } from "@/lib/api/event-stream-proxy";
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

  return proxyEventStream(req, upstreamUrl, token);
}
