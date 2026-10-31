import { z } from "zod";

export const FLEET_ACTIVITY = {type: "activity", status: "recorded"} as const;
export const ACTIVITY_KIND = {
  setupSaved: "setup_saved", setupReady: "setup_ready", accessSaved: "access_saved", memorySharingChanged: "memory_sharing_changed",
} as const;
const Revision = z.string().regex(/^\d+$/);
const SaveOrigin = z.enum(["cli", "dashboard", "chat"]);
const ActivitySchema = z.discriminatedUnion("kind", [
  z.object({kind: z.literal(ACTIVITY_KIND.setupSaved), revision: Revision, origin: SaveOrigin.optional()}),
  z.object({kind: z.literal(ACTIVITY_KIND.setupReady), revision: Revision}),
  z.object({kind: z.literal(ACTIVITY_KIND.accessSaved), service: z.string().min(1)}),
  z.object({kind: z.literal(ACTIVITY_KIND.memorySharingChanged), read: z.boolean(), publish: z.boolean()}),
]);
export type FleetActivity = z.infer<typeof ActivitySchema>;

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
  }
}
