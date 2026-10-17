"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { useId, useState } from "react";
import { Alert, Button, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, Input, Label } from "@agentsfleet/design-system";
import { DRAFT_STATE, START_KIND, type Draft, type Review } from "@/lib/api/draft-types";
import { DefinitionEditor } from "./DefinitionEditor";
import { SetupProgress } from "./SetupProgress";
import { SetupReviewDetails } from "./SetupReviewDetails";

type ReviewedView = DraftView & {draft: Draft; reviewed: Review};

const ignore = (): void => undefined;

export function ReviewSetup({client, view, workspace, open, onOpenChange}: {client: DraftClient; view: DraftView; workspace: string; open: boolean; onOpenChange: (open: boolean) => void}) {
  const [editing, setEditing] = useState(false);
  const review = view.reviewed;
  return <Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent className="ph-no-capture ph-no-record max-h-screen overflow-y-auto sm:max-w-3xl">
      <DialogHeader><DialogTitle>Review fleet setup</DialogTitle><DialogDescription>The authoring model proposes this setup from your conversation. Check its instructions and access. Save creates a fleet; publishing shares the reusable setup with the workspace.</DialogDescription></DialogHeader>
      <div className="flex flex-wrap gap-sm"><Button variant="secondary" disabled={view.busy || (view.draft !== null && view.draft.state !== DRAFT_STATE.editing)} onClick={() => setEditing(!editing)}>{editing ? "Show review" : "Edit setup"}</Button>
        <Button variant="outline" disabled={view.busy || !view.draft?.candidate} onClick={() => { void client.review().catch(ignore); }}>Refresh review</Button></div>
      {view.error ? <Alert variant="destructive">{view.error}</Alert> : null}
      {view.notice ? <Alert>{view.notice}</Alert> : null}
      {view.fleet ? <SetupProgress fleet={view.fleet} workspace={workspace} busy={view.busy} onContinue={() => { void client.retry().catch(ignore); }}
        onRetrySchedule={(schedule) => { void client.syncSchedule(schedule).catch(ignore); }} />
        : view.busy && !view.draft ? <p className="text-sm text-muted-foreground">Loading selected setup…</p>
        : view.error && !view.draft && !view.preview ? null
        : editing || !view.draft ? <DefinitionEditor key={view.draft?.candidate?.id ?? view.preview?.revision ?? "blank"} client={client} view={view} onReviewed={() => setEditing(false)} />
        : review ? <ReviewBody client={client} view={{...view, draft: view.draft, reviewed: review}} /> : <p className="text-sm text-muted-foreground">Refresh review after the candidate changes. Save and Apply require the setup displayed here.</p>}
    </DialogContent>
  </Dialog>;
}

function ReviewBody({client, view}: {client: DraftClient; view: ReviewedView}) {
  const review = view.reviewed;
  return <div className="space-y-lg">
    <SetupReviewDetails review={review} />
    <SaveActions client={client} view={view} />
    <PublicationActions client={client} view={view} />
  </div>;
}

function SaveActions({client, view}: {client: DraftClient; view: ReviewedView}) {
  const inputId = useId();
  const [message, setMessage] = useState("");
  const disabled = view.busy || view.pending || view.draft.state !== DRAFT_STATE.editing;
  if (view.reviewed.effective.target_setup_revision !== undefined) return <Button disabled={disabled} onClick={() => { void client.apply().catch(ignore); }}>Apply reviewed setup</Button>;
  return <div className="space-y-sm">
    <Button disabled={disabled} onClick={() => { void client.save().catch(ignore); }}>Save fleet</Button>
    <div className="space-y-xs"><Label htmlFor={inputId}>Optional first run</Label><Input id={inputId} placeholder="What should the saved fleet do first?" value={message} onChange={(event) => setMessage(event.target.value)} /></div>
    <Button variant="secondary" disabled={disabled || !message.length} onClick={() => { void client.save({kind: START_KIND.runNow, message}).catch(ignore); }}>Save and run now</Button>
  </div>;
}

function PublicationActions({client, view}: {client: DraftClient; view: ReviewedView}) {
  const inputId = useId();
  const [label, setLabel] = useState(view.reviewed.effective.instance_name);
  const disabled = view.busy || view.pending || view.draft.state !== DRAFT_STATE.editing;
  return <div className="space-y-sm border-t border-border pt-lg">
    <h3 className="font-medium">Publish to Library</h3><p className="text-sm text-muted-foreground">Publish the reviewed instructions and configuration as an immutable setup. Your conversation stays private. Existing fleets keep their setup.</p>
    <div className="space-y-xs"><Label htmlFor={inputId}>Library name</Label><Input id={inputId} value={label} onChange={(event) => setLabel(event.target.value)} maxLength={100} /></div>
    <div className="flex flex-wrap gap-sm"><Button variant="outline" disabled={disabled || !label.length} onClick={() => { void client.publish(label, false).catch(ignore); }}>Publish new entry</Button>
      {view.draft.publication ? <Button variant="secondary" disabled={disabled} onClick={() => { void client.publish(label, true).catch(ignore); }}>Publish next revision</Button> : null}
      {client.canStagePlatform ? <Button variant="outline" disabled={disabled || !label.length} onClick={() => { void client.stagePlatform(label).catch(ignore); }}>Stage platform entry</Button> : null}</div>
  </div>;
}
