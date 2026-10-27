import { z } from "zod";
import { Instant } from "./decode";
import { setupRequest } from "./draft-request";
import { FLEET_ACTIVITY } from "../events/fleet-activity";
import type { Fleet } from "./draft-types";
import type { DraftView } from "./draft-state";

const ActivityRowSchema = z.object({
  event_id: z.string(), event_type: z.string(), status: z.string(),
  request_json: z.string(), created_at: Instant,
});
const PageSchema = z.object({items: z.array(ActivityRowSchema), next_cursor: z.string().nullable()});
const ACTIVITY_PAGE_LIMIT = 25;
export type SetupActivity = z.infer<typeof ActivityRowSchema>;

/** Read the same durable history as fleet chat, including safe unknown activity kinds. */
export async function readSetupActivity(base: string, fleet: string, signal: AbortSignal): Promise<SetupActivity[]> {
  const page = await setupRequest({path: `${base}/fleets/${fleet}/messages?limit=${ACTIVITY_PAGE_LIMIT}`}, PageSchema, signal);
  return page.items.filter(row => row.event_type === FLEET_ACTIVITY.type && row.status === FLEET_ACTIVITY.status);
}

/** Keep the confirmed save visible even if its following history read fails. */
export async function updateSavedSetup(base: string, fleet: Fleet, signal: AbortSignal,
  update: (patch: Partial<DraftView>) => void, lists: Partial<DraftView>): Promise<void> {
  update({...lists, fleet, schedules: fleet.schedules, savedSetupRevision: fleet.setup_revision});
  update({activities: await readSetupActivity(base, fleet.id, signal)});
}
