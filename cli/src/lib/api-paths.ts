// All fleet-scoped paths are workspace-scoped. Identity (workspace_id,
// fleet_id, grant_id) goes in the URL path; query params are reserved
// for pagination (page, limit, cursor) and search.

export const WORKSPACES_PATH = "/v1/workspaces/";
export const WEBHOOKS_PATH = "/v1/webhooks/";

// Flat (non-workspace-scoped) routes the CLI hits directly. Centralised
// so the audit catches drift if a server-side rename ships without a
// CLI mirror.
export const HEALTHZ_PATH = "/healthz";
export const AUTH_SESSIONS_PATH = "/v1/auth/sessions";
export const WORKSPACES_COLLECTION_PATH = "/v1/workspaces";
export const TENANT_BILLING_PATH = "/v1/tenants/me/billing";
export const TENANT_PROVIDER_PATH = "/v1/tenants/me/provider";

// First-party Fleet template catalog — global (not workspace-scoped),
// metadata only. Mirrors the dashboard's `listFleetTemplates`. The
// SKILL.md/TRIGGER.md content is fetched server-side at snapshot time.
export const FLEET_BUNDLES_PATH = "/v1/fleets/bundles";

// Healthz body envelope — the server's `{status: "ok"}` response.
export const HEALTHZ_STATUS_OK = "ok";

const enc = (s: string): string => encodeURIComponent(s);

// Workspace-scoped fleet collection.
export const wsFleetsPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets`;

// Workspace-scoped single fleet.
export const wsFleetPath = (wsId: string, fleetId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}`;

// Workspace-scoped Fleet Bundle snapshot import (POST → content-addressed
// snapshot with bundle_id + parsed requirements). The server fetches and
// validates the source for `template`/`github` kinds.
export const wsFleetBundleSnapshotsPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/bundles/snapshots`;

// Workspace-scoped per-fleet chat messages (POST → 202 with event_id).
export const wsFleetMessagesPath = (wsId: string, fleetId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}/messages`;

// Workspace-scoped per-fleet event history.
export const wsFleetEventsPath = (wsId: string, fleetId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}/events`;

// Workspace-scoped per-fleet SSE live tail.
export const wsFleetEventsStreamPath = (wsId: string, fleetId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}/events/stream`;

// Workspace-scoped per-fleet durable-memory entries (read-only).
export const wsFleetMemoriesPath = (wsId: string, fleetId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}/memories`;

// Workspace-aggregate event history.
export const wsEventsPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/events`;

// Workspace-scoped credentials vault (workspace-level, not per-fleet).
export const wsCredentialsPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/credentials`;

export const wsCredentialPath = (wsId: string, name: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/credentials/${enc(name)}`;

// Workspace-scoped integration grant routes (per fleet).
export const wsGrantRequestPath = (wsId: string, fleetId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}/integration-requests`;

export const wsGrantsListPath = (wsId: string, fleetId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}/integration-grants`;

export const wsGrantPath = (
  wsId: string,
  fleetId: string,
  grantId: string,
): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}/integration-grants/${enc(grantId)}`;
