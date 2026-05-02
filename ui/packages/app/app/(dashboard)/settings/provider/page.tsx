import { notFound } from "next/navigation";
import {
  EmptyState,
  PageHeader,
  PageTitle,
  Section,
  SectionLabel,
} from "@usezombie/design-system";
import { ZapIcon } from "lucide-react";
import { getServerToken } from "@/lib/auth/server";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { getTenantProvider } from "@/lib/api/tenant_provider";
import { listCredentials } from "@/lib/api/credentials";
import ProviderSelector from "./components/ProviderSelector";

export const dynamic = "force-dynamic";

export default async function ProviderSettingsPage() {
  const token = await getServerToken();
  if (!token) notFound();

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) {
    return (
      <div>
        <PageHeader>
          <PageTitle>LLM Provider</PageTitle>
        </PageHeader>
        <EmptyState
          icon={<ZapIcon size={32} />}
          title="No workspace yet"
          description="Create a workspace before configuring your LLM provider."
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
        <PageTitle>LLM Provider</PageTitle>
      </PageHeader>
      <p className="mb-6 max-w-2xl text-sm text-muted-foreground">
        Choose between platform-managed credits (we handle the billing) and
        Bring-Your-Own-Key (your API key, your provider account, our flat
        per-event overhead). Platform-managed is the default for every new
        tenant.
      </p>
      <div className="grid gap-8 md:grid-cols-2">
        <Section asChild>
          <section aria-label="Active provider configuration" className="max-w-lg">
            <SectionLabel>Active configuration</SectionLabel>
            <dl className="mt-3 space-y-3 text-sm">
              <div className="flex justify-between">
                <dt className="text-muted-foreground">Mode</dt>
                <dd className="font-medium capitalize">{provider.mode ?? "—"}</dd>
              </div>
              <div className="flex justify-between">
                <dt className="text-muted-foreground">Provider</dt>
                <dd>{provider.provider ?? "—"}</dd>
              </div>
              <div className="flex justify-between">
                <dt className="text-muted-foreground">Model</dt>
                <dd className="font-mono text-xs">{provider.model ?? "—"}</dd>
              </div>
              <div className="flex justify-between">
                <dt className="text-muted-foreground">Context cap</dt>
                <dd className="tabular-nums">
                  {typeof provider.context_cap_tokens === "number"
                    ? provider.context_cap_tokens.toLocaleString() + " tokens"
                    : "—"}
                </dd>
              </div>
              <div className="flex justify-between">
                <dt className="text-muted-foreground">Credential</dt>
                <dd className="font-mono text-xs">{provider.credential_ref ?? "—"}</dd>
              </div>
            </dl>
            {provider.synthesised_default ? (
              <p className="mt-4 text-xs italic text-muted-foreground animate-in fade-in-0 duration-200">
                This is the platform default — no tenant override is set.
              </p>
            ) : null}
            {provider.error ? (
              <p
                role="alert"
                className="mt-4 rounded-md border border-destructive/40 bg-destructive/10 p-3 text-xs text-destructive animate-in fade-in-0 slide-in-from-top-1 duration-200"
              >
                ⚠ Provider resolver error: <code className="font-mono">{provider.error}</code>
                {provider.credential_ref ? (
                  <>
                    {" "}(credential_ref=<code className="font-mono">{provider.credential_ref}</code>)
                  </>
                ) : null}
                . Re-add the credential under the same name OR reset to the
                platform default.
              </p>
            ) : null}
          </section>
        </Section>
        <Section asChild>
          <section aria-label="Change provider" className="max-w-lg">
            <SectionLabel>Change provider</SectionLabel>
            <ProviderSelector
              workspaceId={workspace.id}
              currentMode={provider.mode ?? "platform"}
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
