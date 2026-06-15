"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  listWorkspaceEvents as apiListWorkspaceEvents,
  listAgentEvents as apiListAgentEvents,
  type EventsPage,
  type EventsQuery,
} from "@/lib/api/events";

export async function listAgentEventsAction(
  workspaceId: string,
  agentId: string,
  opts?: Omit<EventsQuery, "agent_id">,
): Promise<ActionResult<EventsPage>> {
  return withToken((t) => apiListAgentEvents(workspaceId, agentId, t, opts));
}

export async function listWorkspaceEventsAction(
  workspaceId: string,
  opts?: EventsQuery,
): Promise<ActionResult<EventsPage>> {
  return withToken((t) => apiListWorkspaceEvents(workspaceId, t, opts));
}
