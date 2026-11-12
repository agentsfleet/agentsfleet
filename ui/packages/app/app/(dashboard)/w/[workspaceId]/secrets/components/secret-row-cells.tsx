"use client";

import { useState } from "react";
import { Button, Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@agentsfleet/design-system";
import type { Secret } from "@/lib/api/secrets";
import { SECRET_KIND } from "@/lib/api/secrets-types";
import { providerLabel } from "@/lib/api/model-library-types";

type Props = {secret: Secret; pending: boolean; protectedFromDelete: boolean; onEdit: (name: string) => void; onDelete: (name: string) => void};

export function secretType(secret: Secret): string {
  return secret.kind === SECRET_KIND.custom_secret ? "Secret" : `${providerLabel(secret.provider)} key`;
}

export function SecretActions({secret, pending, protectedFromDelete, onEdit, onDelete}: Props) {
  const [open, setOpen] = useState(false);
  return <>
    <Button variant="ghost" disabled={pending} aria-label={`Manage ${secret.name}`} onClick={() => setOpen(true)}>Manage</Button>
    <Dialog open={open} onOpenChange={setOpen}><DialogContent>
      <DialogHeader><DialogTitle>{secret.name}</DialogTitle><DialogDescription>
        Stored in this workspace. Fleet access details are not available here.
      </DialogDescription></DialogHeader>
      <p className="text-label text-muted-foreground">{secretType(secret)} · Value hidden</p>
      {protectedFromDelete ? <p className="text-eyebrow text-text-dim">This key is used by your active model. Choose another model before deleting it.</p> : null}
      <DialogFooter>
        <Button variant="outline" disabled={pending || protectedFromDelete} onClick={() => {setOpen(false); onDelete(secret.name);}}>Delete secret</Button>
        <Button variant="outline" disabled={pending} onClick={() => {setOpen(false); onEdit(secret.name);}}>Replace value</Button>
        <Button onClick={() => setOpen(false)}>Done</Button>
      </DialogFooter>
    </DialogContent></Dialog>
  </>;
}

export function SecretNameCell({secret}: {secret: Secret}) {
  return <span className="break-words font-mono text-mono font-normal leading-mono">{secret.name}</span>;
}
