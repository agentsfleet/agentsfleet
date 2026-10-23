import type { Schedule } from "./draft-types";
import { SCHEDULE_SYNC } from "./draft-vocabulary";

/** Unconfirmed schedules may never fire until their registration is retried. */
export function scheduleOwed(schedule: Schedule): boolean {
  return schedule.sync !== SCHEDULE_SYNC.synced;
}
