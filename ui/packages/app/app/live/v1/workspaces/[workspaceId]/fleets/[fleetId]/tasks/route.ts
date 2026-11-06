import { proxyJsonGet } from "@/lib/api/live-json-proxy";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = {params: Promise<{workspaceId: string; fleetId: string}>};

export async function GET(req: Request, {params}: Params) {
  const {workspaceId, fleetId} = await params;
  return proxyJsonGet(req, ["workspaces", workspaceId, "fleets", fleetId, "tasks"], ["starting_after", "limit"]);
}
