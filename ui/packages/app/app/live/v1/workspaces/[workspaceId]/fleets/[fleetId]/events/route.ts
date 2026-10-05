// Same-origin backfill proxy. The browser holds no bearer token, so the
// stream registry's reconnect gap-recovery fetch hits this Route Handler
// instead of the upstream events list directly. The page it carries is bounded
// by `limit` (upstream max 200).

import { proxyJsonGet } from "@/lib/api/live-json-proxy";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = {
  params: Promise<{ workspaceId: string; fleetId: string }>;
};

// Query keys forwarded upstream verbatim; anything else the caller sends is
// dropped so the proxy never widens the upstream surface.
const FORWARDED_QUERY_KEYS = ["cursor", "since", "limit"] as const;

export async function GET(req: Request, { params }: Params) {
  const { workspaceId, fleetId } = await params;
  return proxyJsonGet(req, ["workspaces", workspaceId, "fleets", fleetId, "events"], FORWARDED_QUERY_KEYS);
}
