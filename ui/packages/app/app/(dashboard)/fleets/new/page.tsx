import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { PageHeader, PageTitle } from "@agentsfleet/design-system";
import { resolveActiveWorkspace } from "@/lib/workspace";
import InstallFleetForm from "./InstallFleetForm";

export const dynamic = "force-dynamic";

// Blank-fields only for now; a skill-template picker ships once the
// backend exposes a skills catalog endpoint.
export default async function InstallFleetPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) {
    return (
      <div>
        <PageHeader>
          <PageTitle>Install teammate</PageTitle>
        </PageHeader>
        <p className="text-sm text-muted-foreground">
          Create a workspace before installing teammates.
        </p>
      </div>
    );
  }

  return (
    <div>
      <PageHeader>
        <PageTitle>Install teammate</PageTitle>
      </PageHeader>
      <InstallFleetForm workspaceId={workspace.id} />
    </div>
  );
}
