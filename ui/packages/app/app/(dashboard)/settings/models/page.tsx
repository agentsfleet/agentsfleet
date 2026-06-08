import { redirect } from "next/navigation";
import {
  Badge,
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
import { listCredentials, type CredentialSummary } from "@/lib/api/credentials";
import { getModelCaps, type ModelCap } from "@/lib/api/model_caps";
import { PROVIDER_MODE, type ProviderMode, type TenantProvider } from "@/lib/types";
import ProviderSelector from "./components/ProviderSelector";
import AddCredentialForm from "@/app/(dashboard)/credentials/components/AddCredentialForm";
import CredentialsList from "@/app/(dashboard)/credentials/components/CredentialsList";

export const dynamic = "force-dynamic";

const EMPTY_FIELD = "—";

const MODE_LABELS: Record<ProviderMode, string> = {
  platform: "Platform defaults",
  self_managed: "Own provider key",
};

function formatContextCap(tokens: number | null | undefined) {
  if (typeof tokens !== "number") return EMPTY_FIELD;
  return `${new Intl.NumberFormat("en-US").format(tokens)} tokens`;
}

function modeBadgeVariant(provider: TenantProvider | null, mode: ProviderMode) {
  if (!provider) return "destructive";
  return mode === PROVIDER_MODE.self_managed ? "cyan" : "green";
}

function CurrentModelSetup({
  provider,
  activeMode,
}: {
  provider: TenantProvider | null;
  activeMode: ProviderMode;
}) {
  return (
    <Section asChild>
      <section aria-label="Current model setup" className="max-w-5xl">
        <SectionLabel>Current setup</SectionLabel>
        <div className="rounded-md border border-border bg-card px-4 py-3">
          <div className="flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2">
                <Badge variant={modeBadgeVariant(provider, activeMode)}>
                  {provider ? MODE_LABELS[activeMode] : "Unavailable"}
                </Badge>
                <span className="text-sm font-medium text-foreground">
                  {provider?.provider ?? "Provider could not be loaded"}
                </span>
              </div>
              <p className="mt-1 break-all font-mono text-xs text-muted-foreground">
                {provider?.model ?? EMPTY_FIELD}
              </p>
            </div>
            <CurrentSetupFacts provider={provider} />
          </div>
        </div>
      </section>
    </Section>
  );
}

function CurrentSetupFacts({ provider }: { provider: TenantProvider | null }) {
  return (
    <dl className="grid gap-3 text-xs sm:grid-cols-2 lg:min-w-80">
      <div>
        <dt className="font-medium uppercase tracking-wide text-muted-foreground">Context</dt>
        <dd className="mt-1 tabular-nums text-foreground">
          {formatContextCap(provider?.context_cap_tokens)}
        </dd>
      </div>
      <div>
        <dt className="font-medium uppercase tracking-wide text-muted-foreground">Credential</dt>
        <dd className="mt-1 break-all font-mono text-foreground">
          {provider?.credential_ref ?? "No credential needed"}
        </dd>
      </div>
    </dl>
  );
}

function ModelSetupSection({
  workspaceId,
  provider,
  activeMode,
  credentials,
  catalogue,
}: {
  workspaceId: string;
  provider: TenantProvider | null;
  activeMode: ProviderMode;
  credentials: CredentialSummary[];
  catalogue: ModelCap[];
}) {
  return (
    <Section asChild>
      <section id="model-setup" aria-label="Model setup" className="max-w-3xl scroll-mt-20">
        <div className="space-y-2">
          <SectionLabel>Model setup</SectionLabel>
          <h2 className="font-mono text-heading text-foreground">
            How should agents use models?
          </h2>
          <p className="max-w-2xl text-sm text-muted-foreground">
            Platform defaults need no key. Own-key setup stores a provider credential and then
            points the model configuration at it.
          </p>
        </div>
        <ProviderSelector
          workspaceId={workspaceId}
          currentMode={activeMode}
          currentCredentialRef={provider?.credential_ref ?? null}
          currentModel={provider?.model ?? ""}
          credentials={credentials}
          catalogue={catalogue}
        />
      </section>
    </Section>
  );
}

function CredentialVaultSection({
  workspaceId,
  credentials,
}: {
  workspaceId: string;
  credentials: CredentialSummary[];
}) {
  return (
    <Section asChild>
      <section id="credentials" className="max-w-5xl scroll-mt-20" aria-label="Credential vault">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
          <div className="space-y-2">
            <SectionLabel>Credential vault</SectionLabel>
            <h2 className="font-mono text-heading text-foreground">Credentials</h2>
            <p className="max-w-2xl text-sm text-muted-foreground">
              Encrypted write-only secrets for agents and model providers. Reference one by name,
              e.g. <code>{"${secrets.fly.api_token}"}</code>.
            </p>
          </div>
          <a href="#model-setup" className="text-sm font-medium text-primary underline">
            Use a key in model setup
          </a>
        </div>
        <div className="space-y-4">
          <CredentialsList workspaceId={workspaceId} credentials={credentials} />
          <details className="rounded-md border border-dashed border-border bg-card">
            <summary className="cursor-pointer px-4 py-3">
              <span className="block font-medium text-foreground">Add a generic credential</span>
              <span className="block text-xs text-muted-foreground">
                Use this for non-model secrets or custom JSON payloads.
              </span>
            </summary>
            <div className="border-t border-border p-4">
              <AddCredentialForm workspaceId={workspaceId} />
            </div>
          </details>
        </div>
      </section>
    </Section>
  );
}

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
  const activeMode = provider?.mode ?? PROVIDER_MODE.platform;

  return (
    <div className="space-y-10">
      <PageHeader className="max-w-3xl">
        <PageTitle>Models &amp; Credentials</PageTitle>
        <p className="mt-2 text-sm text-muted-foreground">
          Choose whether agents use platform defaults or your own provider key. Keys live in the
          credential vault and are never shown again after save.
        </p>
      </PageHeader>

      <CurrentModelSetup provider={provider} activeMode={activeMode} />
      <ModelSetupSection
        workspaceId={workspace.id}
        provider={provider}
        activeMode={activeMode}
        credentials={credentialsResp.credentials}
        catalogue={catalogue}
      />
      <CredentialVaultSection workspaceId={workspace.id} credentials={credentialsResp.credentials} />
    </div>
  );
}
