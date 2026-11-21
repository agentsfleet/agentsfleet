import type { Fleet, Schedule } from "./draft-types";
import { SCHEDULE_RUN_STATE, SCHEDULE_SYNC, START_KIND } from "./draft-vocabulary";

import { AGENTSFLEET_STATUS } from "./fleets-types";

/** Unconfirmed schedules may never fire until their registration is retried. */
export function scheduleOwed(schedule: Schedule): boolean {
  return schedule.sync !== SCHEDULE_SYNC.synced;
}

export function nextFleetWakeup(schedules: readonly Schedule[]): string | null {
  let next: string | null = null;
  for (const schedule of schedules) {
    const instant = schedule.next_run_at;
    if (schedule.status !== "active" || schedule.sync !== SCHEDULE_SYNC.synced || schedule.run_state !== SCHEDULE_RUN_STATE.scheduled || !instant) continue;
    if (!next || Date.parse(instant) < Date.parse(next)) next = instant;
  }
  return next;
}

/** Describe stored intent separately from confirmed scheduler registration. */
export function scheduleStateText(schedule: Pick<Schedule, "status" | "sync">): string {
  switch (schedule.status) {
    case "active": return schedule.sync === SCHEDULE_SYNC.synced ? "Active" : schedule.sync === SCHEDULE_SYNC.failed ? "Registration failed" : "Registration pending";
    case "paused": return schedule.sync === SCHEDULE_SYNC.synced ? "Paused" : schedule.sync === SCHEDULE_SYNC.failed ? "Pause failed" : "Pause requested; scheduler sync pending";
    case "deleting": return schedule.sync === SCHEDULE_SYNC.failed ? "Removal failed" : "Removal requested";
    case "removed": return "Removed";
    case "expired": return "Expired without firing";
    default: return "Status unavailable";
  }
}

/** Saved setup and first-run recovery use the same lifecycle rules everywhere. */
export function setupRecovery(fleet: Fleet): {pending: boolean; canFinish: boolean; retryStart: boolean} {
  const installing = fleet.status === AGENTSFLEET_STATUS.INSTALLING;
  const retryStart = fleet.status === AGENTSFLEET_STATUS.ACTIVE && fleet.start.kind === START_KIND.runNow && fleet.start.state !== "accepted";
  return {pending: installing || retryStart, retryStart,
    canFinish: installing && fleet.setup.checks.every(check => check.state === "ready" || check.kind === "stream" || check.kind === "activation")};
}
