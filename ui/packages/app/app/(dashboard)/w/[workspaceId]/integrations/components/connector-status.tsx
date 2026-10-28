import { requireCredential } from "@/lib/auth/credential";
import { getConnectorCatalog, type ConnectorCatalogEntry } from "@/lib/api/connectors";
import { ApiError } from "@/lib/api/errors";
import IntegrationsConnectors, { type ConnectorFetchError } from "./IntegrationsConnectors";

// One workspace-scoped response supplies the cards, status and connection labels.
export async function ConnectorStatus({ workspaceId }: { workspaceId: string }) {
  const token = await requireCredential();
  let catalog: ConnectorCatalogEntry[];
  let catalogError: ConnectorFetchError | null = null;
  try {
    catalog = await getConnectorCatalog(workspaceId, token);
  } catch (error) {
    catalog = [];
    catalogError = error instanceof ApiError
      ? { code: error.code, status: error.status }
      : { code: "UZ-UNKNOWN", status: null };
  }
  return <IntegrationsConnectors workspaceId={workspaceId} catalog={catalog} catalogError={catalogError} />;
}
