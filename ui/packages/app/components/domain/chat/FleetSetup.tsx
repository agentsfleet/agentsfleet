"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { Button, Section } from "@agentsfleet/design-system";
import { savedDraft } from "@/lib/api/draft-continuation";
import { ACTIVITY_KIND, readFleetActivity, saveDoneText } from "@/lib/events/fleet-activity";
import { START_KIND } from "@/lib/api/draft-vocabulary";
import { SetupChecks } from "./SetupChecks";
import { ScheduleSync } from "./ScheduleSync";

export function FleetSetup({client, view, workspace}: {client: DraftClient; view: DraftView; workspace: string}) {
  if (view.inaccessible || (!view.fleet && !view.draft?.candidate && !view.preview)) return null;
  const setup = view.draft?.review;
  const fleet = view.fleet;
  const revision = fleet?.setup_revision ?? view.savedSetupRevision ?? setup?.effective.target_setup_revision ?? view.preview?.setupRevision;
  const editing = Boolean(view.draft && !savedDraft(view) && (setup?.effective.target_setup_revision || fleet));
  const checks = editing || !fleet ? setup?.readiness ?? [] : fleet.setup.checks;
  const retryStart = fleet?.status === "active" && fleet.start.kind === START_KIND.runNow && fleet.start.state !== "accepted";
  const definition = view.draft?.candidate?.definition ?? view.preview?.definition ?? fleet?.definition;
  const continueSetup = fleet?.status === "installing" && fleet.setup.checks.every(check => check.state === "ready" || check.kind === "stream" || check.kind === "activation");
  const recorded = view.activities?.some(row => {
    const activity = readFleetActivity(row.request_json);
    return activity && (activity.kind === ACTIVITY_KIND.setupSaved || activity.kind === ACTIVITY_KIND.setupReady) && activity.revision === revision;
  });
  return <Section className="ph-no-capture ph-no-record my-md gap-sm py-xs" aria-label="Fleet setup">
    {recorded || !revision ? null : <h2 className="text-body font-semibold text-foreground">{saveDoneText(revision)}</h2>}
    {checks.length ? <SetupChecks checks={checks} label="Configuration readiness" workspace={workspace} definition={definition} onRefresh={() => client.refreshFleetSetup(true)} /> : null}
    <p className="text-label text-muted-foreground">{editing ? "Keep chatting. Save changes when you’re ready."
      : revision ? "You can keep chatting here to change the setup."
      : "Keep chatting to refine it. Save fleet when you’re ready."}</p>
    {continueSetup || retryStart ? <Button className="w-fit" variant="link" size="sm" disabled={view.busy} onClick={() => {void client.retry().catch(() => undefined);}}>{retryStart ? "Retry first run" : "Finish setup"}</Button> : null}
    {fleet ? <ScheduleSync schedules={fleet.schedules} busy={view.busy} onRetry={schedule => {void client.syncSchedule(schedule).catch(() => undefined);}} /> : null}
  </Section>;
}
