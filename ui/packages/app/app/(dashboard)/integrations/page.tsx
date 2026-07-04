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
import { withWorkspaceScope } from "@/lib/workspace";
import {
  getConnector,
  getConnectorCatalog,
  CONNECTOR_STATUS,
  type ConnectorCatalogEntry,
} from "@/lib/api/connectors";
import IntegrationsConnectors from "./components/IntegrationsConnectors";

export const dynamic = "force-dynamic";

const PAGE_TITLE = "Integrations";
const PAGE_DESCRIPTION = "Connect the tools your fleets act through.";

export default async function IntegrationsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // One workspace-scoped pass: the registry-driven catalog (the card list) plus
  // the two connectors with a bespoke status route (GitHub/Slack tri-state + the
  // Slack team), fetched together. A missing/unbuilt endpoint degrades closed — an
  // empty catalog or "not connected" — never fabricating a connected state.
  const result = await withWorkspaceScope(token, async (workspaceId) => {
    const [catalog, githubConnector, slackConnector] = await Promise.all([
      getConnectorCatalog(workspaceId, token).catch(() => [] as ConnectorCatalogEntry[]),
      getConnector("github", workspaceId, token).catch(() => ({
        status: CONNECTOR_STATUS.notConnected,
      })),
      getConnector("slack", workspaceId, token).catch(() => ({
        status: CONNECTOR_STATUS.notConnected,
        team: null,
      })),
    ]);
    return { workspaceId, catalog, githubConnector, slackConnector };
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
  const { workspaceId, catalog, githubConnector, slackConnector } = result;

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
            catalog={catalog}
            githubStatus={githubConnector.status}
            slackStatus={slackConnector.status}
            slackTeam={slackConnector.team}
          />
        </section>
      </Section>
    </div>
  );
}
