import { redirect } from "next/navigation";
import { EmptyState, PageHeader, PageTitle, Section, SectionLabel } from "@agentsfleet/design-system";
import { ZapIcon } from "lucide-react";
import { auth } from "@clerk/nextjs/server";
import { withWorkspaceScope, orFallback } from "@/lib/workspace";
import { getTenantProvider } from "@/lib/api/tenant_provider";
import { listCredentials } from "@/lib/api/credentials";
import { getModelCaps, type ModelCap } from "@/lib/api/model_caps";
import { PROVIDER_MODE } from "@/lib/types";
import ProviderSelector from "./components/ProviderSelector";

export const dynamic = "force-dynamic";

const PAGE_TITLE = "Models";
const PAGE_DESCRIPTION =
  "Choose platform defaults or your key.";

export default async function ProviderSettingsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const result = await withWorkspaceScope(token, async (workspaceId) => {
    const [providerResult, credentialsResp, catalogue] = await Promise.all([
      getTenantProvider(token).catch((err) => ({ error: String(err) })),
      listCredentials(workspaceId, token).catch(orFallback({ credentials: [] })),
      getModelCaps()
        .then((caps) => caps.models)
        .catch(() => [] as ModelCap[]),
    ]);
    return { workspaceId, providerResult, credentialsResp, catalogue };
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
  const { workspaceId, providerResult, credentialsResp, catalogue } = result;
  // A provider-fetch error degrades the option cards to platform-default state
  // rather than failing the page; `provider` is null in that case.
  const provider = "error" in providerResult ? null : providerResult;
  const activeMode = provider?.mode ?? PROVIDER_MODE.platform;

  return (
    <div className="space-y-8">
      <PageHeader description={PAGE_DESCRIPTION}>
        <PageTitle>{PAGE_TITLE}</PageTitle>
      </PageHeader>

      <Section asChild>
        <section id="model-setup" aria-label="Model setup" className="scroll-mt-20">
          <SectionLabel>Model access</SectionLabel>
          <p className="max-w-2xl text-sm text-muted-foreground">
            Pick one setup. The current choice is marked.
          </p>
          <ProviderSelector
            workspaceId={workspaceId}
            currentMode={activeMode}
            currentCredentialRef={provider?.credential_ref ?? null}
            currentModel={provider?.model ?? ""}
            credentials={credentialsResp.credentials}
            catalogue={catalogue}
          />
        </section>
      </Section>
    </div>
  );
}
