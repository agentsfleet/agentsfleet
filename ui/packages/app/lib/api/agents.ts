import { API_ORIGIN, request } from "./client";
import { requestWithRetry, type RetryOptions } from "./retry";
import { ApiError } from "./errors";
import type {
  InstallAgentRequest,
  InstallAgentResponse,
  Agent,
  AgentListResponse,
} from "../types";

export type { Agent, AgentListResponse };

export async function listAgents(
  workspaceId: string,
  token: string,
  opts?: { cursor?: string; limit?: number },
): Promise<AgentListResponse> {
  const params = new URLSearchParams();
  if (opts?.cursor) params.set("cursor", opts.cursor);
  if (opts?.limit != null) params.set("limit", String(opts.limit));
  const qs = params.toString();
  const path = qs
    ? `/v1/workspaces/${workspaceId}/agents?${qs}`
    : `/v1/workspaces/${workspaceId}/agents`;
  return request<AgentListResponse>(path, { method: "GET" }, token);
}

// Single-agent lookup. Filters the list response until a dedicated
// GET /v1/workspaces/{ws}/agents/{id} endpoint ships. Requests the
// server max (100) since we cannot target a specific id without that
// endpoint — workspaces above that size will miss agents on later pages.
export async function getAgent(
  workspaceId: string,
  agentId: string,
  token: string,
): Promise<Agent | null> {
  const page = await listAgents(workspaceId, token, { limit: 100 });
  const hit = page.items.find((z) => z.id === agentId);
  if (hit) return hit;
  // `cursor` non-null means the workspace has more agents than we scanned.
  // Surface this as a distinct error instead of a silent null → 404 so
  // operators aren't left staring at "not found" for a agent that exists.
  if (page.cursor) {
    throw new ApiError(
      `Agent ${agentId} is not in the first 100 agents for this workspace. This workspace has more agents than the client-side scan can cover; a dedicated GET /agents/{id} endpoint is required for reliable lookup at this scale.`,
      404,
      "UZ-AGT-SCAN-CAP",
    );
  }
  return null;
}

export async function installAgent(
  workspaceId: string,
  body: InstallAgentRequest,
  token: string,
): Promise<InstallAgentResponse> {
  return request<InstallAgentResponse>(
    `/v1/workspaces/${workspaceId}/agents`,
    { method: "POST", body: JSON.stringify(body) },
    token,
  );
}

// Every agent status the API can return. Source of truth — every consumer
// that switches/compares against a status value reads from this const. Mirrors
// the backend `AgentStatus` enum in src/agent/config_types.zig.
export const AGENTSFLEET_STATUS = {
  ACTIVE: "active",
  PAUSED: "paused",
  STOPPED: "stopped",
  KILLED: "killed",
} as const;
export type AgentStatus = typeof AGENTSFLEET_STATUS[keyof typeof AGENTSFLEET_STATUS];

// Subset PATCH /v1/workspaces/{ws}/agents/{id} accepts. `paused` is a gate-set
// state — the API never lets callers transition to it. Throws ApiError
// UZ-AGT-010 on 409 (transition not allowed from current state, e.g. resume on
// an active agent) and UZ-AGT-009 on 404 (agent missing or already-killed
// tombstone).
export type AgentStatusSettable = "active" | "stopped" | "killed";

// PATCH response. The handler echoes the new status only when the request set
// one (src/http/handlers/agents/patch.zig); `setAgentStatus` always sends a
// status, so it always comes back. `config_revision` is the post-write revision.
export interface AgentStatusUpdate {
  agent_id: string;
  status: AgentStatus;
  config_revision: number;
}

export async function setAgentStatus(
  workspaceId: string,
  agentId: string,
  status: AgentStatusSettable,
  token: string,
): Promise<AgentStatusUpdate> {
  return request<AgentStatusUpdate>(
    `/v1/workspaces/${workspaceId}/agents/${agentId}`,
    { method: "PATCH", body: JSON.stringify({ status }) },
    token,
  );
}

// Convenience wrappers — the dashboard's three lifecycle buttons.
export const stopAgent = (workspaceId: string, agentId: string, token: string) =>
  setAgentStatus(workspaceId, agentId, "stopped", token);
export const resumeAgent = (workspaceId: string, agentId: string, token: string) =>
  setAgentStatus(workspaceId, agentId, "active", token);
export const killAgent = (workspaceId: string, agentId: string, token: string) =>
  setAgentStatus(workspaceId, agentId, "killed", token);

// DELETE /v1/workspaces/{ws}/agents/{id}
// Hard-purge. Precondition: status='killed'. Throws UZ-AGT-010 (409) if not
// killed yet, UZ-AGT-009 (404) if agent missing.
export async function deleteAgent(
  workspaceId: string,
  agentId: string,
  token: string,
): Promise<void> {
  return request<void>(
    `/v1/workspaces/${workspaceId}/agents/${agentId}`,
    { method: "DELETE" },
    token,
  );
}

// Builds the per-source webhook URL the server returns in
// `webhook_urls` on install (`src/http/handlers/agents/create.zig`
// populateWebhookUrls). When `source` is omitted the legacy
// no-source path is returned — the M68 fallback panel still uses it.
export function webhookUrlFor(agentId: string, source?: string): string {
  return source
    ? `${API_ORIGIN}/v1/webhooks/${agentId}/${source}`
    : `${API_ORIGIN}/v1/webhooks/${agentId}`;
}

// POST /v1/workspaces/{ws}/agents/{id}/messages
// Submits a steer message — the user's natural-language nudge during a
// running stage or to start a new one. Returns the synthesized event_id
// so the caller can reconcile its optimistic UI frame against the live
// SSE stream's matching EVENT_RECEIVED.
export async function steerAgent(
  workspaceId: string,
  agentId: string,
  message: string,
  token: string,
  retry?: RetryOptions,
): Promise<{ event_id: string }> {
  return requestWithRetry<{ event_id: string }>(
    `/v1/workspaces/${workspaceId}/agents/${agentId}/messages`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ message }),
    },
    token,
    retry,
  );
}
