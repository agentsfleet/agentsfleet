// Same-origin token-minting proxy for the ONE multiplexed workspace SSE stream.
// EventSource cannot set headers, so the browser hits this Route Handler; it
// resolves the Clerk session, mints an API-audience JWT, and pipes the upstream
// stream body straight back. Mirror of the per-fleet SSE proxy
// (../../fleets/[fleetId]/events/stream/route.ts), differing only in the
// upstream path — one workspace-scoped stream instead of one per fleet.
//
// See docs/AUTH.md "UI · SSE stream" for the full sequence.

import { credential } from "@/lib/auth/credential";
import { API_ORIGIN } from "@/lib/api/client";
import { proxyEventStream } from "@/lib/api/event-stream-proxy";
import { ERROR_CODE } from "@/lib/errors";

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

  const token = await credential();
  if (!token) {
    return new Response(JSON.stringify({ error: "Unauthorized", code: ERROR_CODE.AUTH_401 }), {
      status: 401,
      headers: { "Content-Type": "application/json" },
    });
  }

  const upstreamUrl =
    `${API_ORIGIN}/v1/workspaces/${encodeURIComponent(workspaceId)}/events/stream`;

  return proxyEventStream(req, upstreamUrl, token);
}
