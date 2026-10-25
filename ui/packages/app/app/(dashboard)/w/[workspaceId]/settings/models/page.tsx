import { RouteLoading } from "@/components/layout/RouteLoading";
import { Suspense } from "react";
import { PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { credential, requireCredential } from "@/lib/auth/credential";
import { listTenantModelEntriesCached } from "./lib/reads";
import { ModelCatalogueProvider } from "./components/ModelCatalogueProvider";
import ModelsRegistryTable from "./components/ModelsRegistryTable";
import { MODELS_PAGE_DESCRIPTION, MODELS_PAGE_TITLE } from "./copy";
import { libraryErrorFromCause, type LibraryError } from "@/lib/api/library-types";

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
  await requireCredential();

  return (
    <Suspense fallback={<RouteLoading />}>
      <PageLayout fullHeight className="h-full overflow-hidden">
        <PageHeader description={MODELS_PAGE_DESCRIPTION}>
          <PageTitle>{MODELS_PAGE_TITLE}</PageTitle>
        </PageHeader>

        <ModelsRegistryData workspaceId={workspaceId} />
      </PageLayout>
    </Suspense>
  );
}

/**
 * Async data region: reads the first registry page. Exported so it renders and
 * tests in isolation, matching `ApprovalsData`.
 *
 * The read never REJECTS. Suspense here buys latency, not error handling — a
 * rejected promise would throw in render and need an ErrorBoundary, trading
 * the failed-versus-empty distinction for an undifferentiated fallback.
 */
export async function ModelsRegistryData({ workspaceId }: { workspaceId: string }) {
  const token = await credential();
  if (!token) return null;

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
    error = libraryErrorFromCause(cause);
  }

  return (
    <ModelCatalogueProvider>
      <ModelsRegistryTable workspaceId={workspaceId} initialPage={page} initialError={error} />
    </ModelCatalogueProvider>
  );
}
