import { request } from "./client";

// Connectors API client. The dashboard renders its cards from the registry-driven
// catalog (`getConnectorCatalog`) — the provider list, archetypes, and display
// names all arrive from the backend; nothing here re-declares them. All connectors
// are OAuth/app_install: connecting is a redirect round-trip via `startConnect`, no
// token paste. (Static vendor keys — Datadog/Grafana/Fly — are workspace secrets
// referenced as `${secrets.<name>.<field>}`, rather than through this list.)
// One workspace response includes every provider's connection status and label.
// Reads return public metadata only.

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

export async function disconnectConnector(
  provider: string,
  workspaceId: string,
  token: string,
): Promise<void> {
  return request<void>(
    connectorPath(provider, workspaceId),
    { method: "DELETE" },
    token,
  );
}

// Registry-driven catalog.
// The dashboard renders its connector cards from this, never a hard-coded list:
// `GET /v1/workspaces/{ws}/connectors` returns one entry per registry provider —
// the collection whose items are the per-provider status routes. `configured` is
// platform-side (a `<provider>-app` bag exists); `connected` is this workspace's handle.

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
  label: string | null;
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
