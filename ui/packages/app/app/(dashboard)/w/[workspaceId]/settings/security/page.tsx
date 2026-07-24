import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { EmptyState, PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { ShieldIcon } from "lucide-react";

export const dynamic = "force-dynamic";

// Workspace-scoped concept page: it carries the `/w/[workspaceId]` segment (the
// security policy it will host is per-workspace) even though it renders no data
// yet.
export default async function SettingsSecurityPage({
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
        <PageTitle>Security</PageTitle>
      </PageHeader>
      <EmptyState
        icon={<ShieldIcon size={32} />}
        title="Security"
        description="Security and access policy for this workspace will live here."
      />
    </PageLayout>
  );
}
