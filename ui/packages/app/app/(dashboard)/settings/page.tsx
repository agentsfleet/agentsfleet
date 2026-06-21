import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  DescriptionList,
  DescriptionTerm,
  DescriptionDetails,
  Section,
  SectionLabel,
} from "@agentsfleet/design-system";
import { setActiveWorkspace } from "@/app/(dashboard)/actions";
import { listTenantWorkspacesCached, resolveActiveWorkspace } from "@/lib/workspace";
import SettingsTabs from "@/components/layout/SettingsTabs";
import WorkspaceSwitcher from "@/components/layout/WorkspaceSwitcher";

export const dynamic = "force-dynamic";

export default async function SettingsPage({
  searchParams,
}: {
  searchParams?: Promise<{ notice?: string }>;
} = {}) {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const [workspace, workspaceList] = await Promise.all([
    resolveActiveWorkspace(token),
    listTenantWorkspacesCached(token).catch(() => ({ items: [], total: 0 })),
  ]);
  const workspaces = workspaceList.items.length > 0
    ? workspaceList.items
    : workspace
      ? [workspace]
      : [];
  const { notice } = (await searchParams) ?? {};

  return (
    <div className="space-y-8">
      <SettingsTabs title="Workspace" />

      {notice === "api-keys-operator-only" ? (
        <Alert variant="warning">
          <div>
            <AlertTitle>API keys need admin access</AlertTitle>
            <AlertDescription>
              Ask a tenant admin to manage API keys.
            </AlertDescription>
          </div>
        </Alert>
      ) : null}

      <Section aria-label="Workspace" className="min-w-0 max-w-2xl">
        <SectionLabel>Manage workspace</SectionLabel>
        <p className="mt-2 text-sm text-muted-foreground">
          Switch the active workspace or create a new one.
        </p>
        <div className="mt-4">
          <WorkspaceSwitcher
            workspaces={workspaces}
            activeId={workspace?.id ?? null}
            onSwitch={setActiveWorkspace}
            showCreateButton
            showManageItem={false}
          />
        </div>
        <DescriptionList layout="stacked" className="mt-3 break-all">
          <div>
            <DescriptionTerm>Name</DescriptionTerm>
            <DescriptionDetails>{workspace?.name ?? "—"}</DescriptionDetails>
          </div>
          <div>
            <DescriptionTerm>Workspace ID</DescriptionTerm>
            <DescriptionDetails mono>{workspace?.id ?? "—"}</DescriptionDetails>
          </div>
        </DescriptionList>
      </Section>
    </div>
  );
}
