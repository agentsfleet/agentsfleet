import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { PageHeader, PageTitle } from "@agentsfleet/design-system";
import { withWorkspaceScope, orFallback } from "@/lib/workspace";
import { listWorkspaceFleetTemplatesCached } from "@/lib/api/fleet-templates";
import { listCredentials } from "@/lib/api/credentials";
import { InstallFleet } from "./InstallFleet";
import { hasTemplateWriteScope } from "../scope";

export const dynamic = "force-dynamic";

type SearchParams = { template?: string | string[] };
const INSTALL_PAGE_DESCRIPTION = "Pick a template. Watch live states.";

// Gallery-first install. Templates + the workspace's existing
// credential names are fetched server-side so the client orchestrator can render
// the gallery and the credential preview without a client round-trip.
export default async function InstallFleetPage({
  searchParams,
}: {
  searchParams: Promise<SearchParams>;
}) {
  const { getToken, sessionClaims } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const params = await searchParams;
  const result = await withWorkspaceScope(token, async (workspaceId) => {
    const [templates, credentialNames] = await Promise.all([
      listWorkspaceFleetTemplatesCached(workspaceId, token)
        .then((response) => response.items)
        .catch(() => []),
      listCredentials(workspaceId, token)
        .then((response) => response.credentials.map((credential) => credential.name))
        // null (not []) when the vault read fails: the preview must not mistake an
        // unreadable vault for an empty one and falsely gate create.
        .catch(orFallback(null)),
    ]);
    return { workspaceId, templates, credentialNames };
  });
  if (!result) {
    return (
      <div>
        <PageHeader description={INSTALL_PAGE_DESCRIPTION}>
          <PageTitle>Install fleet</PageTitle>
        </PageHeader>
        <p className="text-sm text-muted-foreground">
          Create a workspace first.
        </p>
      </div>
    );
  }
  const { workspaceId, templates, credentialNames } = result;
  const initialTemplateId =
    typeof params.template === "string" ? params.template : undefined;

  return (
    <div>
      <PageHeader description={INSTALL_PAGE_DESCRIPTION}>
        <PageTitle>Install fleet</PageTitle>
      </PageHeader>
      <InstallFleet
        workspaceId={workspaceId}
        templates={templates}
        presentCredentialNames={credentialNames}
        initialTemplateId={initialTemplateId}
        canAddTemplate={hasTemplateWriteScope(sessionClaims)}
      />
    </div>
  );
}
