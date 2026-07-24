import { redirect } from "next/navigation";
import { PageHeader, PageLayout, PageTitle, Section, SectionHeader } from "@agentsfleet/design-system";
import { auth } from "@clerk/nextjs/server";
import { PROVIDER_MODE } from "@/lib/types";
import { getTenantProviderCached, listSecretsCached } from "./lib/reads";
import SecretsList from "./components/SecretsList";
import AddSecretDialog from "./components/AddSecretDialog";
import { SECRETS_PAGE_DESCRIPTION, SECRETS_PAGE_TITLE } from "./copy";

export const dynamic = "force-dynamic";

export default async function SecretsPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const [secretsResp, providerResult] = await Promise.all([
    listSecretsCached(workspaceId, token).catch(() => ({ secrets: [] })),
    getTenantProviderCached(token).catch((err) => ({ error: String(err) })),
  ]);
  const secrets = secretsResp.secrets;
  // The secret backing the active self-managed provider can't be deleted from
  // here — deleting it would strand the workspace's live model setup.
  const protectedSecretName =
    "error" in providerResult
      ? null
      : providerResult.mode === PROVIDER_MODE.self_managed
        ? providerResult.secret_ref
        : null;

  return (
    <PageLayout>
      <PageHeader description={SECRETS_PAGE_DESCRIPTION}>
        <PageTitle>{SECRETS_PAGE_TITLE}</PageTitle>
      </PageHeader>

      <Section asChild>
        <section aria-label="Secrets">
          <SectionHeader actions={<AddSecretDialog workspaceId={workspaceId} />}>
            Manage secrets
          </SectionHeader>

          <SecretsList
            workspaceId={workspaceId}
            secrets={secrets}
            protectedSecretName={protectedSecretName}
          />
        </section>
      </Section>
    </PageLayout>
  );
}
