"use client";

import type { Fleet, Schedule } from "@/lib/api/draft-types";
import Link from "next/link";
import { Button, DashboardPanel, DashboardPanelTitle } from "@agentsfleet/design-system";
import { START_KIND, scheduleOwed } from "@/lib/api/draft-types";
import { SetupChecks } from "./SetupChecks";
import { ScheduleSync } from "./ScheduleSync";

export function SetupProgress({fleet, workspace, onContinue, onRetrySchedule, busy}: {
  fleet: Fleet; workspace: string; onContinue: () => void; onRetrySchedule: (schedule: Schedule) => void; busy: boolean;
}) {
  const active = fleet.status === "active";
  const unregistered = fleet.schedules.some(scheduleOwed);
  return <DashboardPanel className="space-y-md" aria-label="Saved fleet setup">
    <DashboardPanelTitle>{!active ? `${fleet.name} setup is waiting`
      : unregistered ? `${fleet.name} is active, but a schedule is not registered` : `${fleet.name} is ready`}</DashboardPanelTitle>
    <SetupChecks checks={fleet.setup.checks} label="Saved fleet readiness" />
    <ScheduleSync schedules={fleet.schedules} busy={busy} onRetry={onRetrySchedule} />
    {fleet.start.kind === START_KIND.runNow ? <p className="text-sm">Initial run: {fleet.start.state}.
      {fleet.start.state === "accepted" ? " Accepted work can still be running." : " Setup and durable admission must finish before acceptance."}</p> : null}
    <div className="flex flex-wrap gap-sm">
      {!active || fleet.start.kind === START_KIND.runNow && fleet.start.state !== "accepted" ? <Button onClick={onContinue} disabled={busy}>Continue setup</Button> : null}
      <Button variant="outline" asChild><Link href={`/w/${workspace}/fleets/${fleet.id}`}>Open fleet</Link></Button>
    </div>
  </DashboardPanel>;
}
