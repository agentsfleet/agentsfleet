// A schedule saves before the external scheduler registers it; its `:sync`
// action retries that registration, the same route the CLI names.

import type { DraftView } from "./draft-state";
import type { Schedule } from "./draft-types";

export type { Schedule };

/** The schedule's `:sync` action, as its own path segment (`FleetRoute::ScheduleSync`). */
export function scheduleSyncPath(base: string, schedule: Schedule): string {
  return `${base}/fleets/${encodeURIComponent(schedule.fleet_id)}/schedules/${encodeURIComponent(schedule.schedule_id)}/sync`;
}

/** The view after `row` comes back from a sync: it replaces its namesake in the applied list and the saved fleet. */
export function scheduleSynced(view: DraftView, row: Schedule): Pick<DraftView, "schedules"> & Partial<Pick<DraftView, "fleet">> {
  const swap = (rows: readonly Schedule[]): Schedule[] => rows.map((current) => current.schedule_id === row.schedule_id ? row : current);
  return {schedules: swap(view.schedules), ...(view.fleet ? {fleet: {...view.fleet, schedules: swap(view.fleet.schedules)}} : {})};
}
