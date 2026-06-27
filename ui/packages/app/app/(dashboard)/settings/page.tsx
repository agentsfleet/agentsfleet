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
import { listTenantWorkspacesCached, resolveActiveWorkspaceId } from "@/lib/workspace";
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
          Switch workspaces or create one.
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
