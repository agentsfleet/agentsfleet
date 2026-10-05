// Same-origin read of one tool call in full: its arguments and every line of
// its output, which the chat's cell abbreviates to a preview. A Route Handler
// for the same reason as its parent: a Server Action would queue behind a send.
//
// Next hands the route a decoded call id (`{fence}:{n}`), and the proxy
// percent-encodes it again on the way up. A 404 means the full output was not
// kept, and passes through for the dialog to say so.

import { proxyJsonGet } from "@/lib/api/live-json-proxy";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = {
  params: Promise<{ workspaceId: string; fleetId: string; eventId: string; callId: string }>;
};

export async function GET(req: Request, { params }: Params) {
  const { workspaceId, fleetId, eventId, callId } = await params;
  return proxyJsonGet(req, ["workspaces", workspaceId, "fleets", fleetId, "events", eventId, "tool-calls", callId]);
}
