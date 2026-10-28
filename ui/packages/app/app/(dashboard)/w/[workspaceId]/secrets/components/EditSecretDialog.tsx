"use client";

import { useState } from "react";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@agentsfleet/design-system";
import AddSecretForm, { SECRET_FORM_MODE } from "./AddSecretForm";
import { SecretValueNotice } from "./SecretValueNotice";

export type EditSecretDialogProps = {
  workspaceId: string;
  name: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

/** Stored values are write-only; replacement uses the same field builder as creation. */
export default function EditSecretDialog({workspaceId, name, open, onOpenChange}: EditSecretDialogProps) {
  const [pending, setPending] = useState(false);
  const close = () => {if (!pending) onOpenChange(false);};
  return <Dialog open={open} onOpenChange={close}>
    <DialogContent className="ph-no-capture ph-no-record" closeDisabled={pending}>
      <DialogHeader>
        <DialogTitle>Edit secret &ldquo;{name}&rdquo;</DialogTitle>
        <DialogDescription>
          Re-enter every field you want to keep. Saving replaces all fields. <SecretValueNotice />
        </DialogDescription>
      </DialogHeader>
      {open ? <AddSecretForm key={name} workspaceId={workspaceId} initialName={name} mode={SECRET_FORM_MODE.REPLACE}
        onPendingChange={setPending} onCancel={close} onDone={() => onOpenChange(false)} /> : null}
    </DialogContent>
  </Dialog>;
}
