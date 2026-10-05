// Same-origin read of one event's saved row. The chat reads it when a reply's
// ending never reached the stream. A Route Handler, not a Server Action: Next
// runs a tab's Server Actions one at a time, so the read would queue behind a
// send in flight and the next send behind the read.
//
// The static `stream` sibling wins over this dynamic segment, so the stream
// proxy is unaffected; event ids are `<millis>-<seq>` and never collide.

import { proxyJsonGet } from "@/lib/api/live-json-proxy";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = {
  params: Promise<{ workspaceId: string; fleetId: string; eventId: string }>;
};

export async function GET(req: Request, { params }: Params) {
  const { workspaceId, fleetId, eventId } = await params;
  return proxyJsonGet(req, ["workspaces", workspaceId, "fleets", fleetId, "events", eventId]);
}
