import { redirect } from "next/navigation";
import { PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { auth } from "@clerk/nextjs/server";
import { listTenantModelEntriesCached } from "./lib/reads";
import { ModelCatalogueProvider } from "./components/ModelCatalogueProvider";
import ModelsRegistryTable from "./components/ModelsRegistryTable";
import { MODELS_PAGE_DESCRIPTION, MODELS_PAGE_TITLE } from "./copy";
import {
  errorKindForStatus,
  LIBRARY_ERROR_KIND,
  type LibraryError,
} from "@/lib/api/library-types";

export const dynamic = "force-dynamic";

/**
 * An ordinary visit reads the FIRST registry page and nothing
 * else — no global model catalogue, no secret list.
 *
 * The secret list used to load here in parallel on every visit, to seed a
 * picker most visits never open. It now loads on the path that needs it,
 * through the refetch seam `ModelsRegistryTable` already owned.
 */
export default async function ModelsKeysPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // A failed read is NOT an empty registry. The previous `.catch(() => EMPTY)`
  // rendered "you have no models" at a user whose models were merely
  // unreachable, offering them no way to tell the difference and no way back.
  // The typed error reaches the table, which distinguishes it from empty and
  // offers retry.
  let page = null;
  let error: LibraryError | null = null;
  try {
    page = await listTenantModelEntriesCached(token);
  } catch (cause) {
    const status = (cause as { status?: number }).status;
    error = {
      kind: typeof status === "number" ? errorKindForStatus(status) : LIBRARY_ERROR_KIND.unknown,
      detail: cause instanceof Error ? cause.message : undefined,
    };
  }

  return (
    <PageLayout>
      <PageHeader description={MODELS_PAGE_DESCRIPTION}>
        <PageTitle>{MODELS_PAGE_TITLE}</PageTitle>
      </PageHeader>

      <ModelCatalogueProvider>
        <ModelsRegistryTable workspaceId={workspaceId} initialPage={page} initialError={error} />
      </ModelCatalogueProvider>
    </PageLayout>
  );
}
