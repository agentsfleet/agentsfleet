import { redirect } from "next/navigation";
import { PageHeader, PageTitle } from "@agentsfleet/design-system";
import { auth } from "@clerk/nextjs/server";
import { getTenantProviderCached, listSecretsCached } from "./lib/reads";
import { ModelCatalogueProvider } from "./components/ModelCatalogueProvider";
import ProviderSwitchList from "./components/ProviderSwitchList";

export const dynamic = "force-dynamic";

const PAGE_TITLE = "Models";
const PAGE_DESCRIPTION = "The model your fleets run on, and the key behind it.";

export default async function ModelsKeysPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const [providerResult, secretsResp] = await Promise.all([
    getTenantProviderCached(token).catch((err) => ({ error: String(err) })),
    listSecretsCached(workspaceId, token).catch(() => ({ secrets: [] })),
  ]);
  // A provider-fetch error degrades the hero to the platform-default view rather
  // than failing the page; `provider` is null in that case.
  const provider = "error" in providerResult ? null : providerResult;
  const secrets = secretsResp.secrets;

  return (
    <div className="space-y-8">
      <PageHeader description={PAGE_DESCRIPTION}>
        <PageTitle>{PAGE_TITLE}</PageTitle>
      </PageHeader>

      <ModelCatalogueProvider>
        <ProviderSwitchList workspaceId={workspaceId} provider={provider} secrets={secrets} />
      </ModelCatalogueProvider>
    </div>
  );
}
