import { redirect } from "next/navigation";
import { requireCredential } from "@/lib/auth/credential";
import { defaultFleetPath } from "@/lib/api/fleets";

export const dynamic = "force-dynamic";

export default async function FleetsPage({params}: {params: Promise<{workspaceId: string}>}) {
  const {workspaceId} = await params;
  const token = await requireCredential();
  redirect(await defaultFleetPath(workspaceId, token));
}
