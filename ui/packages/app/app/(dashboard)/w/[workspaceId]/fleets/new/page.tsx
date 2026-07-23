import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { listWorkspaceFleetLibraryCached } from "@/lib/api/fleet-library";
import { listSecrets } from "@/lib/api/secrets";
import { InstallFleet } from "./InstallFleet";
import { hasLibraryWriteScope } from "../scope";

export const dynamic = "force-dynamic";

type SearchParams = { library?: string | string[]; create?: string | string[] };
const INSTALL_PAGE_DESCRIPTION = "Start a fleet from the library. Watch it run in a loop.";

// Gallery-first install. Library entries + the workspace's existing
// credential names are fetched server-side (workspace from the URL) so the
// client orchestrator can render the gallery and the credential preview
// without a client round-trip.
export default async function InstallFleetPage({
  params,
  searchParams,
}: {
  params: Promise<{ workspaceId: string }>;
  searchParams: Promise<SearchParams>;
}) {
  const { workspaceId } = await params;
  const { getToken, sessionClaims } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const query = await searchParams;
  const [entries, credentialNames] = await Promise.all([
    listWorkspaceFleetLibraryCached(workspaceId, token)
      .then((response) => response.items)
      .catch(() => []),
    listSecrets(workspaceId, token)
      .then((response) => response.secrets.map((secret) => secret.name))
      // null (not []) when the vault read fails: the preview must not mistake an
      // unreadable vault for an empty one and falsely gate create.
      .catch(() => null),
  ]);

  const initialLibraryId =
    typeof query.library === "string" ? query.library : undefined;
  // ?create=1 (the dashboard empty-state CTA) opens the add-library-entry dialog
  // immediately — no second identical empty state between click and form.
  const initialCreateOpen = query.create === "1";

  return (
    <PageLayout>
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
    </PageLayout>
  );
}
