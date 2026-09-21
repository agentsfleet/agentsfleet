import { PageHeader, PageLayout, PageTitle, Section } from "@agentsfleet/design-system";
import { requireCredential } from "@/lib/auth/credential";
import { listLibraryEntriesCached } from "./lib/reads";
import WorkspaceLibraryList from "./components/WorkspaceLibraryList";
import { LIBRARY_PAGE_DESCRIPTION, LIBRARY_PAGE_TITLE, LIBRARY_SECTION_LABEL } from "./copy";

export const dynamic = "force-dynamic";

// Only what this workspace onboarded. Platform entries do not appear here —
// not read-only, not greyed — and that is a property of the REQUEST rather
// than a filter this page applies: it reads the owned collection, which never
// carries a platform row. A client-side filter could drift; this cannot.
//
// The installable platform catalogue keeps its home in the gallery at
// /fleets/new, the way /v1/models keeps its home in the Add dialog's picker.
export default async function LibraryPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  const token = await requireCredential();
  const page = await listLibraryEntriesCached(workspaceId, token);

  return (
    <PageLayout fullHeight className="h-full overflow-hidden">
      <PageHeader description={LIBRARY_PAGE_DESCRIPTION}>
        <PageTitle>{LIBRARY_PAGE_TITLE}</PageTitle>
      </PageHeader>

      {/* No `asChild` wrapper: Section switches to the sectioning element on
          its own once it carries a label, precisely so a caller cannot lose
          the accessible name by forgetting to ask for it. */}
      <Section aria-label={LIBRARY_SECTION_LABEL} className="flex min-h-0 flex-1 flex-col gap-xl">
        <WorkspaceLibraryList workspaceId={workspaceId} entries={page.items} />
      </Section>
    </PageLayout>
  );
}
