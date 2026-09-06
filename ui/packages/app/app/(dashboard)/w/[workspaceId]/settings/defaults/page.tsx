import { auth } from "@clerk/nextjs/server";
import Link from "next/link";
import { workspacePath } from "@/lib/workspace-routes";
import { redirect } from "next/navigation";
import { Button, EmptyState, PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { SlidersHorizontalIcon } from "lucide-react";

export const dynamic = "force-dynamic";

// Workspace-scoped concept page: it carries the `/w/[workspaceId]` segment (the
// defaults it will host are per-workspace) even though it renders no data yet.
export default async function SettingsDefaultsPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  return (
    <PageLayout>
      <PageHeader>
        <PageTitle>Defaults</PageTitle>
      </PageHeader>
      <EmptyState
        icon={<SlidersHorizontalIcon size={32} />}
        title="Workspace defaults aren’t available yet"
        description="Configure the model and credentials for each fleet from its workspace."
        action={<Button asChild><Link href={workspacePath(workspaceId, "fleets")}>Back to fleets</Link></Button>}
      />
    </PageLayout>
  );
}
