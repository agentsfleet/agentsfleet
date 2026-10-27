import { z } from "zod";
import { Instant } from "./decode";
import { setupRequest } from "./draft-request";
import { losesAccess } from "./draft-errors";
import { FLEET_ACTIVITY, fleetActivityLabel, readFleetActivity, type FleetActivity } from "../events/fleet-activity";
import type { Fleet } from "./draft-types";
import type { DraftView } from "./draft-state";

export const ActivityRowSchema = z.object({
  event_id: z.string(), event_type: z.string(), status: z.string(),
  request_json: z.string(), created_at: Instant,
});
const PageSchema = z.object({items: z.array(ActivityRowSchema), next_cursor: z.string().nullable()});
const ACTIVITY_PAGE_LIMIT = 25;
export type SetupActivity = z.infer<typeof ActivityRowSchema>;

const parsed = new WeakMap<SetupActivity, FleetActivity | null>();

export function setupActivity(row: SetupActivity): FleetActivity | null {
  if (parsed.has(row)) return parsed.get(row) ?? null;
  const activity = readFleetActivity(row.request_json); parsed.set(row, activity); return activity;
}

export function setupActivityText(row: SetupActivity): string { return fleetActivityLabel(setupActivity(row)); }

/** Read the same durable history as fleet chat, including safe unknown activity kinds. */
export async function readSetupActivity(base: string, fleet: string, signal: AbortSignal): Promise<SetupActivity[]> {
  return (await readSetupActivityPage(base, fleet, signal)).items;
}

export async function readSetupActivityPage(base: string, fleet: string, signal: AbortSignal, after?: string) {
  const query = new URLSearchParams({event_type: FLEET_ACTIVITY.type, limit: String(ACTIVITY_PAGE_LIMIT)});
  if (after) query.set("starting_after", after);
  const page = await setupRequest({path: `${base}/fleets/${fleet}/messages?${query}`}, PageSchema, signal);
  return {...page, items: page.items.filter(row => row.event_type === FLEET_ACTIVITY.type && row.status === FLEET_ACTIVITY.status)};
}

/** Keep the confirmed save visible even if its following history read fails. */
export async function updateSavedSetup(base: string, fleet: Fleet, signal: AbortSignal,
  update: (patch: Partial<DraftView>) => void, lists: Partial<DraftView>): Promise<void> {
  update({...lists, fleet, schedules: fleet.schedules, savedSetupRevision: fleet.setup_revision});
  try { const page = await readSetupActivityPage(base, fleet.id, signal); update({activities: page.items, activityCursor: page.next_cursor}); }
  catch (cause) { if (signal.aborted || losesAccess(cause)) throw cause; }
}
