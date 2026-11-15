"use client";

import { useRef, useState } from "react";
import { Button, Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@agentsfleet/design-system";
import type { Secret } from "@/lib/api/secrets";
import { fleetAccessOf, SECRET_KIND } from "@/lib/api/secrets-types";
import { providerLabel } from "@/lib/api/model-library-types";

type Props = {secret: Secret; pending: boolean; protectedFromDelete: boolean; onEdit: (name: string) => void; onDelete: (name: string) => void};

export function secretType(secret: Secret): string {
  return secret.kind === SECRET_KIND.custom_secret ? "Secret" : `${providerLabel(secret.provider)} key`;
}

const ACCESS_UNAVAILABLE = "Access details unavailable";
const VISIBLE_FLEETS = 2;

/** Recorded fleet access: names for a few, a count beyond that, and an explicit unavailable state. */
export function SecretAccessCell({secret}: {secret: Secret}) {
  const fleets = fleetAccessOf(secret);
  if (fleets === null) return <span className="text-text-dim">{ACCESS_UNAVAILABLE}</span>;
  if (fleets.length === 0) return <span className="text-text-dim">No active fleets</span>;
  const more = fleets.length - VISIBLE_FLEETS;
  return <span className="break-words">{fleets.slice(0, VISIBLE_FLEETS).map(fleet => fleet.name).join(", ")}{more > 0 ? ` +${more} more` : ""}</span>;
}

function accessSentence(secret: Secret): string {
  const fleets = fleetAccessOf(secret);
  if (fleets === null) return "Fleet access details are not available here.";
  if (fleets.length === 0) return "No active fleet's setup uses it.";
  return `Available to ${fleets.map(fleet => fleet.name).join(", ")}.`;
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
    <DialogTrigger asChild><Button ref={trigger} variant="ghost" size="compact" disabled={pending} aria-label={`Manage ${secret.name}`}>Manage</Button></DialogTrigger>
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
      <DialogHeader><DialogTitle>Manage secret</DialogTitle><DialogDescription>
        <span className="mb-md block font-medium text-foreground">“{secret.name}”</span>Stored in this workspace. {accessSentence(secret)}
      </DialogDescription></DialogHeader>
      <p className="text-label text-muted-foreground">{secretType(secret)} · Value hidden</p>
      {protectedFromDelete ? <p className="text-eyebrow text-text-dim">This key is used by your active model. Choose another model before deleting it.</p> : null}
      <DialogFooter className="flex-wrap">
        <Button variant="outline" className="px-lg" disabled={pending} onClick={() => handoff(onEdit)}>Replace value</Button>
        <Button variant="outline" className="px-lg" disabled={pending || protectedFromDelete} onClick={() => handoff(onDelete)}>Delete secret</Button>
        <Button onClick={() => setOpen(false)}>Done</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}

export function SecretNameCell({secret}: {secret: Secret}) {
  return <span className="break-words font-mono text-mono font-normal leading-mono">{secret.name}</span>;
}
