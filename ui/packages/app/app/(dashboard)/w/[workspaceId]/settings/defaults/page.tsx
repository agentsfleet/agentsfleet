import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { EmptyState, PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { SlidersHorizontalIcon } from "lucide-react";

export const dynamic = "force-dynamic";

// Workspace-scoped concept page: it carries the `/w/[workspaceId]` segment (the
// defaults it will host are per-workspace) even though it renders no data yet.
export default async function SettingsDefaultsPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  await params;
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
        title="Defaults"
        description="Workspace-wide defaults for new Fleets will live here."
      />
    </PageLayout>
  );
}
