"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  deleteAgent as apiDeleteAgent,
  installAgent as apiInstallAgent,
  listAgents as apiListAgents,
  setAgentStatus as apiSetAgentStatus,
  steerAgent as apiSteerAgent,
  type AgentListResponse,
  type AgentStatusSettable,
  type AgentStatusUpdate,
} from "@/lib/api/agents";
import type { InstallAgentRequest, InstallAgentResponse } from "@/lib/types";

export async function listAgentsAction(
  workspaceId: string,
  opts?: { cursor?: string; limit?: number },
): Promise<ActionResult<AgentListResponse>> {
  return withToken((t) => apiListAgents(workspaceId, t, opts));
}

export async function setAgentStatusAction(
  workspaceId: string,
  agentId: string,
  status: AgentStatusSettable,
): Promise<ActionResult<AgentStatusUpdate>> {
  return withToken((t) => apiSetAgentStatus(workspaceId, agentId, status, t));
}

export async function deleteAgentAction(
  workspaceId: string,
  agentId: string,
): Promise<ActionResult<void>> {
  return withToken((t) => apiDeleteAgent(workspaceId, agentId, t));
}

export async function installAgentAction(
  workspaceId: string,
  body: InstallAgentRequest,
): Promise<ActionResult<InstallAgentResponse>> {
  return withToken((t) => apiInstallAgent(workspaceId, body, t));
}

// Submits a steer message server-side so the browser never holds the
// api-audience token. Retry runs inside `steerAgent` with its defaults —
// no client-visible per-attempt callback. The caller reconciles its
// optimistic frame against the returned event_id on success, or flips it
// to `failed` when `ok` is false.
export async function steerAgentAction(
  workspaceId: string,
  agentId: string,
  message: string,
): Promise<ActionResult<{ event_id: string }>> {
  return withToken((t) => apiSteerAgent(workspaceId, agentId, message, t));
}
