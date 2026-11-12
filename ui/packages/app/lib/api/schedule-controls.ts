import { z } from "zod";
import { ScheduleSchema, SETUP_HEADERS, type Schedule } from "./draft-types";
import { REQUEST_METHOD, setupRequest, SetupUnavailable } from "./draft-request";

const OverrideSchema = z.object({cron: z.string().optional(), timezone: z.string().optional(),
  message: z.string().optional(), status: z.enum(["active", "paused"]).optional()});
const DetailSchema = ScheduleSchema.extend({source: z.enum(["api", "fleet", "trigger"]), etag: z.string().min(1), override: OverrideSchema.optional()});
export type ScheduleDetail = z.infer<typeof DetailSchema>;
export type ScheduleEdit = Pick<Schedule, "cron" | "timezone" | "message">;

function path(workspace: string, schedule: Schedule) {
  return `/live/v1/workspaces/${encodeURIComponent(workspace)}/fleets/${encodeURIComponent(schedule.fleet_id)}/schedules/${encodeURIComponent(schedule.schedule_id)}`;
}

export async function readSchedule(workspace: string, schedule: Schedule, signal: AbortSignal): Promise<ScheduleDetail> {
  const current = await setupRequest({path: path(workspace, schedule)}, DetailSchema, signal);
  if (current.fleet_id !== schedule.fleet_id || current.schedule_id !== schedule.schedule_id)
    throw new SetupUnavailable("This wake-up could not be confirmed. Refresh the fleet.", 502);
  return current;
}

/** A recipe overlay is replaced as a whole; retain its other explicit fields. */
export async function changeSchedule(workspace: string, current: ScheduleDetail, change: Partial<ScheduleEdit> & {status?: "active" | "paused"}, signal: AbortSignal): Promise<Schedule> {
  const body = current.source === "trigger" ? {override: {...current.override, ...change}} : change;
  return setupRequest({path: path(workspace, current), method: REQUEST_METHOD.patch,
    headers: {[SETUP_HEADERS.match]: current.etag}, body}, ScheduleSchema, signal);
}

export function deleteSchedule(workspace: string, schedule: Schedule, signal: AbortSignal): Promise<Schedule | null> {
  // A 200 means deregistration is still pending; only 204 means it is gone.
  return setupRequest({path: path(workspace, schedule), method: REQUEST_METHOD.delete}, ScheduleSchema.nullable(), signal);
}
