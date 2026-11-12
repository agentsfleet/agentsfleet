"use client";

import { useRef, useState } from "react";
import { Button, Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@agentsfleet/design-system";
import type { Secret } from "@/lib/api/secrets";
import { SECRET_KIND } from "@/lib/api/secrets-types";
import { providerLabel } from "@/lib/api/model-library-types";

type Props = {secret: Secret; pending: boolean; protectedFromDelete: boolean; onEdit: (name: string) => void; onDelete: (name: string) => void};

export function secretType(secret: Secret): string {
  return secret.kind === SECRET_KIND.custom_secret ? "Secret" : `${providerLabel(secret.provider)} key`;
}

export function SecretActions({secret, pending, protectedFromDelete, onEdit, onDelete}: Props) {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const nextAction = useRef<(() => void) | null>(null);
  function handoff(action: (name: string) => void) {
    nextAction.current = () => action(secret.name);
    setOpen(false);
  }
  return <Dialog open={open} onOpenChange={setOpen}>
    <DialogTrigger asChild><Button ref={trigger} variant="ghost" disabled={pending} aria-label={`Manage ${secret.name}`}>Manage</Button></DialogTrigger>
    <DialogContent onCloseAutoFocus={event => {
      const next = nextAction.current;
      nextAction.current = null;
      if (!next) return;
      event.preventDefault();
      // The next dialog must capture the lasting row action, not a button in
      // this closing dialog. Navigation or removal cancels the handoff.
      if (!trigger.current?.isConnected) return;
      trigger.current.focus();
      next();
    }}>
      <DialogHeader><DialogTitle>{secret.name}</DialogTitle><DialogDescription>
        Stored in this workspace. Fleet access details are not available here.
      </DialogDescription></DialogHeader>
      <p className="text-label text-muted-foreground">{secretType(secret)} · Value hidden</p>
      {protectedFromDelete ? <p className="text-eyebrow text-text-dim">This key is used by your active model. Choose another model before deleting it.</p> : null}
      <DialogFooter>
        <Button variant="outline" disabled={pending || protectedFromDelete} onClick={() => handoff(onDelete)}>Delete secret</Button>
        <Button variant="outline" disabled={pending} onClick={() => handoff(onEdit)}>Replace value</Button>
        <Button onClick={() => setOpen(false)}>Done</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}

export function SecretNameCell({secret}: {secret: Secret}) {
  return <span className="break-words font-mono text-mono font-normal leading-mono">{secret.name}</span>;
}
