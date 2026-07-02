import { request } from "./client";

// Browser-OAuth connectors (GitHub App install, Slack OAuth). Connecting is always
// a redirect round-trip — never a token paste: the backend callback vaults the
// credential handle (`fleet:<provider>`) and the broker mints short-lived tokens
// from it on demand. Reads here are status only (no secret ever crosses this API).
//
// GitHub and Slack differ only in their status shape (Slack surfaces the connected
// team), so the client is one provider-parameterised pair — `getConnector` /
// `startConnect` — with `ConnectorStateByProvider` keeping call sites exactly typed
// off the provider argument. A third connector is a one-line map entry, not a new
// function pair.

export const CONNECTOR_STATUS = {
  connected: "connected",
  reconnectRequired: "reconnect_required",
  notConnected: "not_connected",
} as const;
export type ConnectorStatus = (typeof CONNECTOR_STATUS)[keyof typeof CONNECTOR_STATUS];

export const CONNECTOR_PROVIDERS = {
  github: "github",
  slack: "slack",
} as const;
export type ConnectorProvider = (typeof CONNECTOR_PROVIDERS)[keyof typeof CONNECTOR_PROVIDERS];

export interface GithubConnectorState {
  status: ConnectorStatus;
}

export interface SlackConnectorState {
  status: ConnectorStatus;
  // The Slack workspace name (surfaced as "Slack connected: {team}"), null when
  // not connected.
  team: string | null;
}

// Per-provider status shape. Indexing by the provider argument gives each caller
// the exact state type (GitHub has no `team`; Slack does) with no narrowing.
export interface ConnectorStateByProvider {
  github: GithubConnectorState;
  slack: SlackConnectorState;
}

export interface ConnectorConnectStart {
  // The provider authorize/install URL the browser is redirected to. Carries the
  // signed `state` that binds this workspace and guards CSRF at the callback.
  // Same shape for every provider.
  install_url: string;
}

const connectorPath = (provider: ConnectorProvider, workspaceId: string): string =>
  `/v1/workspaces/${encodeURIComponent(workspaceId)}/connectors/${provider}`;

export async function getConnector<P extends ConnectorProvider>(
  provider: P,
  workspaceId: string,
  token: string,
): Promise<ConnectorStateByProvider[P]> {
  return request<ConnectorStateByProvider[P]>(
    connectorPath(provider, workspaceId),
    { method: "GET" },
    token,
  );
}

export async function startConnect(
  provider: ConnectorProvider,
  workspaceId: string,
  token: string,
): Promise<ConnectorConnectStart> {
  return request<ConnectorConnectStart>(
    `${connectorPath(provider, workspaceId)}/connect`,
    { method: "POST" },
    token,
  );
}
