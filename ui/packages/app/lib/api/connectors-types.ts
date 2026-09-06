// The connector status tags and the docs link the integration rows share. Dependency-free on purpose: client components read these
// without pulling the transport, whose retry policy is server-only.

export const CONNECTOR_STATUS = {
  connected: "connected",
  reconnectRequired: "reconnect_required",
  notConnected: "not_connected",
} as const;

export type ConnectorStatus = (typeof CONNECTOR_STATUS)[keyof typeof CONNECTOR_STATUS];

// The docs anchor an unconfigured OAuth connector's card links to. The backend
// reports `configured:false` for the same condition it raises 503 UZ-CONN-001 on
// (a missing `<provider>-app` platform bag); the catalog carries no error body to
// read `docs_uri` from, so the one deep link lives here. Not a provider list — a
// single documentation pointer.
export const CONNECTOR_NOT_CONFIGURED_DOCS_URI =
  "https://docs.agentsfleet.net/api-reference/error-codes#UZ-CONN-001";
