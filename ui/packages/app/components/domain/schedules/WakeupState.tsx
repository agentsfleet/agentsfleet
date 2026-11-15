"use client";

import {useEffect, useState} from "react";
import {Button, Time, sameCalendarDay, nextLocalMidnight} from "@agentsfleet/design-system";
import type {Schedule} from "@/lib/api/draft-types";
import {SCHEDULE_RUN_STATE, SCHEDULE_SYNC} from "@/lib/api/draft-vocabulary";
import {scheduleStateText} from "@/lib/api/schedule-status";

const MAX_TIMER_MS = 2_147_483_647;

export function WakeupState({schedule, onRefresh}: {schedule: Schedule; onRefresh: () => void}) {
  if (schedule.status === "deleting" || schedule.status === "paused") return <p className="text-eyebrow text-muted-foreground">{schedule.sync === SCHEDULE_SYNC.synced ? scheduleStateText(schedule) : `${schedule.status === "deleting" ? "Removal" : "Pause"} ${schedule.sync === SCHEDULE_SYNC.failed ? "failed." : "pending."}`}</p>;
  if (schedule.sync !== SCHEDULE_SYNC.synced) return <p className="text-eyebrow leading-eyebrow text-warning">
    {schedule.sync === SCHEDULE_SYNC.failed ? "Registration failed." : "Scheduler registration pending."}
  </p>;
  if (schedule.status !== "active") return <p className="text-eyebrow text-muted-foreground">{scheduleStateText(schedule)}</p>;
  if (schedule.run_state !== SCHEDULE_RUN_STATE.scheduled || !schedule.next_run_at)
    return <p className="text-eyebrow text-muted-foreground">Next wake-up unavailable.</p>;
  return <NextWakeup instant={schedule.next_run_at} onRefresh={onRefresh} />;
}

function NextWakeup({instant, onRefresh}: {instant: string; onRefresh: () => void}) {
  const [clock, setClock] = useState<number | null>(null);
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>;
    const check = () => {
      const now = Date.now();
      const remaining = Date.parse(instant) - now;
      setClock(now);
      if (remaining > 0) timer = setTimeout(check, Math.min(remaining, nextLocalMidnight(now) - now, MAX_TIMER_MS));
    };
    check();
    return () => clearTimeout(timer);
  }, [instant]);
  const elapsed = clock !== null && Date.parse(instant) <= clock;
  const today = clock !== null && sameCalendarDay(instant, new Date(clock));
  return elapsed ? <div className="text-eyebrow text-muted-foreground">
    <p>Next wake-up needs an update.</p><Button variant="link" size="eyebrow" onClick={onRefresh}>Refresh schedule</Button>
  </div> : <p className="truncate whitespace-nowrap text-label font-medium leading-label text-foreground"><span className="sr-only">Next wake-up · </span><Time value={instant} format={today ? "minute" : "compact"} tooltip /></p>;
}
