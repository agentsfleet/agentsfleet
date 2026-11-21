"use client";

import type { DraftView } from "@/lib/api/draft-state";
import type { Icon as PhosphorIcon } from "@phosphor-icons/react";
import Link from "next/link";
import { ArrowRightIcon, BookOpenIcon, CheckCircleIcon, WarningCircleIcon, QuestionIcon, ClockIcon, PencilSimpleIcon, FloppyDiskIcon, SparkleIcon, UploadSimpleIcon } from "@phosphor-icons/react/ssr";
import { Button } from "@agentsfleet/design-system";
import { savedDraft } from "@/lib/api/draft-continuation";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { MESSAGE_STATE } from "@/lib/api/draft-types";
import { SAVED_SETUP_LABEL } from "@/lib/events/fleet-activity";
import { DRAFT_LIST_STATUS } from "@/lib/api/draft-list-types";
import { ChatProgressLabel } from "../../shared/chat/ChatProgressLabel";

type Status = {label: string; icon: PhosphorIcon; busy?: boolean; confirmed?: boolean; link?: {href: string; label: string}};
const ACTIONS = {reading: {label: "Reading setup", icon: BookOpenIcon}, sending: {label: "Sending request", icon: UploadSimpleIcon}, saving: {label: "Saving", icon: FloppyDiskIcon}, publishing: {label: "Publishing", icon: UploadSimpleIcon}};

/** Only accepted fleet revisions and publication receipts receive a success tick. */
export function SetupStatusFlap({view, workspace, dirty = false, onOpenFleet, modelsHref, showPublication = false}: {view: DraftView; workspace: string; dirty?: boolean; onOpenFleet?: () => void; modelsHref?: string; showPublication?: boolean}) {
  if (view.inaccessible) return null;
  const status = setupStatus(view, dirty, modelsHref, showPublication);
  if (!status) return null;
  const publication = view.draft?.publication;
  const published = showPublication && publication && publication.canonical_digest === view.draft.candidate?.canonical_digest;
  const query = published ? new URLSearchParams({plugin: publication.plugin_id, revision: publication.revision_id}) : null;
  const Icon = status.icon;
  return <div className="mb-sm flex justify-center">
    <output aria-label="Setup status" aria-live="polite" aria-atomic="true" data-setup-status={status.busy ? "working" : status.confirmed ? "confirmed" : "attention"}
      className="flex max-w-full flex-wrap items-center justify-center gap-x-sm gap-y-xs rounded-full border border-border bg-card px-lg py-sm text-label text-muted-foreground motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-bottom-1 motion-safe:duration-stream">
      {status.busy ? <ChatProgressLabel label={status.label} /> : <><Icon className={`size-4 shrink-0 ${status.confirmed ? "text-success" : ""}`} aria-hidden="true" /><span>{status.label}</span></>}
      {status.confirmed && view.fleet ? onOpenFleet
        ? <Button variant="link" size="eyebrow" className="gap-xs px-0" onClick={onOpenFleet}>Open fleet<ArrowRightIcon className="size-4" aria-hidden="true" /></Button>
        : <Link className="inline-flex items-center gap-xs hover:underline" href={`/w/${workspace}/fleets/${view.fleet.id}`}>Open fleet<ArrowRightIcon className="size-4" aria-hidden="true" /></Link> : null}
      {status.link ? <Link className="inline-flex items-center gap-xs hover:underline" href={status.link.href}>{status.link.label}<ArrowRightIcon className="size-4" aria-hidden="true" /></Link> : null}
      {status.confirmed && query ? <Link className="inline-flex items-center gap-xs hover:underline" href={`/w/${workspace}/fleets/new?${query}`}>New fleet<ArrowRightIcon className="size-4" aria-hidden="true" /></Link> : null}
    </output>
  </div>;
}

function setupStatus(view: DraftView, dirty: boolean, modelsHref: string | undefined, showPublication: boolean): Status | null {
  if (view.error) return {label: view.pending ? "Update not confirmed" : "Setup needs attention", icon: view.pending ? QuestionIcon : WarningCircleIcon};
  if (view.saveOperation) return {...ACTIONS.saving, busy: true};
  if (view.setupAction) return {...ACTIONS[view.setupAction], busy: true};
  if (view.optimistic) return {label: "Sending request", icon: UploadSimpleIcon, busy: true};
  if (chatRecovery(view).pending) {
    const refining = view.messages.some(message => message.state === MESSAGE_STATE.running);
    return {label: refining ? "Refining setup" : "Waiting to refine setup", icon: refining ? SparkleIcon : ClockIcon, busy: true};
  }
  const unusableModels = modelStatus(view, modelsHref);
  if (unusableModels) return unusableModels;
  if (dirty) return {label: "Unsaved setup changes", icon: PencilSimpleIcon};
  const revision = view.fleet?.setup_revision;
  if (revision && (!view.draft || savedDraft(view))) {
    return {label: SAVED_SETUP_LABEL, icon: CheckCircleIcon, confirmed: true};
  }
  if (showPublication && view.draft?.publication && view.draft.publication.canonical_digest === view.draft.candidate?.canonical_digest) return {label: "Setup published", icon: CheckCircleIcon, confirmed: true};
  if (view.draft?.candidate) return {label: revision ? "Unsaved setup changes" : "Setup ready to save", icon: PencilSimpleIcon};
  return null;
}

/** Explains the disabled composer, and only once a successful list proves no model can author. */
function modelStatus(view: DraftView, modelsHref?: string): Status | null {
  if (view.listStatus !== DRAFT_LIST_STATUS.ready || view.models.some(model => model.eligibility.authoring)) return null;
  if (view.moreModels) return {label: "Load more models to find one for chat", icon: WarningCircleIcon};
  const link = modelsHref ? {href: modelsHref, label: "Set up model"} : undefined;
  return {label: view.models.length > 0 ? "No chat model available" : "No chat model yet", icon: WarningCircleIcon, link};
}
