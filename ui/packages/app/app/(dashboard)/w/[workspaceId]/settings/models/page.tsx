import { redirect } from "next/navigation";
import { PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { auth } from "@clerk/nextjs/server";
import { listSecretsCached, listTenantModelEntriesCached } from "./lib/reads";
import { ModelCatalogueProvider } from "./components/ModelCatalogueProvider";
import ModelsRegistryTable from "./components/ModelsRegistryTable";
import { MODELS_PAGE_DESCRIPTION, MODELS_PAGE_TITLE } from "./copy";

export const dynamic = "force-dynamic";

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
    <PageLayout>
      <PageHeader description={MODELS_PAGE_DESCRIPTION}>
        <PageTitle>{MODELS_PAGE_TITLE}</PageTitle>
      </PageHeader>

      <ModelCatalogueProvider>
        <ModelsRegistryTable workspaceId={workspaceId} initial={registry} initialSecrets={secretsResp.secrets} />
      </ModelCatalogueProvider>
    </PageLayout>
  );
}
