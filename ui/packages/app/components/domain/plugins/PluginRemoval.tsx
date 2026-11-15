"use client";

import { useState } from "react";
import { ConfirmDialog, Button } from "@agentsfleet/design-system";
import { TrashIcon } from "@phosphor-icons/react/ssr";
import { removePluginEntryAction } from "@/app/(dashboard)/w/[workspaceId]/plugins/actions";
import { REMOVE_CONFIRM_LABEL, REMOVE_DIALOG_BODY, REMOVE_DIALOG_TITLE, REMOVE_ROW_LABEL } from "@/app/(dashboard)/w/[workspaceId]/plugins/copy";
import type { PluginCard } from "@/lib/api/draft-plugin";
import { presentErrorString } from "@/lib/errors";

export function PluginRemoval({ workspace, card, onRemoved }: { workspace: string; card: PluginCard; onRemoved: () => void }) {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  return <>
    <Button variant="outline" aria-label={REMOVE_ROW_LABEL} onClick={() => { setError(null); setOpen(true); }}><TrashIcon size={16} />Delete Plugin</Button>
    <ConfirmDialog open={open} onOpenChange={setOpen} title={REMOVE_DIALOG_TITLE(card.name)} description={REMOVE_DIALOG_BODY}
      confirmLabel={REMOVE_CONFIRM_LABEL} intent="destructive" errorMessage={error}
      onError={() => setError("Removal could not be confirmed. Refresh the Plugin before trying again.")}
      onConfirm={async () => {
        const result = await removePluginEntryAction(workspace, card.id);
        if (!result.ok) {
          setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action: "remove the plugin entry" }));
          return;
        }
        setOpen(false); onRemoved();
      }} />
  </>;
}
