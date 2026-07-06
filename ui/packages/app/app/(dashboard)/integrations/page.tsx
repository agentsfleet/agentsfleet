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
import { ApiError } from "@/lib/api/errors";
import IntegrationsConnectors, {
  type ConnectorFetchError,
} from "./components/IntegrationsConnectors";

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
    const [catalogResult, githubConnector, slackConnector] = await Promise.all([
      // Capture the failure instead of swallowing it to []: an empty catalog is
      // rendered as "Couldn't load", and the code/status is what makes it
      // diagnosable (console logging is lint-banned in app source).
      getConnectorCatalog(workspaceId, token)
        .then((entries) => ({ entries, error: null as ConnectorFetchError | null }))
        .catch((err: unknown) => ({
          entries: [] as ConnectorCatalogEntry[],
          error:
            err instanceof ApiError
              ? { code: err.code, status: err.status }
              : // A thrown non-ApiError carries no HTTP status; null keeps the
                // rendered detail honest instead of a fabricated status 0.
                { code: "UZ-UNKNOWN", status: null },
        })),
      getConnector("github", workspaceId, token).catch(() => ({
        status: CONNECTOR_STATUS.notConnected,
      })),
      getConnector("slack", workspaceId, token).catch(() => ({
        status: CONNECTOR_STATUS.notConnected,
        team: null,
      })),
    ]);
    return {
      workspaceId,
      catalog: catalogResult.entries,
      catalogError: catalogResult.error,
      githubConnector,
      slackConnector,
    };
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
  const { workspaceId, catalog, catalogError, githubConnector, slackConnector } = result;

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
            catalogError={catalogError}
            githubStatus={githubConnector.status}
            slackStatus={slackConnector.status}
            slackTeam={slackConnector.team}
          />
        </section>
      </Section>
    </div>
  );
}
