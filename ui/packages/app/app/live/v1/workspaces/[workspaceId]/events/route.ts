// Same-origin backfill proxy for the workspace events list. The wall's
// reconnect gap-recovery fetch hits this Route Handler instead of the upstream
// list directly (the browser holds no bearer token). The per-fleet backfill
// proxy (../fleets/[fleetId]/events/route.ts) differs in the upstream path and
// in one forwarded key: `fleet_id`, so the wall can page one tile's history or
// the whole workspace's.

import { proxyJsonGet } from "@/lib/api/live-json-proxy";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = {
  params: Promise<{ workspaceId: string }>;
};

// Forwarded upstream verbatim; anything else is dropped so the proxy never
// widens the upstream surface. `fleet_id` is the workspace list's drill-down
// filter — the wall uses it to backfill a single tile.
const FORWARDED_QUERY_KEYS = ["cursor", "since", "limit", "fleet_id"] as const;

export async function GET(req: Request, { params }: Params) {
  const { workspaceId } = await params;
  return proxyJsonGet(req, ["workspaces", workspaceId, "events"], FORWARDED_QUERY_KEYS);
}
