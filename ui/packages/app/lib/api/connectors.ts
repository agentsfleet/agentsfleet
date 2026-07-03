import { request } from "./client";

// Connectors API client. The dashboard renders its cards from the registry-driven
// catalog (`getConnectorCatalog`) — the provider list, archetypes, and display
// names all arrive from the backend; nothing here re-declares them. All connectors
// are OAuth/app_install: connecting is a redirect round-trip via `startConnect`, no
// token paste. (Static vendor keys — Datadog/Grafana/Fly — are workspace secrets
// referenced as `${secrets.<name>.<field>}`, not connectors.) Two connectors —
// GitHub and Slack — additionally expose a bespoke per-provider status route
// (`getConnector`) richer than the catalog's `connected` bool (reconnect state;
// Slack's connected team). No secret ever crosses this API on a read.

export const CONNECTOR_STATUS = {
  connected: "connected",
  reconnectRequired: "reconnect_required",
  notConnected: "not_connected",
} as const;
export type ConnectorStatus = (typeof CONNECTOR_STATUS)[keyof typeof CONNECTOR_STATUS];

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

// The providers with a bespoke per-provider status read (GitHub, Slack). Every
// other provider derives its card status from the catalog's `connected` flag
// rather than a dedicated status route.
export type ConnectorStatusProvider = keyof ConnectorStateByProvider;

export interface ConnectorConnectStart {
  // The provider authorize/install URL the browser is redirected to. Carries the
  // signed `state` that binds this workspace and guards CSRF at the callback.
  // Same shape for every provider.
  install_url: string;
}

// `provider` is a registry id sourced from the catalog at runtime; encode it as a
// single path segment. The server action that reaches here re-validates the id's
// shape, and the backend is the authority on which providers exist.
const connectorPath = (provider: string, workspaceId: string): string =>
  `/v1/workspaces/${encodeURIComponent(workspaceId)}/connectors/${encodeURIComponent(provider)}`;

export async function getConnector<P extends ConnectorStatusProvider>(
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
  provider: string,
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
} as const;
export type ConnectorArchetype = (typeof CONNECTOR_ARCHETYPE)[keyof typeof CONNECTOR_ARCHETYPE];

export interface ConnectorCatalogEntry {
  id: string;
  archetype: ConnectorArchetype;
  display_name: string;
  configured: boolean;
  connected: boolean;
}

// The docs anchor an unconfigured OAuth connector's card links to. The backend
// reports `configured:false` for the same condition it raises 503 UZ-CONN-001 on
// (a missing `<provider>-app` platform bag); the catalog carries no error body to
// read `docs_uri` from, so the one deep link lives here. Not a provider list — a
// single documentation pointer.
export const CONNECTOR_NOT_CONFIGURED_DOCS_URI =
  "https://docs.agentsfleet.net/api-reference/error-codes#UZ-CONN-001";

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

