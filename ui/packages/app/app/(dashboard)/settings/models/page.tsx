import { redirect } from "next/navigation";
import {
  Card,
  DescriptionList,
  DescriptionTerm,
  DescriptionDetails,
  EmptyState,
  PageHeader,
  PageTitle,
  Section,
  SectionLabel,
} from "@usezombie/design-system";
import { ZapIcon } from "lucide-react";
import { auth } from "@clerk/nextjs/server";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { getTenantProvider } from "@/lib/api/tenant_provider";
import { listCredentials } from "@/lib/api/credentials";
import { getModelCaps, type ModelCap } from "@/lib/api/model_caps";
import { PROVIDER_MODE } from "@/lib/types";
import ProviderSelector from "./components/ProviderSelector";

export const dynamic = "force-dynamic";

export default async function ProviderSettingsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) {
    return (
      <div>
        <PageHeader>
          <PageTitle>Models</PageTitle>
        </PageHeader>
        <EmptyState
          icon={<ZapIcon size={32} />}
          title="No workspace yet"
          description="Create a workspace before configuring your model."
        />
      </div>
    );
  }

  const [provider, credentialsResp, catalogue] = await Promise.all([
    getTenantProvider(token).catch((err) => ({ error: String(err) }) as never),
    listCredentials(workspace.id, token).catch(() => ({ credentials: [] })),
    getModelCaps()
      .then((caps) => caps.models)
      .catch(() => [] as ModelCap[]),
  ]);

  return (
    <div>
      <PageHeader>
        <PageTitle>Models</PageTitle>
      </PageHeader>
      <p className="mb-6 max-w-2xl text-sm text-muted-foreground">
        Pick who pays for model usage: platform-managed credits (we bill you per event) or
        your own provider key (you bring the account, we add a flat per-event fee).
      </p>
      <div className="grid items-start gap-8 md:grid-cols-2">
        <Section asChild>
          <section aria-label="Active provider configuration" className="max-w-lg">
            <SectionLabel>Active configuration</SectionLabel>
            <Card asChild>
              <div>
                <DescriptionList className="[&>div]:justify-start [&>div]:gap-x-6">
                  <div>
                    <DescriptionTerm className="w-28 shrink-0">Mode</DescriptionTerm>
                    <DescriptionDetails className="font-medium capitalize">
                      {provider.mode ?? "—"}
                    </DescriptionDetails>
                  </div>
                  <div>
                    <DescriptionTerm className="w-28 shrink-0">Provider</DescriptionTerm>
                    <DescriptionDetails>{provider.provider ?? "—"}</DescriptionDetails>
                  </div>
                  <div>
                    <DescriptionTerm className="w-28 shrink-0">Model</DescriptionTerm>
                    <DescriptionDetails mono>{provider.model ?? "—"}</DescriptionDetails>
                  </div>
                  <div>
                    <DescriptionTerm className="w-28 shrink-0">Context cap</DescriptionTerm>
                    <DescriptionDetails className="tabular-nums">
                      {typeof provider.context_cap_tokens === "number"
                        ? new Intl.NumberFormat("en-US").format(provider.context_cap_tokens) + " tokens"
                        : "—"}
                    </DescriptionDetails>
                  </div>
                  <div>
                    <DescriptionTerm className="w-28 shrink-0">Credential</DescriptionTerm>
                    <DescriptionDetails mono>{provider.credential_ref ?? "—"}</DescriptionDetails>
                  </div>
                </DescriptionList>
              </div>
            </Card>
          </section>
        </Section>
        <Section asChild>
          <section aria-label="Change provider" className="max-w-lg">
            <SectionLabel>Change provider</SectionLabel>
            <ProviderSelector
              workspaceId={workspace.id}
              currentMode={provider.mode ?? PROVIDER_MODE.platform}
              currentCredentialRef={provider.credential_ref}
              currentModel={provider.model ?? ""}
              credentials={credentialsResp.credentials}
              catalogue={catalogue}
            />
          </section>
        </Section>
      </div>
    </div>
  );
}
