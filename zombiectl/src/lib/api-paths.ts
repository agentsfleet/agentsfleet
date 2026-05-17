// All zombie-scoped paths are workspace-scoped. Identity (workspace_id,
// zombie_id, grant_id) goes in the URL path; query params are reserved
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

// Healthz body envelope — the server's `{status: "ok"}` response.
export const HEALTHZ_STATUS_OK = "ok";

const enc = (s: string): string => encodeURIComponent(s);

// Workspace-scoped zombie collection.
export const wsZombiesPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/zombies`;

// Workspace-scoped single zombie.
export const wsZombiePath = (wsId: string, zombieId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/zombies/${enc(zombieId)}`;

// Workspace-scoped per-zombie chat messages (POST → 202 with event_id).
export const wsZombieMessagesPath = (wsId: string, zombieId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/zombies/${enc(zombieId)}/messages`;

// Workspace-scoped per-zombie event history.
export const wsZombieEventsPath = (wsId: string, zombieId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/zombies/${enc(zombieId)}/events`;

// Workspace-scoped per-zombie SSE live tail.
export const wsZombieEventsStreamPath = (wsId: string, zombieId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/zombies/${enc(zombieId)}/events/stream`;

// Workspace-aggregate event history.
export const wsEventsPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/events`;

// Workspace-scoped credentials vault (workspace-level, not per-zombie).
export const wsCredentialsPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/credentials`;

export const wsCredentialPath = (wsId: string, name: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/credentials/${enc(name)}`;

// Workspace-scoped integration grant routes (per zombie).
export const wsGrantRequestPath = (wsId: string, zombieId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/zombies/${enc(zombieId)}/integration-requests`;

export const wsGrantsListPath = (wsId: string, zombieId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/zombies/${enc(zombieId)}/integration-grants`;

export const wsGrantPath = (
  wsId: string,
  zombieId: string,
  grantId: string,
): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/zombies/${enc(zombieId)}/integration-grants/${enc(grantId)}`;
