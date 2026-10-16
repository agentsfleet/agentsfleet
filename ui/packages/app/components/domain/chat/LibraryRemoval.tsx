"use client";

import { useState } from "react";
import { ConfirmDialog, IconAction } from "@agentsfleet/design-system";
import { Trash2Icon } from "lucide-react";
import { removeLibraryEntryAction } from "@/app/(dashboard)/w/[workspaceId]/library/actions";
import { REMOVE_CONFIRM_LABEL, REMOVE_DIALOG_BODY, REMOVE_DIALOG_TITLE, REMOVE_ROW_LABEL } from "@/app/(dashboard)/w/[workspaceId]/library/copy";
import type { LibraryCard } from "@/lib/api/draft-library";
import { presentErrorString } from "@/lib/errors";

export function LibraryRemoval({ workspace, card, onRemoved }: { workspace: string; card: LibraryCard; onRemoved: () => void }) {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  return <>
    <IconAction variant="destructive" label={REMOVE_ROW_LABEL} onClick={() => { setError(null); setOpen(true); }}><Trash2Icon size={14} /></IconAction>
    <ConfirmDialog open={open} onOpenChange={setOpen} title={REMOVE_DIALOG_TITLE(card.name)} description={REMOVE_DIALOG_BODY}
      confirmLabel={REMOVE_CONFIRM_LABEL} intent="destructive" errorMessage={error}
      onError={() => setError("Removal could not be confirmed. Refresh the Library before trying again.")}
      onConfirm={async () => {
        const result = await removeLibraryEntryAction(workspace, card.id);
        if (!result.ok) {
          setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action: "remove the library entry" }));
          return;
        }
        setOpen(false); onRemoved();
      }} />
  </>;
}
