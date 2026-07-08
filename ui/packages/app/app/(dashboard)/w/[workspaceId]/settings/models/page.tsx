import { redirect } from "next/navigation";
import { PageHeader, PageTitle } from "@agentsfleet/design-system";
import { auth } from "@clerk/nextjs/server";
import { listSecretsCached, listTenantModelEntriesCached } from "./lib/reads";
import { ModelCatalogueProvider } from "./components/ModelCatalogueProvider";
import ModelsRegistryTable from "./components/ModelsRegistryTable";

export const dynamic = "force-dynamic";

const PAGE_TITLE = "Models";
const PAGE_DESCRIPTION = "The model your fleets run on, and the key behind it.";

const EMPTY_REGISTRY = { models: [], platform_default_available: false };

export default async function ModelsKeysPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const [registry, secretsResp] = await Promise.all([
    listTenantModelEntriesCached(token).catch(() => EMPTY_REGISTRY),
    listSecretsCached(workspaceId, token).catch(() => ({ secrets: [] })),
  ]);

  return (
    <div className="space-y-8">
      <PageHeader description={PAGE_DESCRIPTION}>
        <PageTitle>{PAGE_TITLE}</PageTitle>
      </PageHeader>

      <ModelCatalogueProvider>
        <ModelsRegistryTable workspaceId={workspaceId} initial={registry} secrets={secretsResp.secrets} />
      </ModelCatalogueProvider>
    </div>
  );
}
