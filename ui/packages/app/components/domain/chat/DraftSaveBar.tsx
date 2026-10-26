"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { Button } from "@agentsfleet/design-system";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { SetupSaveAction } from "./FleetSetupWorkspace";

const ignore = (): void => undefined;

/** Saving is explicit and remains reachable while the transcript grows. */
export function DraftSaveBar({client, view, dirty = false}: {client: DraftClient; view: DraftView; dirty?: boolean}) {
  if (view.draft?.review?.effective.target_setup_revision === undefined && !view.preview?.setupRevision) return null;
  const save = <DraftSaveButton client={client} view={view} dirty={dirty} />;
  return <SetupSaveAction fallback={<div className="mb-sm flex shrink-0 items-center justify-end gap-md" aria-label="Save fleet setup">
    {dirty ? <p className="mr-auto text-label text-muted-foreground">Update instructions before saving these changes.</p> : null}
    {save}
  </div>}>{save}</SetupSaveAction>;
}

export function DraftSaveButton({client, view, dirty = false}: {client: DraftClient; view: DraftView; dirty?: boolean}) {
  const draft = view.draft;
  if (view.inaccessible) return null;
  const recovery = chatRecovery(view);
  const updating = draft?.review?.effective.target_setup_revision !== undefined || Boolean(view.preview?.setupRevision);
  const disabled = dirty || !draft?.candidate || !draft.review || draft.state !== DRAFT_STATE.editing || view.busy || view.pending || recovery.pending || recovery.blocked;
  return <Button size="sm" disabled={disabled} title={dirty ? "Update instructions before saving these changes." : undefined} onClick={() => {void (updating ? client.apply() : client.save()).catch(ignore);}}>
      {updating ? "Save changes" : "Save fleet"}
    </Button>;
}
