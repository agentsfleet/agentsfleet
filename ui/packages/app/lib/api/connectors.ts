import { request } from "./client";

// Connectors API client. The dashboard renders its cards from the registry-driven
// catalog (`getConnectorCatalog`) — the provider list, archetypes, display names,
// and (for api_key) the connect-form field schema all arrive from the backend;
// nothing here re-declares them. Two connectors — GitHub and Slack — additionally
// expose a bespoke per-provider status route (`getConnector`) whose shape is
// richer than the catalog's `connected` bool (reconnect state; Slack's connected
// team). Connecting is uniform: OAuth/app_install redirect via `startConnect` (no
// token paste), api_key probe-then-vault via `submitApiKeyConnect`. No secret ever
// crosses this API on a read.

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
  apiKey: "api_key",
} as const;
export type ConnectorArchetype = (typeof CONNECTOR_ARCHETYPE)[keyof typeof CONNECTOR_ARCHETYPE];

// One api_key input field, as declared by the backend registry. The dashboard's
// connect form renders these — which inputs a provider needs, and which are secret
// (masked) vs plain coordinates (site, instance_url). The app never hard-codes
// this; it arrives on the catalog entry.
export interface ApiKeyField {
  name: string;
  secret: boolean;
}

export interface ConnectorCatalogEntry {
  id: string;
  archetype: ConnectorArchetype;
  display_name: string;
  configured: boolean;
  connected: boolean;
  // The connect-form field schema for api_key connectors; empty for
  // oauth2/app_install (they connect by redirect, not a form).
  fields: readonly ApiKeyField[];
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

// api_key connect is an authed POST whose body carries the fields the catalog
// entry declared (see `ConnectorCatalogEntry.fields`). The handler runs a bounded
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
