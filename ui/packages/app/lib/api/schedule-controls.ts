import { z } from "zod";
import { ScheduleSchema, type Schedule } from "./schedule-types";
import { liveRequest, REQUEST_METHOD, RequestFailed } from "./live-request";

// Every schedule is its own row: a person's (`api`) or one the fleet added (`fleet`).
const DetailSchema = ScheduleSchema.extend({source: z.enum(["api", "fleet"]), etag: z.string().min(1)});
export type ScheduleDetail = z.infer<typeof DetailSchema>;
export type ScheduleEdit = Pick<Schedule, "cron" | "timezone" | "message">;

function path(workspace: string, schedule: Schedule) {
  return `/live/v1/workspaces/${encodeURIComponent(workspace)}/fleets/${encodeURIComponent(schedule.fleet_id)}/schedules/${encodeURIComponent(schedule.schedule_id)}`;
}

export async function readSchedule(workspace: string, schedule: Schedule, signal: AbortSignal): Promise<ScheduleDetail> {
  const current = await liveRequest({path: path(workspace, schedule)}, DetailSchema, signal);
  if (current.fleet_id !== schedule.fleet_id || current.schedule_id !== schedule.schedule_id)
    throw new RequestFailed("This wake-up could not be confirmed. Refresh the fleet.", 502);
  return current;
}

/** A schedule changes only its own fields, against the version the caller read. */
export function changeSchedule(workspace: string, current: ScheduleDetail, change: Partial<ScheduleEdit> & {status?: "active" | "paused"}, signal: AbortSignal): Promise<Schedule> {
  return liveRequest({path: path(workspace, current), method: REQUEST_METHOD.patch,
    headers: {"If-Match": current.etag}, body: change}, ScheduleSchema, signal);
}

/** The schedule's `:sync` action (`FleetRoute::ScheduleSync`), a POST with no body: retries its registration. */
export function syncSchedule(workspace: string, schedule: Schedule, signal: AbortSignal): Promise<Schedule> {
  return liveRequest({path: `${path(workspace, schedule)}/sync`, method: REQUEST_METHOD.post}, ScheduleSchema, signal);
}

export function deleteSchedule(workspace: string, schedule: Schedule, signal: AbortSignal): Promise<Schedule | null> {
  // A 200 means deregistration is still pending; only 204 means it is gone.
  return liveRequest({path: path(workspace, schedule), method: REQUEST_METHOD.delete}, ScheduleSchema.nullable(), signal);
}
