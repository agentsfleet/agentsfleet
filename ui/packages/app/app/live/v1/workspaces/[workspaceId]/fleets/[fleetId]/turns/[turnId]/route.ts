// Same-origin read of one turn's saved row, and the composer's Stop of it. The
// chat reads the row when a reply's ending never reached the stream; Stop
// PATCHes its status through the whitelisted setup proxy, which checks origin,
// media type and size before it mints a token. A Route Handler, not a Server
// Action: Next runs a tab's Server Actions one at a time, so the read would
// queue behind a send in flight and the next send behind the read.
//
// The static `stream` sibling wins over this dynamic segment, so the stream
// proxy is unaffected; a turn id is a UUID and never collides with it.

import { proxyJsonGet } from "@/lib/api/live-json-proxy";
import { proxySetup } from "@/lib/api/setup-proxy";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = {
  params: Promise<{ workspaceId: string; fleetId: string; turnId: string }>;
};

export async function GET(req: Request, { params }: Params) {
  const { workspaceId, fleetId, turnId } = await params;
  return proxyJsonGet(req, ["workspaces", workspaceId, "fleets", fleetId, "turns", turnId]);
}

export async function PATCH(req: Request, { params }: Params) {
  const { workspaceId, fleetId, turnId } = await params;
  return proxySetup(req, { params: Promise.resolve({ workspaceId, setupPath: ["fleets", fleetId, "turns", turnId] }) });
}
