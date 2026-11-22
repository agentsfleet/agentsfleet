import { proxySetup } from "@/lib/api/setup-proxy";
import { proxyJsonGet } from "@/lib/api/live-json-proxy";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = {params: Promise<{workspaceId: string; fleetId: string; durableTaskId: string}>};

export async function GET(req: Request, {params}: Params) {
  const {workspaceId, fleetId, durableTaskId} = await params;
  return proxyJsonGet(req, ["workspaces", workspaceId, "fleets", fleetId, "durable_tasks", durableTaskId]);
}

export async function PATCH(req: Request, {params}: Params) {
  const {workspaceId, fleetId, durableTaskId} = await params;
  return proxySetup(req, {params: Promise.resolve({workspaceId, setupPath: ["fleets", fleetId, "durable_tasks", durableTaskId]})});
}
