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

// ── Registry-driven catalog (M108) ───────────────────────────────────────────
// The dashboard renders its connector cards from this, never a hard-coded list:
// `GET /v1/workspaces/{ws}/connectors` returns one entry per registry provider —
// the collection whose items are the per-provider status routes. `configured` is
// platform-side (an oauth2/app_install `<provider>-app` bag exists; api_key
// self-provisions → always true); `connected` is this workspace's handle.

export const CONNECTOR_ARCHETYPE = {
  oauth2: "oauth2",
  appInstall: "app_install",
  apiKey: "api_key",
} as const;
export type ConnectorArchetype = (typeof CONNECTOR_ARCHETYPE)[keyof typeof CONNECTOR_ARCHETYPE];

export interface ConnectorCatalogEntry {
  id: string;
  archetype: ConnectorArchetype;
  display_name: string;
  configured: boolean;
  connected: boolean;
}

export async function getConnectorCatalog(
  workspaceId: string,
  token: string,
): Promise<ConnectorCatalogEntry[]> {
  return request<ConnectorCatalogEntry[]>(
    `/v1/workspaces/${encodeURIComponent(workspaceId)}/connectors`,
    { method: "GET" },
    token,
  );
}

// api_key connect is an authed POST whose body carries the archetype's declared
// fields (Datadog `{api_key, app_key, site}`, Grafana `{instance_url,
// service_account_token}`, Fly `{org_token}`). The handler runs a bounded
// validation probe before vaulting; a bad key → 400 `UZ-CONN-005`, no write. The
// submitted secrets travel only in this request body — never echoed back.
export interface ApiKeyConnectResult {
  status: ConnectorStatus;
}

export async function submitApiKeyConnect(
  provider: string,
  workspaceId: string,
  fields: Record<string, string>,
  token: string,
): Promise<ApiKeyConnectResult> {
  return request<ApiKeyConnectResult>(
    `/v1/workspaces/${encodeURIComponent(workspaceId)}/connectors/${encodeURIComponent(provider)}/connect`,
    { method: "POST", body: JSON.stringify(fields) },
    token,
  );
}
