"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  listWorkspaceEvents as apiListWorkspaceEvents,
  listFleetEvents as apiListFleetEvents,
  type EventsPage,
  type EventsQuery,
} from "@/lib/api/events";

export async function listFleetEventsAction(
  workspaceId: string,
  fleetId: string,
  opts?: Omit<EventsQuery, "fleet_id">,
): Promise<ActionResult<EventsPage>> {
  return withToken((t) => apiListFleetEvents(workspaceId, fleetId, t, opts));
}

export async function listWorkspaceEventsAction(
  workspaceId: string,
  opts?: EventsQuery,
): Promise<ActionResult<EventsPage>> {
  return withToken((t) => apiListWorkspaceEvents(workspaceId, t, opts));
}
