"use client";

import type { Definition, Fleet, Schedule } from "@/lib/api/draft-types";
import { Button, Section } from "@agentsfleet/design-system";
import { saveDoneText } from "@/lib/events/fleet-activity";
import { START_KIND } from "@/lib/api/draft-vocabulary";
import { SetupChecks } from "./SetupChecks";
import { ScheduleSync } from "./ScheduleSync";

export function SetupProgress({fleet, definition = fleet.definition, workspace, onContinue, onRetrySchedule, onRefresh, busy}: {
  fleet: Fleet; definition?: Definition; workspace: string; onContinue: () => void; onRetrySchedule: (schedule: Schedule) => void; onRefresh?: () => Promise<void>; busy: boolean;
}) {
  const retryStart = fleet.status === "active" && fleet.start.kind === START_KIND.runNow && fleet.start.state !== "accepted";
  const canFinish = fleet.status === "installing" && fleet.setup.checks.every(check => check.state === "ready" || check.kind === "stream" || check.kind === "activation");
  return <Section className="my-md gap-sm py-xs" aria-label="Saved fleet setup">
    <h2 className="text-body-sm font-semibold text-foreground">{saveDoneText(fleet.setup_revision)}</h2>
    <SetupChecks checks={fleet.setup.checks} label="Saved fleet readiness" workspace={workspace} definition={definition} onRefresh={onRefresh} />
    <div className="flex flex-wrap items-center gap-sm">
      {canFinish || retryStart ? <Button size="sm" variant="ghost" onClick={onContinue} disabled={busy}>{retryStart ? "Retry first run" : "Finish setup"}</Button> : null}
    </div>
    <ScheduleSync schedules={fleet.schedules} busy={busy} onRetry={onRetrySchedule} />
  </Section>;
}
