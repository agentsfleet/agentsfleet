"use client";

import { ConfirmDialog } from "@agentsfleet/design-system";
import type { SidebarDraft } from "@/lib/api/draft-summary";
import type { DeleteDraft } from "@/lib/api/draft-request";
import { useDeleteDraft } from "./use-delete-draft";

export type DraftDeleteDialogProps = {
  card: SidebarDraft;
  workspace: string;
  remove: DeleteDraft | null;
  onRemoved: (id: string) => void;
  onClose: () => void;
};

export function DraftDeleteDialog({card, workspace, remove, onRemoved, onClose}: DraftDeleteDialogProps) {
  const deletion = useDeleteDraft(workspace, remove, onRemoved);
  return <ConfirmDialog open onOpenChange={onClose} title="Delete setup chat?" confirmLabel="Delete chat" intent="destructive"
    description={`Delete “${card.title}” and its private conversation? Saved fleets and published library versions stay available.`}
    onConfirm={() => deletion.remove(card.id)} errorMessage={deletion.error} />;
}
