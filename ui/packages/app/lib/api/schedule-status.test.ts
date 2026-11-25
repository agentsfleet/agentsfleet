import { expect, it } from "vitest";
import { FAILED_SCHEDULE, PENDING_SCHEDULE, SCHEDULE } from "@/tests/helpers/fleet-fixtures";
import { nextFleetWakeup, scheduleOwed } from "./schedule-status";

it("finds the earliest wake-up among registered active schedules only", () => {
  const later = {...SCHEDULE, schedule_id: "later", next_run_at: "2026-11-06T03:30:00Z"};
  const earlier = {...SCHEDULE, schedule_id: "earlier", next_run_at: "2026-11-04T03:30:00Z"};
  expect(nextFleetWakeup([later, earlier, later])).toBe(earlier.next_run_at);
  expect(nextFleetWakeup([SCHEDULE, later])).toBe(SCHEDULE.next_run_at);
  expect(nextFleetWakeup([{...SCHEDULE, status: "paused"}, FAILED_SCHEDULE, PENDING_SCHEDULE, {...SCHEDULE, run_state: "paused"}, {...SCHEDULE, next_run_at: null}])).toBeNull();
  expect(nextFleetWakeup([])).toBeNull();
});

it("owes a retry for every schedule the scheduler has not confirmed", () => {
  expect([SCHEDULE, FAILED_SCHEDULE, PENDING_SCHEDULE].map(scheduleOwed)).toEqual([false, true, true]);
});
