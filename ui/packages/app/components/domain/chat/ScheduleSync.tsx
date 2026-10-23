"use client";

import { Alert, Button, List, ListItem } from "@agentsfleet/design-system";
import { type Schedule } from "@/lib/api/draft-types";
import { SCHEDULE_SYNC } from "@/lib/api/draft-vocabulary";
import { scheduleOwed } from "@/lib/api/schedule-status";

// A schedule saves before the external scheduler registers it, so a fleet can
// be active while cron work never wakes. Each unregistered schedule says so and
// offers the same `:sync` retry the CLI names.
export function ScheduleSync({schedules, busy, onRetry}: {schedules: readonly Schedule[]; busy: boolean; onRetry: (schedule: Schedule) => void}) {
  const owed = schedules.filter(scheduleOwed);
  if (owed.length === 0) return null;
  return <Alert variant="warning" aria-label="Schedule registration">
    <List variant="plain" className="flex flex-col gap-sm space-y-0">
      {owed.map((schedule) => <ListItem key={schedule.schedule_id} className="flex flex-wrap items-center gap-sm">
        <span>{owedSentence(schedule)}</span>
        <Button size="sm" variant="outline" disabled={busy} onClick={() => onRetry(schedule)}>Retry registration</Button>
      </ListItem>)}
    </List>
  </Alert>;
}

function owedSentence(schedule: Schedule): string {
  if (schedule.sync !== SCHEDULE_SYNC.failed) return `Schedule ${schedule.schedule_id} is saved but its registration was not confirmed, so it may not fire.`;
  const reason = schedule.last_error ? ` (${schedule.last_error})` : "";
  return `Schedule ${schedule.schedule_id} is saved but not registered, so it will not fire${reason}.`;
}
