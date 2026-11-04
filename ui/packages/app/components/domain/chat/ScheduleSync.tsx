"use client";

import { Button, List, ListItem, Section, Time } from "@agentsfleet/design-system";
import { type Schedule } from "@/lib/api/draft-types";
import { SCHEDULE_RUN_STATE, SCHEDULE_SYNC } from "@/lib/api/draft-vocabulary";
import { scheduleOwed, scheduleStateText } from "@/lib/api/schedule-status";

// These controls read current state; historical receipts keep their own snapshot.
export function ScheduleSync({schedules, busy, onRetry}: {schedules: readonly Schedule[]; busy: boolean; onRetry: (schedule: Schedule) => void}) {
  if (schedules.length === 0) return null;
  return <Section className="gap-sm" aria-label="Schedules">
    <List variant="plain" className="flex flex-col gap-sm space-y-0">
      {schedules.map((schedule) => <ListItem key={schedule.schedule_id} className="flex flex-wrap items-center justify-between gap-sm text-conversation">
        <div className="min-w-0 break-words">
          <p>{schedule.description} · {scheduleStateText(schedule)}</p>
          <p className="text-muted-foreground">{schedule.message}</p>
          {schedule.run_state === SCHEDULE_RUN_STATE.scheduled && schedule.next_run_at
            ? <p className="text-muted-foreground">Next run <Time value={schedule.next_run_at} format="absolute" tooltip={false} /></p>
            : schedule.run_state === SCHEDULE_RUN_STATE.unavailable ? <p className="text-muted-foreground">No upcoming run.</p> : null}
          {scheduleOwed(schedule) ? <p className="text-muted-foreground">{owedSentence(schedule)}</p> : null}
        </div>
        {scheduleOwed(schedule) ? <Button size="sm" variant="ghost" disabled={busy} onClick={() => onRetry(schedule)}>{schedule.status === "active" ? "Retry registration" : "Retry sync"}</Button> : null}
      </ListItem>)}
    </List>
  </Section>;
}

function owedSentence(schedule: Schedule): string {
  if (schedule.sync !== SCHEDULE_SYNC.failed) return "Scheduler sync has not been confirmed.";
  return `${schedule.last_error || "Scheduler sync failed."} Will retry on the next sync.`;
}
