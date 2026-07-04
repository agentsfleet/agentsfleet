import { redirect } from "next/navigation";
import { EmptyState, PageHeader, PageTitle } from "@agentsfleet/design-system";
import { ZapIcon } from "lucide-react";
import { auth } from "@clerk/nextjs/server";
import { withWorkspaceScope, orFallback } from "@/lib/workspace";
import { getTenantProviderCached, listSecretsCached } from "./lib/reads";
import { ModelCatalogueProvider } from "./components/ModelCatalogueProvider";
import ProviderSwitchList from "./components/ProviderSwitchList";

export const dynamic = "force-dynamic";

const PAGE_TITLE = "Models";
const PAGE_DESCRIPTION = "The model your fleets run on, and the key behind it.";

export default async function ModelsKeysPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const result = await withWorkspaceScope(token, async (workspaceId) => {
    const [providerResult, secretsResp] = await Promise.all([
      getTenantProviderCached(token).catch((err) => ({ error: String(err) })),
      listSecretsCached(workspaceId, token).catch(orFallback({ secrets: [] })),
    ]);
    return { workspaceId, providerResult, secretsResp };
  });
  if (!result) {
    return (
      <div>
        <PageHeader description={PAGE_DESCRIPTION}>
          <PageTitle>{PAGE_TITLE}</PageTitle>
        </PageHeader>
        <EmptyState
          icon={<ZapIcon size={32} />}
          title="No workspace yet"
          description="Create a workspace first."
        />
      </div>
    );
  }
  const { workspaceId, providerResult, secretsResp } = result;
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
