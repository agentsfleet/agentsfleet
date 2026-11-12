"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Button, Time, formatTimeCompactUtc } from "@agentsfleet/design-system";
import { ClockIcon } from "@phosphor-icons/react/ssr";
import type { Schedule } from "@/lib/api/draft-types";
import { SCHEDULE_RUN_STATE, SCHEDULE_SYNC } from "@/lib/api/draft-vocabulary";
import { scheduleStateText } from "@/lib/api/schedule-status";
import { WorkSection } from "./WorkContextSections";

const MAX_TIMER_MS = 2_147_483_647;

export function WorkWakeups({schedules, compact = false}: {schedules?: readonly Schedule[]; compact?: boolean}) {
  const visible = schedules?.filter(schedule => schedule.status !== "removed" && schedule.status !== "expired");
  if (!visible?.length) return null;
  const ordered = [...visible].sort((left, right) => {
    const attention = Number(right.sync !== SCHEDULE_SYNC.synced) - Number(left.sync !== SCHEDULE_SYNC.synced);
    return attention || (left.next_run_at ? Date.parse(left.next_run_at) : Infinity) - (right.next_run_at ? Date.parse(right.next_run_at) : Infinity);
  });
  const displayed = compact ? ordered.slice(0, 1) : visible;
  return <WorkSection title="Wake-ups"><WakeupRows schedules={displayed} compact={compact} />
    {compact && visible.length > 1 ? <details className="text-eyebrow text-muted-foreground"><summary className="min-h-11 cursor-pointer content-center summary:min-h-9">{visible.length - 1} more</summary><WakeupRows schedules={ordered.slice(1)} compact={false} /></details> : null}
  </WorkSection>;
}

function WakeupRows({schedules, compact}: {schedules: readonly Schedule[]; compact: boolean}) {
  return <ul className="space-y-lg">
    {schedules.map(schedule => <li key={schedule.schedule_id} className="flex min-w-0 items-start gap-sm">
      <ClockIcon size={16} className="mt-xs shrink-0 text-muted-foreground" aria-hidden="true" />
      <div className="min-w-0 break-words text-label leading-label">
        {compact ? null : <p>{schedule.description}</p>}<WakeupState schedule={schedule} compact={compact} />
      </div>
    </li>)}
  </ul>;
}

function WakeupState({schedule, compact}: {schedule: Schedule; compact?: boolean}) {
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
  const [elapsed, setElapsed] = useState(false);
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>;
    const check = () => {
      const remaining = Date.parse(instant) - Date.now();
      setElapsed(remaining <= 0);
      if (remaining > 0) timer = setTimeout(check, Math.min(remaining, MAX_TIMER_MS));
    };
    check();
    return () => clearTimeout(timer);
  }, [instant]);
  return elapsed ? <div className="text-eyebrow text-muted-foreground">
    <p>Next wake-up needs an update.</p><Button variant="link" size="eyebrow" onClick={() => router.refresh()}>Refresh schedule</Button>
  </div> : <p className="text-eyebrow leading-eyebrow text-muted-foreground">Next <Time value={instant} timeZone={timezone} label={compact ? `${formatTimeCompactUtc(instant)} UTC` : undefined} tooltip={compact} />{compact ? null : <span> ({timezone})</span>}</p>;
}
