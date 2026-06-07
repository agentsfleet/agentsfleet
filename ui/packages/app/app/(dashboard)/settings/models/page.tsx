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
import AddCredentialForm from "@/app/(dashboard)/credentials/components/AddCredentialForm";
import CredentialsList from "@/app/(dashboard)/credentials/components/CredentialsList";

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
          <PageTitle>Models &amp; Credentials</PageTitle>
        </PageHeader>
        <EmptyState
          icon={<ZapIcon size={32} />}
          title="No workspace yet"
          description="Create a workspace before configuring your model."
        />
      </div>
    );
  }

  const [providerResult, credentialsResp, catalogue] = await Promise.all([
    getTenantProvider(token).catch((err) => ({ error: String(err) })),
    listCredentials(workspace.id, token).catch(() => ({ credentials: [] })),
    getModelCaps()
      .then((caps) => caps.models)
      .catch(() => [] as ModelCap[]),
  ]);
  // A provider-fetch error degrades the config card to em-dash placeholders
  // rather than failing the page; `provider` is null in that case.
  const provider = "error" in providerResult ? null : providerResult;
  const contextCap = provider?.context_cap_tokens;

  return (
    <div className="space-y-12">
      <PageHeader>
        <PageTitle>Models &amp; Credentials</PageTitle>
      </PageHeader>

      {/* § MODEL — who runs the model, and which one. */}
      <div>
        <h2 className="mb-2 font-mono text-heading text-foreground">Model</h2>
        <p className="mb-6 max-w-2xl text-sm text-muted-foreground">
          Pick who pays for model usage: platform-managed credits (we bill you per event) or
          your own provider key (you bring the account, we add a flat per-event fee). Provider
          keys are stored below in Credentials.
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
                        {provider?.mode ?? "—"}
                      </DescriptionDetails>
                    </div>
                    <div>
                      <DescriptionTerm className="w-28 shrink-0">Provider</DescriptionTerm>
                      <DescriptionDetails>{provider?.provider ?? "—"}</DescriptionDetails>
                    </div>
                    <div>
                      <DescriptionTerm className="w-28 shrink-0">Model</DescriptionTerm>
                      <DescriptionDetails mono>{provider?.model ?? "—"}</DescriptionDetails>
                    </div>
                    <div>
                      <DescriptionTerm className="w-28 shrink-0">Context cap</DescriptionTerm>
                      <DescriptionDetails className="tabular-nums">
                        {typeof contextCap === "number"
                          ? new Intl.NumberFormat("en-US").format(contextCap) + " tokens"
                          : "—"}
                      </DescriptionDetails>
                    </div>
                    <div>
                      <DescriptionTerm className="w-28 shrink-0">Credential</DescriptionTerm>
                      <DescriptionDetails mono>{provider?.credential_ref ?? "—"}</DescriptionDetails>
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
                currentMode={provider?.mode ?? PROVIDER_MODE.platform}
                currentCredentialRef={provider?.credential_ref ?? null}
                currentModel={provider?.model ?? ""}
                credentials={credentialsResp.credentials}
                catalogue={catalogue}
              />
            </section>
          </Section>
        </div>
      </div>

      {/* § CREDENTIALS — the secrets agents resolve, incl. the provider keys above. */}
      <div id="credentials" className="scroll-mt-20">
        <h2 className="mb-2 font-mono text-heading text-foreground">Credentials</h2>
        <p className="mb-6 max-w-2xl text-sm text-muted-foreground">
          Encrypted secrets your agents use to reach other services. Reference one by name, e.g.{" "}
          <code>{"${secrets.fly.api_token}"}</code>. Values are write-only — edit to rotate or
          rename.
        </p>
        <div className="grid items-start gap-8 md:grid-cols-2">
          <Section asChild>
            <section aria-label="Stored credentials">
              <SectionLabel>Stored credentials</SectionLabel>
              <CredentialsList workspaceId={workspace.id} credentials={credentialsResp.credentials} />
            </section>
          </Section>
          <Section asChild>
            <section aria-label="Add credential">
              <SectionLabel>Add credential</SectionLabel>
              <AddCredentialForm workspaceId={workspace.id} />
            </section>
          </Section>
        </div>
      </div>
    </div>
  );
}
