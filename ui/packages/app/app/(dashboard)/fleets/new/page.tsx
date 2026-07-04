import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { PageHeader, PageTitle } from "@agentsfleet/design-system";
import { withWorkspaceScope, orFallback } from "@/lib/workspace";
import { listWorkspaceFleetLibraryCached } from "@/lib/api/fleet-library";
import { listSecrets } from "@/lib/api/secrets";
import { InstallFleet } from "./InstallFleet";
import { hasLibraryWriteScope } from "../scope";

export const dynamic = "force-dynamic";

type SearchParams = { library?: string | string[]; create?: string | string[] };
const INSTALL_PAGE_DESCRIPTION = "Start a fleet from the library. Watch it run in a loop.";

// Gallery-first install. Library entries + the workspace's existing
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
    const [entries, credentialNames] = await Promise.all([
      listWorkspaceFleetLibraryCached(workspaceId, token)
        .then((response) => response.items)
        .catch(() => []),
      listSecrets(workspaceId, token)
        .then((response) => response.secrets.map((secret) => secret.name))
        // null (not []) when the vault read fails: the preview must not mistake an
        // unreadable vault for an empty one and falsely gate create.
        .catch(orFallback(null)),
    ]);
    return { workspaceId, entries, credentialNames };
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
  const { workspaceId, entries, credentialNames } = result;
  const initialLibraryId =
    typeof params.library === "string" ? params.library : undefined;
  // ?create=1 (the dashboard empty-state CTA) opens the add-library-entry dialog
  // immediately — no second identical empty state between click and form.
  const initialCreateOpen = params.create === "1";

  return (
    <div>
      <PageHeader description={INSTALL_PAGE_DESCRIPTION}>
        <PageTitle>Install fleet</PageTitle>
      </PageHeader>
      <InstallFleet
        workspaceId={workspaceId}
        entries={entries}
        presentCredentialNames={credentialNames}
        initialLibraryId={initialLibraryId}
        canAddLibraryEntry={hasLibraryWriteScope(sessionClaims)}
        initialCreateOpen={initialCreateOpen}
      />
    </div>
  );
}
