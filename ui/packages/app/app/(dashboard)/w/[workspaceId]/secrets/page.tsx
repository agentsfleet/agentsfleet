import { PageHeader, PageLayout, PageTitle, Section, SectionHeader } from "@agentsfleet/design-system";
import { requireCredential } from "@/lib/auth/credential";
import { PROVIDER_MODE } from "@/lib/types";
import { getConnectorCatalogCached, getTenantProviderCached, listSecretsCached } from "./lib/reads";
import SecretsList from "./components/SecretsList";
import AddSecretDialog from "./components/AddSecretDialog";
import { SECRETS_PAGE_TITLE } from "./copy";
import { SecretsDescription } from "./components/SecretsDescription";

export const dynamic = "force-dynamic";

export default async function SecretsPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  const token = await requireCredential();

  const [secretsResp, providerResult, catalog] = await Promise.all([
    listSecretsCached(workspaceId, token),
    getTenantProviderCached(token),
    // Only the delete dialog's wording rides on this read, so a failed one
    // costs the warning rather than the page. The server disconnects a
    // connection's secret on delete either way.
    getConnectorCatalogCached(workspaceId, token).catch(() => []),
  ]);
  const secrets = secretsResp.secrets;
  // The secret backing the active self-managed provider can't be deleted from
  // here — deleting it would strand the workspace's live model setup.
  const protectedSecretName =
    providerResult.mode === PROVIDER_MODE.self_managed
        ? providerResult.secret_ref
        : null;
  // A connected connector's grant secret is named for its catalog id, and
  // deleting that secret disconnects it — so the dialog must say so.
  const connections = Object.fromEntries(
    catalog.filter((entry) => entry.connected).map((entry) => [entry.id, entry.display_name]),
  );

  return (
    <PageLayout fullHeight className="h-full overflow-hidden">
      <PageHeader description={<SecretsDescription />}>
        <PageTitle>{SECRETS_PAGE_TITLE}</PageTitle>
      </PageHeader>

      <Section aria-label="Stored secrets" className="flex min-h-0 flex-col gap-xl">
        <SectionHeader as="p" actions={<AddSecretDialog workspaceId={workspaceId} />}>Stored secrets</SectionHeader>
        <SecretsList
            workspaceId={workspaceId}
            secrets={secrets}
            protectedSecretName={protectedSecretName}
            connections={connections}
        />
      </Section>
    </PageLayout>
  );
}
