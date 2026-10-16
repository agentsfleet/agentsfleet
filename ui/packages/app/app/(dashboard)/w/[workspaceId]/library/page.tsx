import { PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { requireCredential } from "@/lib/auth/credential";
import { LibraryWorkspace } from "@/components/domain/chat/LibraryWorkspace";

import { LIBRARY_PAGE_TITLE, LIBRARY_PAGE_DESCRIPTION } from "./copy";

export const dynamic = "force-dynamic";

export default async function LibraryPage({ params }: { params: Promise<{ workspaceId: string }> }) {
  const { workspaceId } = await params;
  await requireCredential();
  return <PageLayout>
    <PageHeader description={LIBRARY_PAGE_DESCRIPTION}><PageTitle>{LIBRARY_PAGE_TITLE}</PageTitle></PageHeader>
    <LibraryWorkspace workspace={workspaceId} />
  </PageLayout>;
}
