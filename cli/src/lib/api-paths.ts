// All fleet-scoped paths are workspace-scoped. Identity (workspace_id,
// fleet_id, grant_id) goes in the URL path; query params are reserved
// for pagination (starting_after, limit) and search.

export const WORKSPACES_PATH = "/v1/workspaces/";

// Mirrors the daemon's QUERY_STARTING_AFTER (http/pagination.zig) — the
// keyset paging request parameter every cursor-paged list accepts.
export const QUERY_STARTING_AFTER = "starting_after";
// Mirrors Q_LIMIT / Q_PROVIDER in http/handlers/model_library.zig. `limit` is
// bounded 1..100 server-side; `provider` filters the catalogue page.
export const QUERY_LIMIT = "limit";
export const QUERY_PROVIDER = "provider";

// Flat (non-workspace-scoped) routes the CLI hits directly. Centralised
// so the audit catches drift if a server-side rename ships without a
// CLI mirror.
export const HEALTHZ_PATH = "/healthz";
export const AUTH_SESSIONS_PATH = "/v1/auth/sessions";
// Durable per-user credential minted by `login` from the recovered session
// token. Mirrors the daemon's S_CLI_CREDENTIALS (http/route_matchers.zig).
export const CLI_CREDENTIALS_PATH = "/v1/cli-credentials";
export const WORKSPACES_COLLECTION_PATH = "/v1/workspaces";
export const TENANT_API_KEYS_PATH = "/v1/api-keys";
export const TENANT_BILLING_PATH = "/v1/tenants/me/billing";
export const TENANT_PROVIDER_PATH = "/v1/tenants/me/provider";
// The priced model catalogue (core.model_library). Shared verbatim with
// MODEL_LIBRARY_PATH in http/handlers/model_library.zig and the dashboard's
// lib/api/model_library.ts. Backs `agentsfleet models` and the `--provider`
// check — the CLI carries no provider list of its own.
export const MODEL_LIBRARY_PATH = "/v1/models";
export const TENANT_WORKSPACES_PATH = "/v1/tenants/me/workspaces";
// Who this credential belongs to. Mirrors `TenantRoute::CurrentUser` in
// rustd/crates/afd_http/src/route/tenant.rs, and the one route on the tenant
// plane that requires no capability — which is why `login` and `auth status`
// both probe it rather than the billing snapshot they used to reach for.
export const USERS_ME_PATH = "/v1/users/me";

// Healthz body envelope — the server's `{status: "ok"}` response.
export const HEALTHZ_STATUS_OK = "ok";

const enc = (s: string): string => encodeURIComponent(s);

// Workspace-scoped fleet collection.
export const wsFleetsPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets`;

// Workspace-scoped single fleet.
export const wsFleetPath = (wsId: string, fleetId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}`;

// Workspace-scoped Fleet library gallery (GET → platform ∪ this workspace's
// tenant libraries, each carrying `visibility` + declared requirements). The
// install flow resolves `--library <id>` here, then keys the create body off
// the entry's tier (M103 §5).
export const wsFleetLibrariesPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleet-libraries`;

// Workspace-scoped OWNED entries (GET → only what this workspace onboarded,
// never a platform row and never another workspace's). A second collection
// rather than a filter on the gallery above: that one answers "what can I
// install here", this one "what did we onboard". Its cursor is its own and a
// gallery cursor cannot be spent against it.
export const wsLibraryEntriesPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/library-entries`;

// One owned entry (DELETE → 204, idempotent). An id already gone and one
// naming another workspace's entry answer identically, on purpose.
export const wsLibraryEntryPath = (wsId: string, entryId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/library-entries/${enc(entryId)}`;

// Workspace-scoped per-fleet chat messages (POST → 202 with event_id).
export const wsFleetMessagesPath = (wsId: string, fleetId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}/messages`;

// Workspace-scoped per-fleet event history.
export const wsFleetEventsPath = (wsId: string, fleetId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}/events`;

// Workspace-scoped per-fleet SSE live tail.
export const wsFleetEventsStreamPath = (
  wsId: string,
  fleetId: string,
): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}/events/stream`;

// Workspace-scoped per-fleet durable-memory entries (read-only).
export const wsFleetMemoriesPath = (wsId: string, fleetId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}/memories`;

export const wsFleetSchedulesPath = (wsId: string, fleetId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}/schedules`;

export const wsFleetSchedulePath = (
  wsId: string,
  fleetId: string,
  scheduleId: string,
): string => `${wsFleetSchedulesPath(wsId, fleetId)}/${enc(scheduleId)}`;

// `/sync` as its own segment. The daemon's router binds one parameter per path
// segment and refuses a literal after it, so the custom verb cannot ride the
// identifier — see `FleetRoute::ScheduleSync` in `rustd/crates/afd_api`.
export const wsFleetScheduleSyncPath = (
  wsId: string,
  fleetId: string,
  scheduleId: string,
): string => `${wsFleetSchedulePath(wsId, fleetId, scheduleId)}/sync`;

// Workspace-scoped secrets vault (workspace-level, not per-fleet).
export const wsSecretsPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/secrets`;

export const wsSecretPath = (wsId: string, name: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/secrets/${enc(name)}`;

export const tenantApiKeyPath = (keyId: string): string =>
  `${TENANT_API_KEYS_PATH}/${enc(keyId)}`;

export const wsConnectorsPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/connectors`;

export const wsConnectorPath = (wsId: string, provider: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/connectors/${enc(provider)}`;

// Workspace-scoped approval gates. The decision rides its own path segment
// rather than a colon suffix on the identifier: the daemon's router binds one
// parameter per segment, and reading a gate and deciding one carry different
// capabilities (`WorkspaceRoute::ApprovalRead` / `ApprovalResolve`).
export const wsApprovalsPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/approvals`;

export const wsApprovalPath = (wsId: string, gateId: string): string =>
  `${wsApprovalsPath(wsId)}/${enc(gateId)}`;

export const wsApprovalDecisionPath = (
  wsId: string,
  gateId: string,
  decision: string,
): string => `${wsApprovalPath(wsId, gateId)}/${enc(decision)}`;

// Workspace-scoped integration grant routes (per fleet).
export const wsGrantsListPath = (wsId: string, fleetId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}/integration-grants`;

export const wsGrantPath = (
  wsId: string,
  fleetId: string,
  grantId: string,
): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/fleets/${enc(fleetId)}/integration-grants/${enc(grantId)}`;
