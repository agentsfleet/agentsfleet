import { notFound, redirect } from "next/navigation";
import { getFleet } from "@/lib/api/fleets";
import { ApiError } from "@/lib/api/errors";
import { requireCredential } from "@/lib/auth/credential";
import { fleetSetupPath, workspacePath } from "@/lib/workspace-routes";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";

export default async function EditFleetPage({params}: {params: Promise<{workspaceId: string; id: string}>}) {
  const {workspaceId, id} = await params;
  const token = await requireCredential();
  const {fleet} = await getFleet(workspaceId, id, token).catch((error: unknown) => {
    if (error instanceof ApiError && error.status === 404) notFound();
    throw error;
  });
  redirect(fleet.status === AGENTSFLEET_STATUS.ARCHIVED
    ? workspacePath(workspaceId, `fleets/${id}`)
    : fleetSetupPath(workspaceId, id, fleet.setup_revision));
}
