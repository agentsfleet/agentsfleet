import { z } from "zod";
import { scheduleStateText } from "@/lib/api/schedule-status";

export const FLEET_ACTIVITY = {type: "activity", status: "recorded"} as const;
export const ACTIVITY_KIND = {
  setupSaved: "setup_saved", setupReady: "setup_ready", accessSaved: "access_saved", memorySharingChanged: "memory_sharing_changed",
  scheduleChanged: "schedule_changed", scheduleRegistered: "schedule_registered", scheduleUnregistered: "schedule_unregistered",
  scheduleSyncFailed: "schedule_sync_failed", scheduleRemoved: "schedule_removed", scheduleExpired: "schedule_expired",
} as const;
const Revision = z.string().regex(/^\d+$/);
const SaveOrigin = z.enum(["cli", "dashboard", "chat"]);
const ScheduleSnapshot = z.object({schedule_id: z.string().min(1), description: z.string().min(1),
  message: z.string(), status: z.string(), sync: z.string()});
const ScheduleChange = z.enum(["created", "changed", "paused", "removal_requested"]);
const ActivitySchema = z.discriminatedUnion("kind", [
  z.object({kind: z.literal(ACTIVITY_KIND.setupSaved), revision: Revision, origin: SaveOrigin.optional()}),
  z.object({kind: z.literal(ACTIVITY_KIND.setupReady), revision: Revision}),
  z.object({kind: z.literal(ACTIVITY_KIND.accessSaved), service: z.string().min(1)}),
  z.object({kind: z.literal(ACTIVITY_KIND.memorySharingChanged), read: z.boolean(), publish: z.boolean()}),
  ScheduleSnapshot.extend({kind: z.literal(ACTIVITY_KIND.scheduleChanged), change: ScheduleChange}),
  ScheduleSnapshot.extend({kind: z.literal(ACTIVITY_KIND.scheduleRegistered)}),
  ScheduleSnapshot.extend({kind: z.literal(ACTIVITY_KIND.scheduleUnregistered)}),
  ScheduleSnapshot.extend({kind: z.literal(ACTIVITY_KIND.scheduleSyncFailed), last_error: z.string()}),
  ScheduleSnapshot.extend({kind: z.literal(ACTIVITY_KIND.scheduleRemoved)}),
  ScheduleSnapshot.extend({kind: z.literal(ACTIVITY_KIND.scheduleExpired)}),
]);
export type FleetActivity = z.infer<typeof ActivitySchema>;
const SCHEDULE_CHANGE_TEXT: Record<z.infer<typeof ScheduleChange>, string> = {
  created: "set up", changed: "changed", paused: "pause requested", removal_requested: "removal requested",
};

export function saveDoneText(revision: string, origin?: z.infer<typeof SaveOrigin>): string {
  return origin === "cli" ? `Setup saved via CLI · v${revision}` : `Setup saved · v${revision}`;
}

/** Only the permitted fields of a known activity can become visible text. */
export function readFleetActivity(json: string): FleetActivity | null {
  try {
    const result = ActivitySchema.safeParse(JSON.parse(json));
    return result.success ? result.data : null;
  } catch {return null;}
}

export function fleetActivityText(json: string): string {
  return fleetActivityLabel(readFleetActivity(json));
}

export function fleetActivityLabel(activity: FleetActivity | null): string {
  if (!activity) return "Activity recorded";
  switch (activity.kind) {
    case ACTIVITY_KIND.setupSaved: return saveDoneText(activity.revision, activity.origin);
    case ACTIVITY_KIND.setupReady: return `Setup done · v${activity.revision}`;
    case ACTIVITY_KIND.accessSaved: return `${activity.service.charAt(0).toUpperCase()}${activity.service.slice(1)} access saved`;
    case ACTIVITY_KIND.memorySharingChanged: return `Workspace memories: learning ${activity.read ? "on" : "off"}, sharing ${activity.publish ? "on" : "off"}`;
    case ACTIVITY_KIND.scheduleChanged: return scheduleLabel(activity, SCHEDULE_CHANGE_TEXT[activity.change]);
    case ACTIVITY_KIND.scheduleRegistered: return scheduleLabel(activity, "registered with the scheduler");
    case ACTIVITY_KIND.scheduleUnregistered: return scheduleLabel(activity, "removed from the scheduler");
    case ACTIVITY_KIND.scheduleSyncFailed:
      return `${scheduleLabel(activity, "could not sync")}. ${activity.last_error} Will retry on the next sync.`;
    case ACTIVITY_KIND.scheduleRemoved: return scheduleLabel(activity, "removed");
    case ACTIVITY_KIND.scheduleExpired: return scheduleLabel(activity, "expired without firing");
  }
}

function scheduleLabel(schedule: z.infer<typeof ScheduleSnapshot>, action: string): string {
  return `Schedule ${schedule.description} ${action}: ${schedule.message} · ${scheduleStateText(schedule)}`;
}
