"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  listWorkspaceEvents as apiListWorkspaceEvents,
  type EventsPage,
  type EventsQuery,
} from "@/lib/api/events";

export async function listWorkspaceEventsAction(
  workspaceId: string,
  opts?: EventsQuery,
): Promise<ActionResult<EventsPage>> {
  return withToken((t) => apiListWorkspaceEvents(workspaceId, t, opts));
}
