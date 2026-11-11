"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Button, Time } from "@agentsfleet/design-system";
import { ClockIcon } from "@phosphor-icons/react/ssr";
import type { Schedule } from "@/lib/api/draft-types";
import { SCHEDULE_RUN_STATE, SCHEDULE_SYNC } from "@/lib/api/draft-vocabulary";
import { scheduleStateText } from "@/lib/api/schedule-status";
import { WorkSection } from "./WorkContextSections";

const MAX_TIMER_MS = 2_147_483_647;

export function WorkWakeups({schedules}: {schedules?: readonly Schedule[]}) {
  const visible = schedules?.filter(schedule => schedule.status !== "removed" && schedule.status !== "expired");
  if (!visible?.length) return null;
  return <WorkSection title="Wake-ups"><ul className="space-y-lg">
    {visible.map(schedule => <li key={schedule.schedule_id} className="flex min-w-0 items-start gap-sm">
      <ClockIcon size={16} className="mt-xs shrink-0 text-muted-foreground" aria-hidden="true" />
      <div className="min-w-0 break-words text-label leading-label">
        <p>{schedule.description}</p><WakeupState schedule={schedule} />
      </div>
    </li>)}
  </ul></WorkSection>;
}

function WakeupState({schedule}: {schedule: Schedule}) {
  if (schedule.sync !== SCHEDULE_SYNC.synced) return <p className="text-eyebrow leading-eyebrow text-warning">
    {schedule.sync === SCHEDULE_SYNC.failed ? "Registration failed. Open setup to retry." : "Scheduler registration pending."}
  </p>;
  if (schedule.status !== "active") return <p className="text-eyebrow text-muted-foreground">{scheduleStateText(schedule)}</p>;
  if (schedule.run_state !== SCHEDULE_RUN_STATE.scheduled || !schedule.next_run_at)
    return <p className="text-eyebrow text-muted-foreground">Next wake-up unavailable.</p>;
  return <NextWakeup instant={schedule.next_run_at} timezone={schedule.timezone} />;
}

function NextWakeup({instant, timezone}: {instant: string; timezone: string}) {
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
  </div> : <p className="text-eyebrow leading-eyebrow text-muted-foreground">Next <Time value={instant} format="absolute" timeZone={timezone} tooltip={false} /> <span>({timezone})</span></p>;
}
