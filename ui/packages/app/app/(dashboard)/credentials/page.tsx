import { redirect } from "next/navigation";
import Link from "next/link";
import { auth } from "@clerk/nextjs/server";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
  Button,
  DashboardPanel,
  DashboardRowGroup,
  EmptyState,
  PageHeader,
  PageTitle,
  Section,
  SectionLabel,
  StatusPill,
  TerminalPanel,
} from "@agentsfleet/design-system";
import { CpuIcon, KeyRoundIcon, LinkIcon } from "lucide-react";
import { withWorkspaceScope, orFallback } from "@/lib/workspace";
import { getTenantProvider } from "@/lib/api/tenant_provider";
import { listCredentials, type CredentialSummary } from "@/lib/api/credentials";
import { OPENAI_COMPATIBLE_PROVIDER, PROVIDER_MODE, type TenantProvider } from "@/lib/types";
import { VAULT_KIND, VAULT_KINDS } from "./lib/vault-kinds";
import AddCredentialFormDynamic from "@/components/domain/island-dynamic/AddCredentialFormDynamic";
import CustomSecretsList from "./components/CustomSecretsList";
import CustomEndpointForm from "./components/CustomEndpointForm";
import IntegrationsComingSoon from "./components/IntegrationsComingSoon";
import ProviderCredentialRows from "./components/ProviderCredentialRows";

export const dynamic = "force-dynamic";

const PAGE_TITLE = "Credentials";
const PAGE_DESCRIPTION =
  "Write-only keys for models, tools, and secrets.";
const ADD_CREDENTIAL_LABEL = "Add credential";
const NOT_CONNECTED = "Not connected";
const CONNECTED = "Connected";

const KIND_ICON = {
  [VAULT_KIND.providers]: CpuIcon,
  [VAULT_KIND.custom]: KeyRoundIcon,
  [VAULT_KIND.integrations]: LinkIcon,
} as const;

function KindsStrip() {
  return (
    <div className="grid gap-3 sm:grid-cols-3" data-testid="vault-kinds-strip">
      {VAULT_KINDS.map((kind) => (
        <DashboardPanel
          key={kind.kind}
          data-testid={`vault-kind-${kind.kind}`}
          padding="compact"
        >
          <div className="flex items-center gap-2 font-medium text-foreground">
            {(() => {
              const Icon = KIND_ICON[kind.kind];
              return <Icon size={15} className="text-pulse" aria-hidden="true" />;
            })()}
            {kind.label}
          </div>
          <p className="mt-1 text-body-sm leading-body-sm text-muted-foreground">{kind.blurb}</p>
          <p className="mt-2 font-mono text-label leading-label text-text-subtle">{kind.examples}</p>
        </DashboardPanel>
      ))}
    </div>
  );
}

function CustomEndpointRow({
  workspaceId,
  provider,
}: {
  workspaceId: string;
  provider: TenantProvider | null;
}) {
  const connected =
    provider?.mode === PROVIDER_MODE.self_managed &&
    provider.provider === OPENAI_COMPATIBLE_PROVIDER;
  // Built on the shared Accordion primitive (reused from TriggerPanel); the
  // primitive itself is untouched — row-specific look comes from className
  // overrides resolved by tailwind-merge, so other Accordion usages are
  // unaffected.
  return (
    <Accordion type="single" collapsible>
      <AccordionItem value="custom-endpoint" className="border-b-0">
        <AccordionTrigger className="items-start gap-3 px-lg py-md font-normal hover:bg-secondary hover:no-underline">
          <span
            className="grid h-8 w-8 flex-none place-items-center rounded-md border border-border bg-secondary text-muted-foreground"
            aria-hidden="true"
          >
            <LinkIcon size={15} />
          </span>
          <span className="min-w-0 flex-1 text-left">
            <span className="block font-medium text-foreground">Custom — OpenAI-compatible</span>
            <span className="mt-1 block text-body-sm leading-body-sm text-muted-foreground">
              OpenAI-compatible URL. Gateway, OpenRouter, or self-hosted.
            </span>
          </span>
          <StatusPill variant={connected ? "success" : "neutral"} dot={connected}>
            {connected ? CONNECTED : NOT_CONNECTED}
          </StatusPill>
        </AccordionTrigger>
        <AccordionContent className="px-lg pb-lg pt-0">
          <div className="border-t border-border pt-md">
            <CustomEndpointForm workspaceId={workspaceId} />
          </div>
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}

function ProvidersGroup({
  workspaceId,
  provider,
}: {
  workspaceId: string;
  provider: TenantProvider | null;
}) {
  return (
    <Section asChild>
      <section aria-label="Model providers" data-testid={`group-${VAULT_KIND.providers}`}>
        <SectionLabel>Model providers</SectionLabel>
        <DashboardRowGroup data-testid="provider-credential-rows">
          <ProviderCredentialRows workspaceId={workspaceId} provider={provider} />
          <CustomEndpointRow workspaceId={workspaceId} provider={provider} />
        </DashboardRowGroup>
      </section>
    </Section>
  );
}

function CustomSecretsGroup({
  workspaceId,
  secrets,
}: {
  workspaceId: string;
  secrets: CredentialSummary[];
}) {
  return (
    <Section asChild>
      <section aria-label="Custom secrets" data-testid={`group-${VAULT_KIND.custom}`}>
        <SectionLabel>Custom secrets</SectionLabel>
        <TerminalPanel title="vault · resolved by name" tag="write-only">
          <div className="p-lg">
            <CustomSecretsList workspaceId={workspaceId} secrets={secrets} />
          </div>
          <div className="border-t border-border bg-surface-deep p-lg" id="add-custom-secret">
            <div className="mb-md">
              <div className="font-medium text-foreground">Add a custom secret</div>
              <p className="text-body-sm leading-body-sm text-muted-foreground">
                Store JSON. Use{" "}
                <code className="font-mono">secrets.NAME.FIELD</code>.
              </p>
            </div>
            <AddCredentialFormDynamic workspaceId={workspaceId} />
          </div>
        </TerminalPanel>
      </section>
    </Section>
  );
}

function IntegrationsGroup({ secrets }: { secrets: CredentialSummary[] }) {
  return (
    <Section asChild>
      <section aria-label="Integrations" data-testid={`group-${VAULT_KIND.integrations}`}>
        <SectionLabel>Integrations</SectionLabel>
        <IntegrationsComingSoon credentialNames={secrets.map((secret) => secret.name)} />
      </section>
    </Section>
  );
}

export default async function CredentialsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const result = await withWorkspaceScope(token, async (workspaceId) => {
    const [providerResult, credentialsResp] = await Promise.all([
      getTenantProvider(token).catch((err) => ({ error: String(err) })),
      listCredentials(workspaceId, token).catch(orFallback({ credentials: [] })),
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
          icon={<KeyRoundIcon size={32} />}
          title="No workspace yet"
          description="Create a workspace first."
        />
      </div>
    );
  }
  const { workspaceId, providerResult, credentialsResp } = result;
  // The only KNOWN reference is the active model credential; a provider-fetch
  // error simply means we surface no referenced-by hint (never fabricated).
  const provider = "error" in providerResult ? null : providerResult;
  const activeModelRef = provider?.credential_ref ?? null;

  const credentials = credentialsResp.credentials;
  // Best-effort split: a stored credential the active model setup points at is a
  // model-provider key; everything else is a custom secret. No usage graph is
  // synthesized beyond this one known reference.
  const customSecrets = credentials.filter((c) => c.name !== activeModelRef);

  return (
    <div className="space-y-8">
      <PageHeader
        description={PAGE_DESCRIPTION}
        actions={
          <Button asChild>
            <Link href="#add-custom-secret">{ADD_CREDENTIAL_LABEL}</Link>
          </Button>
        }
      >
        <PageTitle>{PAGE_TITLE}</PageTitle>
      </PageHeader>

      <KindsStrip />

      <ProvidersGroup
        workspaceId={workspaceId}
        provider={provider}
      />
      <CustomSecretsGroup workspaceId={workspaceId} secrets={customSecrets} />
      <IntegrationsGroup secrets={customSecrets} />
    </div>
  );
}
