"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { Button } from "@agentsfleet/design-system";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { chatRecovery } from "@/lib/api/draft-recovery";

const ignore = (): void => undefined;

/** Saving is explicit and remains reachable while the transcript grows. */
export function DraftSaveBar({client, view}: {client: DraftClient; view: DraftView}) {
  if (view.draft?.review?.effective.target_setup_revision === undefined) return null;
  return <div className="flex shrink-0 items-center justify-between gap-md pt-sm" aria-label="Save fleet setup">
    <p className="text-label text-muted-foreground">Changes apply to new runs after saving.</p>
    <DraftSaveButton client={client} view={view} />
  </div>;
}

export function DraftSaveButton({client, view}: {client: DraftClient; view: DraftView}) {
  const draft = view.draft;
  if (!draft?.candidate || !draft.review || draft.state !== DRAFT_STATE.editing || view.inaccessible) return null;
  const recovery = chatRecovery(view);
  const updating = draft.review.effective.target_setup_revision !== undefined;
  const disabled = view.busy || view.pending || recovery.pending || recovery.blocked;
  return <Button size="sm" disabled={disabled} onClick={() => {void (updating ? client.apply() : client.save()).catch(ignore);}}>
      {updating ? "Save changes" : "Save fleet"}
    </Button>;
}
