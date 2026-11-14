"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { Alert, Button, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger, Section } from "@agentsfleet/design-system";
import { SlidersHorizontalIcon } from "@phosphor-icons/react/ssr";
import { savedDraft } from "@/lib/api/draft-continuation";
import { SetupSummary } from "./SetupSummary";
import { FleetSetupDetails } from "./FleetSetupDetails";
import { ScheduleSync } from "./ScheduleSync";

/** Review is a projection of the existing draft; opening it never starts an edit. */
export function SetupReview({view, client}: {view: DraftView; client: DraftClient}) {
  const proposed = Boolean(view.draft?.candidate && !savedDraft(view));
  const review = proposed ? view.draft?.review : undefined;
  const preview = !view.fleet || view.preview?.setupRevision === view.fleet.setup_revision ? view.preview : null;
  const definition = proposed ? view.draft?.candidate?.definition : view.fleet?.definition ?? preview?.definition;
  const name = review?.effective.instance_name ?? view.fleet?.name ?? preview?.instanceName;
  const sourceRevision = review?.effective.source_revision ?? (!view.fleet ? preview?.revision : undefined);
  const status = proposed ? "Proposed setup · not saved" : view.fleet ? `Saved setup · revision ${view.fleet.setup_revision}` : "Setup preview";
  return <div className="space-y-lg">
    <div className="space-y-xs">
      <p className="break-words text-label font-semibold">{name ?? "New fleet"}</p>
      <p className="text-eyebrow text-muted-foreground">{status}</p>
    </div>
    <SetupSummary definition={definition} compact />
    <Dialog>
      <DialogTrigger asChild><Button variant="outline" className="w-full"><SlidersHorizontalIcon size={16} aria-hidden="true" />Review setup</Button></DialogTrigger>
      <DialogContent className="ph-no-capture ph-no-record bottom-0 left-auto right-0 top-0 flex h-dvh max-h-dvh w-full max-w-none translate-x-0 translate-y-0 flex-col rounded-none sm:max-w-form">
        <DialogHeader><DialogTitle>Fleet setup</DialogTitle><DialogDescription>{status}. Review the configuration before saving.</DialogDescription></DialogHeader>
        <div className="min-h-0 flex-1 space-y-2xl overflow-y-auto pr-xs pb-xl">
          <FleetSetupDetails review={review} definition={definition} name={name} revision={view.fleet?.setup_revision} sourceRevision={sourceRevision} />
          <Section aria-label="Saved schedules" className="gap-lg">
            <h3 className="text-label font-semibold">Saved schedules</h3>
            {view.error ? <Alert variant="destructive">{view.error}</Alert> : null}
            {view.fleet?.schedules.length ? <ScheduleSync schedules={view.fleet.schedules} busy={view.busy || view.pending || view.inaccessible}
              onRetry={schedule => {void client.syncSchedule(schedule).catch(() => undefined);}} />
              : <p className="text-label text-muted-foreground">{view.fleet ? "No schedules saved for this fleet." : "Schedules appear here after you save the fleet."}</p>}
            {proposed ? <p className="text-label text-muted-foreground">Proposed timing changes take effect after saving and scheduler registration.</p> : null}
          </Section>
        </div>
        <p className="border-t border-border pt-lg text-label text-muted-foreground">Describe changes in the setup chat. Use Edit instructions to change the name and instructions, then save when you’re ready.</p>
      </DialogContent>
    </Dialog>
  </div>;
}
