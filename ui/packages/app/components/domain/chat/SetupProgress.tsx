"use client";

import type { Definition, Fleet, Schedule } from "@/lib/api/draft-types";
import Link from "next/link";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, Button, DashboardPanel, DashboardPanelTitle } from "@agentsfleet/design-system";
import { START_KIND, scheduleOwed } from "@/lib/api/draft-types";
import { SetupChecks } from "./SetupChecks";
import { ScheduleSync } from "./ScheduleSync";
import { SetupSummary } from "./SetupSummary";

const CHECKS = "checks";

export function SetupProgress({fleet, definition = fleet.definition, workspace, onContinue, onRetrySchedule, busy}: {
  fleet: Fleet; definition?: Definition; workspace: string; onContinue: () => void; onRetrySchedule: (schedule: Schedule) => void; busy: boolean;
}) {
  const active = fleet.status === "active";
  const unregistered = fleet.schedules.some(scheduleOwed);
  return <DashboardPanel className="space-y-lg border-primary/30" aria-label="Saved fleet setup">
    <DashboardPanelTitle>{!active ? `${fleet.name} setup is waiting`
      : unregistered ? `${fleet.name} is active, but a schedule is not registered` : `${fleet.name} is ready`}</DashboardPanelTitle>
    <p className="text-sm text-muted-foreground">{active && !unregistered ? "Setup is complete. This fleet can receive work."
      : "Your fleet is saved, but setup still needs attention. Complete the steps below before relying on it."}</p>
    <SetupSummary definition={definition} />
    <ScheduleSync schedules={fleet.schedules} busy={busy} onRetry={onRetrySchedule} />
    <div className="space-y-sm rounded-md bg-primary/5 p-lg">
      <h3 className="font-semibold">What happens next</h3>
      {fleet.start.kind === START_KIND.none ? <p className="text-sm">You saved this fleet without requesting a first run. Open the fleet and send a message to start work. Configured automatic triggers can also start work once setup is ready.</p>
        : <p className="text-sm">{fleet.start.state === "accepted" ? "Your first run was accepted. Open the fleet to follow its progress and results. Accepted work can still be running."
          : "Your first run has not been confirmed. Continue setup to retry, then open the fleet to check its activity."}</p>}
    </div>
    {fleet.setup.checks.length ? <Accordion type="multiple" defaultValue={active ? [] : [CHECKS]}>
      <AccordionItem value={CHECKS}><AccordionTrigger>{active ? "Setup checks" : "What needs attention"}</AccordionTrigger>
        <AccordionContent><SetupChecks checks={fleet.setup.checks} label="Saved fleet readiness" /></AccordionContent>
      </AccordionItem>
    </Accordion> : null}
    <div className="flex flex-wrap gap-sm">
      {!active || fleet.start.kind === START_KIND.runNow && fleet.start.state !== "accepted" ? <Button onClick={onContinue} disabled={busy}>Continue setup</Button> : null}
      <Button asChild><Link href={`/w/${workspace}/fleets/${fleet.id}`}>Open fleet</Link></Button>
    </div>
  </DashboardPanel>;
}
