import { redirect } from "next/navigation";
import { EmptyState, PageHeader, PageTitle } from "@agentsfleet/design-system";
import { ZapIcon } from "lucide-react";
import { auth } from "@clerk/nextjs/server";
import { withWorkspaceScope, orFallback } from "@/lib/workspace";
import { PROVIDER_MODE } from "@/lib/types";
import { getTenantProviderCached, listSecretsCached } from "./lib/reads";
import SecretsList from "./components/SecretsList";
import AddSecretDialog from "./components/AddSecretDialog";

export const dynamic = "force-dynamic";

const PAGE_TITLE = "Secrets & ENVs";
const PAGE_DESCRIPTION = "Encrypted secrets your fleets can use — write-only once saved.";

export default async function SecretsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const result = await withWorkspaceScope(token, async (workspaceId) => {
    const [secretsResp, providerResult] = await Promise.all([
      listSecretsCached(workspaceId, token).catch(orFallback({ secrets: [] })),
      getTenantProviderCached(token).catch((err) => ({ error: String(err) })),
    ]);
    return { workspaceId, secrets: secretsResp.secrets, providerResult };
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
  const { workspaceId, secrets, providerResult } = result;
  // The secret backing the active self-managed provider can't be deleted from
  // here — deleting it would strand the workspace's live model setup.
  const protectedSecretName =
    "error" in providerResult
      ? null
      : providerResult.mode === PROVIDER_MODE.self_managed
        ? providerResult.secret_ref
        : null;

  return (
    <div className="space-y-8">
      <PageHeader description={PAGE_DESCRIPTION}>
        <PageTitle>{PAGE_TITLE}</PageTitle>
      </PageHeader>

      <div className="flex justify-end">
        <AddSecretDialog workspaceId={workspaceId} />
      </div>

      <SecretsList
        workspaceId={workspaceId}
        secrets={secrets}
        protectedSecretName={protectedSecretName}
      />
    </div>
  );
}
