import { auth } from "@clerk/nextjs/server";
import Link from "next/link";
import { workspacePath } from "@/lib/workspace-routes";
import { redirect } from "next/navigation";
import { Button, EmptyState, PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
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
  const { workspaceId } = await params;
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
        title="Workspace policies aren’t available yet"
        description="Manage account security from your account menu. Workspace policy controls will appear here when available."
        action={<Button asChild><Link href={workspacePath(workspaceId, "fleets")}>Back to fleets</Link></Button>}
      />
    </PageLayout>
  );
}
