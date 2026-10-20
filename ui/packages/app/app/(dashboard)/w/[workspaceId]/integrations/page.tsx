import {
  PageHeader,
  PageLayout,
  PageTitle,
} from "@agentsfleet/design-system";
import { requireCredential } from "@/lib/auth/credential";
import {
  getConnector,
  getConnectorCatalog,
  type ConnectorCatalogEntry,
} from "@/lib/api/connectors";
import { ApiError } from "@/lib/api/errors";
import IntegrationsConnectors, {
  type ConnectorFetchError,
} from "./components/IntegrationsConnectors";
import { CONNECT_APPS_COPY } from "./copy";

export const dynamic = "force-dynamic";

export default async function IntegrationsPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  const token = await requireCredential();

  // The registry-driven catalog (the card list) plus the two connectors with a
  // bespoke status route (GitHub/Slack tri-state + the Slack team), fetched
  // together. Failed status reads reach the retry boundary instead of claiming
  // a connection has disappeared. The workspace comes
  // from the URL; the backend re-authorizes it (`ownsWithinTenant`) per call.
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
    getConnector("github", workspaceId, token),
    getConnector("slack", workspaceId, token),
  ]);
  const catalog = catalogResult.entries;
  const catalogError = catalogResult.error;

  return (
    <PageLayout>
      <PageHeader description={CONNECT_APPS_COPY.description}>
        <PageTitle>{CONNECT_APPS_COPY.title}</PageTitle>
      </PageHeader>

          <IntegrationsConnectors
            workspaceId={workspaceId}
            catalog={catalog}
            catalogError={catalogError}
            githubStatus={githubConnector.status}
            slackStatus={slackConnector.status}
            slackTeam={slackConnector.team}
          />
    </PageLayout>
  );
}
