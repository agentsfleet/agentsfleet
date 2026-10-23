"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import Link from "next/link";
import { Button, Section } from "@agentsfleet/design-system";
import { START_KIND, scheduleOwed } from "@/lib/api/draft-types";
import { ScheduleSync } from "./ScheduleSync";

export function FleetSetup({client, view, workspace}: {client: DraftClient; view: DraftView; workspace: string}) {
  if (view.inaccessible || (!view.fleet && !view.draft?.candidate && !view.preview)) return null;
  const setup = view.draft?.review;
  const revision = view.fleet?.setup_revision ?? view.savedSetupRevision ?? setup?.effective.target_setup_revision ?? view.preview?.setupRevision;
  const attention = Boolean(view.fleet && (view.fleet.status !== "active" || view.fleet.schedules.some(scheduleOwed)
    || view.fleet.start.kind === START_KIND.runNow && view.fleet.start.state !== "accepted"));
  const runtime = setup?.effective.runtime_model_ref;
  const modelLabel = (ref: string): string => view.models.find((model) => model.ref === ref)?.label ?? ref;
  const continueSetup = Boolean(view.fleet && (view.fleet.status !== "active"
    || view.fleet.start.kind === START_KIND.runNow && view.fleet.start.state !== "accepted"));
  return <Section className="ph-no-capture ph-no-record my-md gap-xs border-l-2 border-border pl-md py-xs" aria-label="Fleet setup">
    <h2 className="text-body font-semibold">{revision ? `Saved v${revision}` : setup ? "Setup ready · Unsaved" : "Draft setup"}</h2>
    <p className="text-label text-muted-foreground">{revision
      ? `New runs use v${revision}. Runs already started keep their version.`
      : setup || view.preview ? "Save fleet from the header when ready." : "Continue chatting to finish these instructions before saving."}
      {!revision ? <> Proposed runtime: {runtime ? modelLabel(runtime) : "Not yet resolved"}. Setup chat: {view.model ? modelLabel(view.model) : "Not selected"}.</> : null}
      {" "}Edit instructions manually from the chat menu.
      {view.fleet ? <> <Link className="underline underline-offset-4" href={`/w/${workspace}/fleets/${view.fleet.id}`}>Open fleet</Link>.</> : null}
    </p>
    {attention ? <p className="text-label text-warning">Saved, but setup needs attention before relying on this fleet.</p> : null}
    {continueSetup ? <Button variant="secondary" size="sm" disabled={view.busy} onClick={() => {void client.retry().catch(() => undefined);}}>Continue setup</Button> : null}
    {view.fleet ? <ScheduleSync schedules={view.fleet.schedules} busy={view.busy} onRetry={(schedule) => {void client.syncSchedule(schedule).catch(() => undefined);}} /> : null}
  </Section>;
}
