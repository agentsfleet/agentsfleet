import type { Fleet, Schedule } from "./draft-types";
import { SCHEDULE_SYNC, START_KIND } from "./draft-vocabulary";

import { AGENTSFLEET_STATUS } from "./fleets-types";

/** Unconfirmed schedules may never fire until their registration is retried. */
export function scheduleOwed(schedule: Schedule): boolean {
  return schedule.sync !== SCHEDULE_SYNC.synced;
}

/** Saved setup and first-run recovery use the same lifecycle rules everywhere. */
export function setupRecovery(fleet: Fleet): {pending: boolean; canFinish: boolean; retryStart: boolean} {
  const installing = fleet.status === AGENTSFLEET_STATUS.INSTALLING;
  const retryStart = fleet.status === AGENTSFLEET_STATUS.ACTIVE && fleet.start.kind === START_KIND.runNow && fleet.start.state !== "accepted";
  return {pending: installing || retryStart, retryStart,
    canFinish: installing && fleet.setup.checks.every(check => check.state === "ready" || check.kind === "stream" || check.kind === "activation")};
}
