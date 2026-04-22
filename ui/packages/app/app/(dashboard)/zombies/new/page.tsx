import { auth } from "@clerk/nextjs/server";
import { notFound } from "next/navigation";
import { PageHeader, PageTitle } from "@usezombie/design-system";
import { resolveActiveWorkspace } from "@/lib/workspace";
import InstallZombieForm from "./InstallZombieForm";

export const dynamic = "force-dynamic";

// Blank-fields only for now; a skill-template picker ships once the
// backend exposes a skills catalog endpoint.
export default async function InstallZombiePage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) notFound();

  const workspace = await resolveActiveWorkspace(token);
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
