"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import Link from "next/link";
import { Button, Section } from "@agentsfleet/design-system";
import { START_KIND } from "@/lib/api/draft-vocabulary";
import { scheduleOwed } from "@/lib/api/schedule-status";
import { ScheduleSync } from "./ScheduleSync";

export function FleetSetup({client, view, workspace}: {client: DraftClient; view: DraftView; workspace: string}) {
  if (view.inaccessible || (!view.fleet && !view.draft?.candidate && !view.preview)) return null;
  const setup = view.draft?.review;
  const revision = view.fleet?.setup_revision ?? view.savedSetupRevision ?? setup?.effective.target_setup_revision ?? view.preview?.setupRevision;
  const attention = Boolean(view.fleet && (view.fleet.status !== "active" || view.fleet.schedules.some(scheduleOwed)
    || view.fleet.start.kind === START_KIND.runNow && view.fleet.start.state !== "accepted"));
  const editing = !view.fleet && !view.savedSetupRevision && Boolean(setup?.effective.target_setup_revision || view.preview?.setupRevision);
  const continueSetup = Boolean(view.fleet && (view.fleet.status !== "active"
    || view.fleet.start.kind === START_KIND.runNow && view.fleet.start.state !== "accepted"));
  return <Section className="ph-no-capture ph-no-record my-md gap-xs border-l-2 border-border pl-md py-xs" aria-label="Fleet setup">
    <h2 className="text-body font-semibold">{editing ? `Based on v${revision}` : revision ? `Saved v${revision}` : setup ? "Setup ready" : "Preparing your setup"}</h2>
    <p className="text-label text-muted-foreground">{revision
      ? editing ? "Tell me what you’d like to change. Save changes when you’re ready." : "New runs use this setup. Edit setup to change it."
      : setup || view.preview ? "Keep chatting to refine it, or select Save fleet in the header to create your fleet." : "Keep chatting to work out the instructions together."}
      {view.fleet ? <> <Link className="underline underline-offset-4" href={`/w/${workspace}/fleets/${view.fleet.id}`}>Open fleet</Link>.</> : null}
    </p>
    {attention ? <p className="text-label text-warning">Saved, but setup needs attention before relying on this fleet.</p> : null}
    {continueSetup ? <Button variant="secondary" size="sm" disabled={view.busy} onClick={() => {void client.retry().catch(() => undefined);}}>Continue setup</Button> : null}
    {view.fleet ? <ScheduleSync schedules={view.fleet.schedules} busy={view.busy} onRetry={(schedule) => {void client.syncSchedule(schedule).catch(() => undefined);}} /> : null}
  </Section>;
}
