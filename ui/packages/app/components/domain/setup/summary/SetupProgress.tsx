"use client";

import type { Definition, Fleet, Schedule } from "@/lib/api/draft-types";
import { Button, Section } from "@agentsfleet/design-system";
import { saveDoneText } from "@/lib/events/fleet-activity";
import { setupRecovery } from "@/lib/api/schedule-status";
import { SetupChecks } from "./SetupChecks";
import { ScheduleSync } from "../../schedules/ScheduleSync";

export function SetupProgress({fleet, definition = fleet.definition, workspace, onContinue, onRetrySchedule, onRefresh, busy}: {
  fleet: Fleet; definition?: Definition; workspace: string; onContinue: () => void; onRetrySchedule: (schedule: Schedule) => void; onRefresh?: () => Promise<void>; busy: boolean;
}) {
  const {retryStart, canFinish} = setupRecovery(fleet);
  return <Section className="my-md gap-sm py-xs" aria-label="Saved fleet setup">
    <h2 className="text-label font-medium text-muted-foreground">{saveDoneText(fleet.setup_revision)}</h2>
    <SetupChecks issuesOnly checks={fleet.setup.checks} label="Saved fleet readiness" workspace={workspace} definition={definition} onRefresh={onRefresh} />
    {canFinish || retryStart ? <Button className="w-fit" size="sm" variant="ghost" onClick={onContinue} disabled={busy}>{retryStart ? "Retry first run" : "Finish setup"}</Button> : null}
    <ScheduleSync schedules={fleet.schedules} busy={busy} onRetry={onRetrySchedule} />
  </Section>;
}
