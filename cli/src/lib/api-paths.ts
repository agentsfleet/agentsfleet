// All agent-scoped paths are workspace-scoped. Identity (workspace_id,
// agent_id, grant_id) goes in the URL path; query params are reserved
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

// Workspace-scoped agent collection.
export const wsAgentsPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/agents`;

// Workspace-scoped single agent.
export const wsAgentPath = (wsId: string, agentId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/agents/${enc(agentId)}`;

// Workspace-scoped per-agent chat messages (POST → 202 with event_id).
export const wsAgentMessagesPath = (wsId: string, agentId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/agents/${enc(agentId)}/messages`;

// Workspace-scoped per-agent event history.
export const wsAgentEventsPath = (wsId: string, agentId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/agents/${enc(agentId)}/events`;

// Workspace-scoped per-agent SSE live tail.
export const wsAgentEventsStreamPath = (wsId: string, agentId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/agents/${enc(agentId)}/events/stream`;

// Workspace-scoped per-agent durable-memory entries (read-only).
export const wsAgentMemoriesPath = (wsId: string, agentId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/agents/${enc(agentId)}/memories`;

// Workspace-aggregate event history.
export const wsEventsPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/events`;

// Workspace-scoped credentials vault (workspace-level, not per-agent).
export const wsCredentialsPath = (wsId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/credentials`;

export const wsCredentialPath = (wsId: string, name: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/credentials/${enc(name)}`;

// Workspace-scoped integration grant routes (per agent).
export const wsGrantRequestPath = (wsId: string, agentId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/agents/${enc(agentId)}/integration-requests`;

export const wsGrantsListPath = (wsId: string, agentId: string): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/agents/${enc(agentId)}/integration-grants`;

export const wsGrantPath = (
  wsId: string,
  agentId: string,
  grantId: string,
): string =>
  `${WORKSPACES_PATH}${enc(wsId)}/agents/${enc(agentId)}/integration-grants/${enc(grantId)}`;
