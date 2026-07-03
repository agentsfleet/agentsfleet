import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  CopyButton,
  DescriptionList,
  DescriptionTerm,
  DescriptionDetails,
  Section,
  SectionLabel,
} from "@agentsfleet/design-system";
import { setActiveWorkspace } from "@/app/(dashboard)/actions";
import { listTenantWorkspacesCached, resolveActiveWorkspaceId } from "@/lib/workspace";
import SettingsTabs from "@/components/layout/SettingsTabs";
import WorkspaceSwitcher from "@/components/layout/WorkspaceSwitcher";

export const dynamic = "force-dynamic";

const WORKSPACE_DESCRIPTION = "Switch workspaces or create one.";

export default async function SettingsPage({
  searchParams,
}: {
  searchParams?: Promise<{ notice?: string }>;
} = {}) {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // This page IS the switcher surface — it needs the full workspace list
  // anyway, so the active workspace object is derived from it (id from the
  // cookie/claim hint, name from the list). No separate resolve round-trip.
  const [active, workspaceList] = await Promise.all([
    resolveActiveWorkspaceId(token),
    listTenantWorkspacesCached(token).catch(() => ({ items: [], total: 0 })),
  ]);
  const workspaces = workspaceList.items;
  // Prefer the named entry from the list; if the list is unavailable but the
  // hint resolved an id, synthesize a name-less entry so the id (and switcher)
  // still render rather than collapsing to "no workspace".
  const workspace =
    workspaces.find((ws) => ws.id === active?.id) ??
    workspaces[0] ??
    (active ? { id: active.id, name: null, created_at: 0 } : null);
  const { notice } = (await searchParams) ?? {};

  return (
    <div className="space-y-8">
      <SettingsTabs title="Workspace" description={WORKSPACE_DESCRIPTION} />

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
        <div>
          <WorkspaceSwitcher
            workspaces={workspaces}
            activeId={workspace?.id ?? null}
            onSwitch={setActiveWorkspace}
            showCreateButton
            showManageItem={false}
          />
        </div>
        {/* Name + ID as aligned rows, each copyable — the ID is what the
            command line and the API target, so copy is one click. */}
        <DescriptionList className="mt-6">
          <div>
            <DescriptionTerm>Name</DescriptionTerm>
            <DescriptionDetails className="flex min-w-0 items-center gap-1">
              <span className="truncate">{workspace?.name ?? "—"}</span>
              {workspace?.name ? (
                <CopyButton value={workspace.name} label="Copy workspace name" />
              ) : null}
            </DescriptionDetails>
          </div>
          <div>
            <DescriptionTerm>Workspace ID</DescriptionTerm>
            <DescriptionDetails mono className="flex min-w-0 items-center gap-1">
              <span className="break-all">{workspace?.id ?? "—"}</span>
              {workspace?.id ? (
                <CopyButton value={workspace.id} label="Copy workspace ID" />
              ) : null}
            </DescriptionDetails>
          </div>
        </DescriptionList>
      </Section>
    </div>
  );
}
