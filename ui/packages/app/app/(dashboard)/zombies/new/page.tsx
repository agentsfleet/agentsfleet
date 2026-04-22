import { auth } from "@clerk/nextjs/server";
import { notFound } from "next/navigation";
import { PageHeader, PageTitle } from "@usezombie/design-system";
import { listWorkspaces } from "@/lib/api";
import InstallZombieForm from "./InstallZombieForm";

export const dynamic = "force-dynamic";

// Blank-fields only for now; a skill-template picker ships once the
// backend exposes a skills catalog endpoint.
export default async function InstallZombiePage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) notFound();

  const workspaces = await listWorkspaces(token).then((r) => r.data);
  const workspace = workspaces[0];
  if (!workspace) {
    return (
      <div>
        <PageHeader>
          <PageTitle>Install Zombie</PageTitle>
        </PageHeader>
        <p className="text-sm text-muted-foreground">
          Create a workspace before installing zombies.
        </p>
      </div>
    );
  }

  return (
    <div>
      <PageHeader>
        <PageTitle>Install Zombie</PageTitle>
      </PageHeader>
      <InstallZombieForm workspaceId={workspace.id} />
    </div>
  );
}
