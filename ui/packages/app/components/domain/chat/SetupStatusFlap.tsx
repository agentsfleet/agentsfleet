"use client";

import type { DraftView } from "@/lib/api/draft-state";
import type { LucideIcon } from "lucide-react";
import Link from "next/link";
import { ArrowRightIcon, BookOpenIcon, CheckIcon, CircleAlertIcon, CircleHelpIcon, ClockIcon, PencilIcon, SaveIcon, SparklesIcon, UploadIcon } from "lucide-react";
import { Button, SignalSpinner } from "@agentsfleet/design-system";
import { savedDraft } from "@/lib/api/draft-continuation";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { MESSAGE_STATE } from "@/lib/api/draft-types";
import { setupActivity } from "@/lib/api/draft-activity";
import { ACTIVITY_KIND, saveDoneText } from "@/lib/events/fleet-activity";

type Status = {label: string; icon: LucideIcon; busy?: boolean; confirmed?: boolean};
const ACTIONS = {reading: {label: "Reading setup", icon: BookOpenIcon}, sending: {label: "Sending request", icon: UploadIcon}, saving: {label: "Saving", icon: SaveIcon}, publishing: {label: "Publishing", icon: UploadIcon}};

/** Only accepted fleet revisions and publication receipts receive a success tick. */
export function SetupStatusFlap({view, workspace, dirty = false, onOpenFleet}: {view: DraftView; workspace: string; dirty?: boolean; onOpenFleet?: () => void}) {
  if (view.inaccessible) return null;
  const status = setupStatus(view, dirty);
  if (!status) return null;
  const publication = view.draft?.publication;
  const published = publication && publication.canonical_digest === view.draft.candidate?.canonical_digest;
  const query = published ? new URLSearchParams({library: publication.library_id, revision: publication.revision_id}) : null;
  const Icon = status.icon;
  return <div className="mb-sm flex justify-center">
    <output aria-label="Setup status" aria-live="polite" aria-atomic="true" data-setup-status={status.busy ? "working" : status.confirmed ? "confirmed" : "attention"}
      className="flex max-w-full flex-wrap items-center justify-center gap-x-sm gap-y-xs rounded-full border border-border bg-card px-lg py-sm text-label text-muted-foreground motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-bottom-1 motion-safe:duration-stream">
      {status.busy ? <SignalSpinner className="text-pulse" aria-hidden="true" /> : null}
      <Icon className={`size-3.5 shrink-0 ${status.confirmed ? "text-success" : ""}`} aria-hidden="true" />
      <span>{status.label}</span>
      {status.confirmed && view.fleet ? onOpenFleet
        ? <Button variant="link" size="eyebrow" className="gap-xs px-0" onClick={onOpenFleet}>Open fleet<ArrowRightIcon className="size-3.5" aria-hidden="true" /></Button>
        : <Link className="inline-flex items-center gap-xs hover:underline" href={`/w/${workspace}/fleets/${view.fleet.id}`}>Open fleet<ArrowRightIcon className="size-3.5" aria-hidden="true" /></Link> : null}
      {status.confirmed && query ? <Link className="inline-flex items-center gap-xs hover:underline" href={`/w/${workspace}/fleets/new?${query}`}>New fleet<ArrowRightIcon className="size-3.5" aria-hidden="true" /></Link> : null}
    </output>
  </div>;
}

function setupStatus(view: DraftView, dirty: boolean): Status | null {
  if (view.error) return {label: view.pending ? "Update not confirmed" : "Setup needs attention", icon: view.pending ? CircleHelpIcon : CircleAlertIcon};
  if (view.saveOperation) return {...ACTIONS.saving, busy: true};
  if (view.setupAction) return {...ACTIONS[view.setupAction], busy: true};
  if (view.optimistic) return {label: "Sending request", icon: UploadIcon, busy: true};
  if (chatRecovery(view).pending) {
    const refining = view.messages.some(message => message.state === MESSAGE_STATE.running);
    return {label: refining ? "Refining setup" : "Waiting to refine setup", icon: refining ? SparklesIcon : ClockIcon, busy: true};
  }
  if (dirty) return {label: "Unsaved setup changes", icon: PencilIcon};
  const revision = view.fleet?.setup_revision;
  if (revision && (!view.draft || savedDraft(view))) {
    const receipt = view.activities?.map(setupActivity).find(activity => activity?.kind === ACTIVITY_KIND.setupSaved && activity.revision === revision);
    return {label: saveDoneText(revision, receipt?.kind === ACTIVITY_KIND.setupSaved ? receipt.origin : undefined), icon: CheckIcon, confirmed: true};
  }
  if (view.draft?.publication && view.draft.publication.canonical_digest === view.draft.candidate?.canonical_digest) return {label: "Setup published", icon: CheckIcon, confirmed: true};
  if (view.draft?.candidate) return {label: revision ? "Unsaved setup changes" : "Setup ready to save", icon: PencilIcon};
  return null;
}
