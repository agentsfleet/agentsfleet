import { redirect } from "next/navigation";
import {
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
          <PageTitle>Model</PageTitle>
        </PageHeader>
        <EmptyState
          icon={<ZapIcon size={32} />}
          title="No workspace yet"
          description="Create a workspace before configuring your model."
        />
      </div>
    );
  }

  const [provider, credentialsResp] = await Promise.all([
    getTenantProvider(token).catch((err) => ({ error: String(err) }) as never),
    listCredentials(workspace.id, token).catch(() => ({ credentials: [] })),
  ]);

  return (
    <div>
      <PageHeader>
        <PageTitle>Model</PageTitle>
      </PageHeader>
      <p className="mb-6 max-w-2xl text-sm text-muted-foreground">
        Choose between platform-managed credits (we handle the billing) and
        self-managed provider keys (your API key, your provider account,
        our flat per-event overhead). Platform-managed is the default for
        every new tenant.
      </p>
      <div className="grid gap-8 md:grid-cols-2">
        <Section asChild>
          <section aria-label="Active provider configuration" className="max-w-lg">
            <SectionLabel>Active configuration</SectionLabel>
            <DescriptionList className="mt-3">
              <div>
                <DescriptionTerm>Mode</DescriptionTerm>
                <DescriptionDetails className="font-medium capitalize">
                  {provider.mode ?? "—"}
                </DescriptionDetails>
              </div>
              <div>
                <DescriptionTerm>Provider</DescriptionTerm>
                <DescriptionDetails>{provider.provider ?? "—"}</DescriptionDetails>
              </div>
              <div>
                <DescriptionTerm>Model</DescriptionTerm>
                <DescriptionDetails mono>{provider.model ?? "—"}</DescriptionDetails>
              </div>
              <div>
                <DescriptionTerm>Context cap</DescriptionTerm>
                <DescriptionDetails className="tabular-nums">
                  {typeof provider.context_cap_tokens === "number"
                    ? new Intl.NumberFormat("en-US").format(provider.context_cap_tokens) + " tokens"
                    : "—"}
                </DescriptionDetails>
              </div>
              <div>
                <DescriptionTerm>Credential</DescriptionTerm>
                <DescriptionDetails mono>{provider.credential_ref ?? "—"}</DescriptionDetails>
              </div>
            </DescriptionList>
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
            />
          </section>
        </Section>
      </div>
    </div>
  );
}
