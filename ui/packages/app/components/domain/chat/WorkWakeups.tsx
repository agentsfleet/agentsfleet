"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Button, Time, formatTimeClockUtc, formatTimeCompactUtc } from "@agentsfleet/design-system";
import { ClockIcon } from "@phosphor-icons/react/ssr";
import type { Schedule } from "@/lib/api/draft-types";
import { SCHEDULE_RUN_STATE, SCHEDULE_SYNC } from "@/lib/api/draft-vocabulary";
import { scheduleStateText } from "@/lib/api/schedule-status";
import { WorkSection } from "./WorkContextSections";

const MAX_TIMER_MS = 2_147_483_647;
const DAY_MS = 86_400_000;

export function WorkWakeups({schedules, compact = false}: {schedules?: readonly Schedule[]; compact?: boolean}) {
  const visible = schedules?.filter(schedule => schedule.status !== "removed" && schedule.status !== "expired");
  if (!visible?.length) return null;
  const ordered = [...visible].sort((left, right) => {
    const attention = Number(right.sync !== SCHEDULE_SYNC.synced) - Number(left.sync !== SCHEDULE_SYNC.synced);
    return attention || (left.next_run_at ? Date.parse(left.next_run_at) : Infinity) - (right.next_run_at ? Date.parse(right.next_run_at) : Infinity);
  });
  const displayed = compact ? ordered.slice(0, 1) : visible;
  return <WorkSection title={compact ? "Next wake-up" : "Wake-ups"}><WakeupRows schedules={displayed} compact={compact} />
    {compact && visible.length > 1 ? <details className="text-eyebrow text-muted-foreground"><summary className="min-h-11 cursor-pointer content-center summary:min-h-9">{visible.length - 1} more</summary><WakeupRows schedules={ordered.slice(1)} compact={false} /></details> : null}
  </WorkSection>;
}

function WakeupRows({schedules, compact}: {schedules: readonly Schedule[]; compact: boolean}) {
  return <ul className="space-y-lg">
    {schedules.map(schedule => <li key={schedule.schedule_id} className="flex min-w-0 items-center gap-sm">
      <ClockIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" />
      <div className="min-w-0 break-words text-label leading-label">
        {compact ? null : <p>{schedule.description}</p>}<WakeupState schedule={schedule} compact={compact} />
        {compact ? <p className="mt-xs text-eyebrow text-muted-foreground">{schedule.description}</p> : null}
      </div>
    </li>)}
  </ul>;
}

export function WakeupState({schedule, compact}: {schedule: Schedule; compact?: boolean}) {
  if (schedule.status === "deleting" || schedule.status === "paused") return <p className="text-eyebrow text-muted-foreground">{schedule.sync === SCHEDULE_SYNC.synced ? scheduleStateText(schedule) : `${schedule.status === "deleting" ? "Removal" : "Pause"} ${schedule.sync === SCHEDULE_SYNC.failed ? "failed. Open setup to retry." : "pending."}`}</p>;
  if (schedule.sync !== SCHEDULE_SYNC.synced) return <p className="text-eyebrow leading-eyebrow text-warning">
    {schedule.sync === SCHEDULE_SYNC.failed ? "Registration failed. Open setup to retry." : "Scheduler registration pending."}
  </p>;
  if (schedule.status !== "active") return <p className="text-eyebrow text-muted-foreground">{scheduleStateText(schedule)}</p>;
  if (schedule.run_state !== SCHEDULE_RUN_STATE.scheduled || !schedule.next_run_at)
    return <p className="text-eyebrow text-muted-foreground">Next wake-up unavailable.</p>;
  return <NextWakeup instant={schedule.next_run_at} timezone={schedule.timezone} compact={compact} />;
}

function NextWakeup({instant, timezone, compact}: {instant: string; timezone: string; compact?: boolean}) {
  const router = useRouter();
  const [clock, setClock] = useState<number | null>(null);
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>;
    const check = () => {
      const now = Date.now();
      const remaining = Date.parse(instant) - now;
      setClock(now);
      const midnight = (Math.floor(now / DAY_MS) + 1) * DAY_MS;
      if (remaining > 0) timer = setTimeout(check, Math.min(remaining, midnight - now, MAX_TIMER_MS));
    };
    check();
    return () => clearTimeout(timer);
  }, [instant]);
  const elapsed = clock !== null && Date.parse(instant) <= clock;
  const today = clock !== null && new Date(instant).toISOString().slice(0, 10) === new Date(clock).toISOString().slice(0, 10);
  const compactLabel = `${today ? formatTimeClockUtc(instant) : formatTimeCompactUtc(instant)} UTC`;
  return elapsed ? <div className="text-eyebrow text-muted-foreground">
    <p>Next wake-up needs an update.</p><Button variant="link" size="eyebrow" onClick={() => router.refresh()}>Refresh schedule</Button>
  </div> : <p className={`text-eyebrow leading-eyebrow ${compact ? "truncate whitespace-nowrap text-inherit" : "text-muted-foreground"}`}><span className={compact ? "sr-only sm:not-sr-only" : undefined}>Next wake-up · </span><Time value={instant} timeZone={timezone} label={compact ? compactLabel : undefined} tooltip={compact} />{compact ? null : <span> ({timezone})</span>}</p>;
}
