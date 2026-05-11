"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  listWorkspaceEvents as apiListWorkspaceEvents,
  listZombieEvents as apiListZombieEvents,
  type EventsPage,
  type EventsQuery,
} from "@/lib/api/events";

export async function listZombieEventsAction(
  workspaceId: string,
  zombieId: string,
  opts?: Omit<EventsQuery, "zombie_id">,
): Promise<ActionResult<EventsPage>> {
  return withToken((t) => apiListZombieEvents(workspaceId, zombieId, t, opts));
}

export async function listWorkspaceEventsAction(
  workspaceId: string,
  opts?: EventsQuery,
): Promise<ActionResult<EventsPage>> {
  return withToken((t) => apiListWorkspaceEvents(workspaceId, t, opts));
}
