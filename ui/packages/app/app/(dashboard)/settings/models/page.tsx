import { redirect } from "next/navigation";
import {
  EmptyState,
  PageHeader,
  PageTitle,
  SectionLabel,
  TerminalPanel,
} from "@agentsfleet/design-system";
import { ZapIcon } from "lucide-react";
import { auth } from "@clerk/nextjs/server";
import { withWorkspaceScope, orFallback } from "@/lib/workspace";
import { customSecretsOf } from "@/lib/api/credentials";
import { getTenantProviderCached, listCredentialsCached } from "./lib/reads";
import AddCredentialFormDynamic from "@/components/domain/island-dynamic/AddCredentialFormDynamic";
import CustomSecretsList from "@/app/(dashboard)/credentials/components/CustomSecretsList";
import { ModelCatalogueProvider } from "./components/ModelCatalogueProvider";
import ActiveModelHero from "./components/ActiveModelHero";
import ProviderSwitchList from "./components/ProviderSwitchList";

export const dynamic = "force-dynamic";

const PAGE_TITLE = "Models & Keys";
const PAGE_DESCRIPTION = "The model your fleets run on, and the keys behind it.";

export default async function ModelsKeysPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const result = await withWorkspaceScope(token, async (workspaceId) => {
    const [providerResult, credentialsResp] = await Promise.all([
      getTenantProviderCached(token).catch((err) => ({ error: String(err) })),
      listCredentialsCached(workspaceId, token).catch(orFallback({ credentials: [] })),
    ]);
    return { workspaceId, providerResult, credentialsResp };
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
  const { workspaceId, providerResult, credentialsResp } = result;
  // A provider-fetch error degrades the hero to the platform-default view rather
  // than failing the page; `provider` is null in that case.
  const provider = "error" in providerResult ? null : providerResult;
  const credentials = credentialsResp.credentials;
  // Classification is the server's `kind`, never a name heuristic — provider keys
  // and custom endpoints live in the model layer; only custom_secret rows here.
  const customSecrets = customSecretsOf(credentials);

  return (
    <div className="space-y-8">
      <PageHeader description={PAGE_DESCRIPTION}>
        <PageTitle>{PAGE_TITLE}</PageTitle>
      </PageHeader>

      <ModelCatalogueProvider>
        <div className="space-y-6">
          <ActiveModelHero workspaceId={workspaceId} provider={provider} credentials={credentials} />
          <ProviderSwitchList workspaceId={workspaceId} provider={provider} credentials={credentials} />
        </div>
      </ModelCatalogueProvider>

      <div aria-label="Custom secrets" data-testid="custom-secrets-group" className="space-y-md">
        <SectionLabel>Custom secrets</SectionLabel>
        <TerminalPanel title="Encrypted secrets" tag="write-only">
          <div className="p-lg">
            <CustomSecretsList workspaceId={workspaceId} secrets={customSecrets} />
          </div>
          <div className="border-t border-border bg-surface-deep p-lg" id="add-custom-secret">
            <div className="mb-md">
              <div className="font-medium text-foreground">Add a custom secret</div>
              <p className="text-body-sm leading-body-sm text-muted-foreground">
                Give it a name and one or more fields (like <span className="font-mono">api_key</span>). Once
                saved, values are encrypted and can&apos;t be viewed again — only replaced.{" "}
                <a
                  href="https://docs.agentsfleet.net/secrets"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-pulse underline-offset-2 hover:underline focus-visible:underline"
                >
                  Learn more<span className="sr-only"> (opens in a new tab)</span>
                </a>
              </p>
            </div>
            <AddCredentialFormDynamic workspaceId={workspaceId} />
          </div>
        </TerminalPanel>
      </div>
    </div>
  );
}
