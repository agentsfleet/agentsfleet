import { redirect } from "next/navigation";
import Link from "next/link";
import { auth } from "@clerk/nextjs/server";
import {
  Button,
  EmptyState,
  PageHeader,
  PageTitle,
  Section,
  SectionLabel,
} from "@agentsfleet/design-system";
import { KeyRoundIcon } from "lucide-react";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { getTenantProvider } from "@/lib/api/tenant_provider";
import { listCredentials, type CredentialSummary } from "@/lib/api/credentials";
import { VAULT_KIND, VAULT_KINDS } from "./lib/vault-kinds";
import AddCredentialForm from "./components/AddCredentialForm";
import CredentialsList from "./components/CredentialsList";
import CustomSecretsList from "./components/CustomSecretsList";
import IntegrationsComingSoon from "./components/IntegrationsComingSoon";

export const dynamic = "force-dynamic";

const PAGE_TITLE = "Credentials";
const PAGE_DESCRIPTION =
  "Your write-only secret vault. Model keys, integration tokens, and your own custom secrets — each stored once and resolved by name at runtime.";
const ADD_CREDENTIAL_LABEL = "Add credential";

function KindsStrip() {
  return (
    <div className="grid gap-3 sm:grid-cols-3" data-testid="vault-kinds-strip">
      {VAULT_KINDS.map((kind) => (
        <div
          key={kind.kind}
          data-testid={`vault-kind-${kind.kind}`}
          className="rounded-md border border-border bg-card px-4 py-3"
        >
          <div className="font-medium text-foreground">{kind.label}</div>
          <p className="mt-1 text-xs text-muted-foreground">{kind.blurb}</p>
          <p className="mt-2 font-mono text-xs text-text-subtle">{kind.examples}</p>
        </div>
      ))}
    </div>
  );
}

function ProvidersGroup({
  workspaceId,
  providerCredentials,
  activeModelRef,
}: {
  workspaceId: string;
  providerCredentials: CredentialSummary[];
  activeModelRef: string | null;
}) {
  return (
    <Section asChild>
      <section aria-label="Model providers" data-testid={`group-${VAULT_KIND.providers}`}>
        <SectionLabel>Model providers</SectionLabel>
        {providerCredentials.length === 0 ? (
          <EmptyState
            icon={<KeyRoundIcon size={28} />}
            title="No model-provider key in use"
            description="Switch a teammate to own-key model setup to store a provider key here."
          />
        ) : (
          <CredentialsList
            workspaceId={workspaceId}
            credentials={providerCredentials}
            protectedCredentialName={activeModelRef}
          />
        )}
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
        <CustomSecretsList workspaceId={workspaceId} secrets={secrets} />
        <details className="rounded-md border border-dashed border-border bg-card">
          <summary className="cursor-pointer px-4 py-3">
            <span className="block font-medium text-foreground">Add a custom secret</span>
            <span className="block text-xs text-muted-foreground">
              An arbitrary <code className="font-mono">NAME=value</code> your SKILL.md reads by name.
            </span>
          </summary>
          <div className="border-t border-border p-4">
            <AddCredentialForm workspaceId={workspaceId} />
          </div>
        </details>
      </section>
    </Section>
  );
}

function IntegrationsGroup() {
  return (
    <Section asChild>
      <section aria-label="Integrations" data-testid={`group-${VAULT_KIND.integrations}`}>
        <SectionLabel>Integrations</SectionLabel>
        <IntegrationsComingSoon />
      </section>
    </Section>
  );
}

export default async function CredentialsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) {
    return (
      <div>
        <PageHeader description={PAGE_DESCRIPTION}>
          <PageTitle>{PAGE_TITLE}</PageTitle>
        </PageHeader>
        <EmptyState
          icon={<KeyRoundIcon size={32} />}
          title="No workspace yet"
          description="Create a workspace before storing credentials."
        />
      </div>
    );
  }

  const [providerResult, credentialsResp] = await Promise.all([
    getTenantProvider(token).catch((err) => ({ error: String(err) })),
    listCredentials(workspace.id, token).catch(() => ({ credentials: [] })),
  ]);
  // The only KNOWN reference is the active model credential; a provider-fetch
  // error simply means we surface no referenced-by hint (never fabricated).
  const activeModelRef =
    "error" in providerResult ? null : providerResult.credential_ref;

  const credentials = credentialsResp.credentials;
  // Best-effort split: a stored credential the active model setup points at is a
  // model-provider key; everything else is a custom secret. No usage graph is
  // synthesized beyond this one known reference.
  const providerCredentials = credentials.filter((c) => c.name === activeModelRef);
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
        workspaceId={workspace.id}
        providerCredentials={providerCredentials}
        activeModelRef={activeModelRef}
      />
      <div id="add-custom-secret">
        <CustomSecretsGroup workspaceId={workspace.id} secrets={customSecrets} />
      </div>
      <IntegrationsGroup />
    </div>
  );
}
