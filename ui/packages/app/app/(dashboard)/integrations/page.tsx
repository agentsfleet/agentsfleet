import { redirect } from "next/navigation";
import { auth } from "@clerk/nextjs/server";
import {
  EmptyState,
  PageHeader,
  PageTitle,
  Section,
  SectionLabel,
} from "@agentsfleet/design-system";
import { LinkIcon } from "lucide-react";
import { withWorkspaceScope, orFallback } from "@/lib/workspace";
import { listCredentials } from "@/lib/api/credentials";
import { getConnector, CONNECTOR_STATUS } from "@/lib/api/connectors";
import IntegrationsConnectors from "./components/IntegrationsConnectors";

export const dynamic = "force-dynamic";

const PAGE_TITLE = "Integrations";
const PAGE_DESCRIPTION = "Connect the tools your fleets act through.";

export default async function IntegrationsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // One workspace-scoped pass: the connector status and the stored-secret names
  // (used to mark a planned connector "token stored") are the only two reads,
  // fetched together. A missing/unbuilt connector endpoint degrades to "not
  // connected" — the pill never fabricates a connected state.
  const result = await withWorkspaceScope(token, async (workspaceId) => {
    const [credentialsResp, githubConnector, slackConnector] = await Promise.all([
      listCredentials(workspaceId, token).catch(orFallback({ credentials: [] })),
      getConnector("github", workspaceId, token).catch(() => ({
        status: CONNECTOR_STATUS.notConnected,
      })),
      getConnector("slack", workspaceId, token).catch(() => ({
        status: CONNECTOR_STATUS.notConnected,
        team: null,
      })),
    ]);
    return { workspaceId, credentialsResp, githubConnector, slackConnector };
  });
  if (!result) {
    return (
      <div>
        <PageHeader description={PAGE_DESCRIPTION}>
          <PageTitle>{PAGE_TITLE}</PageTitle>
        </PageHeader>
        <EmptyState
          icon={<LinkIcon size={32} />}
          title="No workspace yet"
          description="Create a workspace first."
        />
      </div>
    );
  }
  const { workspaceId, credentialsResp, githubConnector, slackConnector } = result;

  return (
    <div className="space-y-8">
      <PageHeader description={PAGE_DESCRIPTION}>
        <PageTitle>{PAGE_TITLE}</PageTitle>
      </PageHeader>

      <Section asChild>
        <section aria-label="Integrations" data-testid="integrations-page">
          <SectionLabel>Connectors</SectionLabel>
          <IntegrationsConnectors
            workspaceId={workspaceId}
            githubStatus={githubConnector.status}
            slackStatus={slackConnector.status}
            slackTeam={slackConnector.team}
            credentialNames={credentialsResp.credentials.map((secret) => secret.name)}
          />
        </section>
      </Section>
    </div>
  );
}
