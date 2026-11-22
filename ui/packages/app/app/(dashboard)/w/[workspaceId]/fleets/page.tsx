import { redirect } from "next/navigation";
import { requireCredential } from "@/lib/auth/credential";
import { listFleets } from "@/lib/api/fleets";
import { workspacePath } from "@/lib/workspace-routes";

export const dynamic = "force-dynamic";

export default async function FleetsPage({params}: {params: Promise<{workspaceId: string}>}) {
  const {workspaceId} = await params;
  const token = await requireCredential();
  const {items} = await listFleets(workspaceId, token, {limit: 1});
  redirect(workspacePath(workspaceId, items[0] ? `fleets/${items[0].id}` : "fleets/new"));
}
