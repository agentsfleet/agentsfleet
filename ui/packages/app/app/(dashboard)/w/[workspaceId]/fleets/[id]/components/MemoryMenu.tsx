"use client";

import { useEffect, useId, useRef, useState } from "react";
import { BookOpenIcon } from "@phosphor-icons/react/ssr";
import nextDynamic from "next/dynamic";
import { Alert, Button, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle,
  DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, Spinner, SwitchIndicator } from "@agentsfleet/design-system";
import type { MemoryAccess, MemoryEntry } from "@/lib/types";
import { listFleetMemoriesAction, setMemoryAccessAction } from "../../actions";
import { presentErrorString } from "@/lib/errors";

type MemoryState = {kind: "loading"} | {kind: "ready"; entries: MemoryEntry[]} | {kind: "error"; message: string};
const MemoryPanel = nextDynamic(() => import("./MemoryPanel"), {loading: MemorySpinner});

const SAVED_MEMORIES = "Saved memories";
const GRANTS = [
  {grant: "read", label: "Read shared memories", description: "Use memories from other fleets."},
  {grant: "publish", label: "Share memories", description: "Let other fleets use what it shares."},
] as const;

export function MemoryMenu({sharing, onBrowse}: {sharing: ReturnType<typeof useMemorySharing>; onBrowse: () => void}) {
  const descriptionId = useId();
  const {access, notice, saving, flip} = sharing;
  return <DropdownMenuGroup>
    {access ? <>
      <DropdownMenuLabel variant="name" className="text-eyebrow">Workspace memory</DropdownMenuLabel>
      {GRANTS.map(({grant, label, description}) => <DropdownMenuItem key={grant} role="menuitemcheckbox"
        aria-label={label} aria-describedby={`${descriptionId}-${grant}`} aria-checked={access[grant]} disabled={saving}
        className="justify-between gap-sm" onSelect={event => {event.preventDefault(); void flip(grant);}}>
        <span className="min-w-0 flex-1"><span className="block font-semibold">{label}</span>
          <span id={`${descriptionId}-${grant}`} className="block text-eyebrow leading-eyebrow text-muted-foreground">{description}</span></span>
        <SwitchIndicator checked={access[grant]} />
      </DropdownMenuItem>)}
      {notice ? <Alert variant="destructive">{notice}</Alert> : null}
    </> : null}
    <DropdownMenuItem onSelect={onBrowse}>
      <BookOpenIcon aria-hidden="true" />{SAVED_MEMORIES}
    </DropdownMenuItem>
  </DropdownMenuGroup>;
}

export function useMemorySharing(workspaceId: string, fleetId: string, initial: MemoryAccess | undefined) {
  const [access, setAccess] = useState(initial);
  const [notice, setNotice] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const inFlight = useRef(false);
  useEffect(() => {if (!inFlight.current) setAccess(initial);}, [initial]);
  const flip = async (grant: keyof MemoryAccess): Promise<void> => {
    if (!access || inFlight.current) return;
    inFlight.current = true; setSaving(true); setNotice(null);
    try {
      const result = await setMemoryAccessAction(workspaceId, fleetId, {[grant]: !access[grant]});
      if (result.ok) setAccess(result.data);
      else setNotice(presentErrorString({message: result.error, errorCode: result.errorCode, action: "change memory sharing"}));
    } catch {setNotice("Memory sharing could not be changed. Try again.");}
    finally {inFlight.current = false; setSaving(false);}
  };
  return {access: initial ? access : undefined, notice, saving, flip};
}

export function SavedMemoriesDialog({workspaceId, fleetId, onClose, restoreFocus}: {
  workspaceId: string; fleetId: string; onClose: () => void; restoreFocus: () => void;
}) {
  return <Dialog open onOpenChange={onClose}>
    <DialogContent className="ph-no-capture ph-no-record flex max-w-2xl flex-col overflow-hidden p-0"
      onCloseAutoFocus={event => {event.preventDefault(); restoreFocus();}}>
      <DialogHeader className="shrink-0 gap-md px-lg py-lg sm:px-2xl">
        <DialogTitle>{SAVED_MEMORIES}</DialogTitle>
        <DialogDescription>Memories this fleet can use in future work.</DialogDescription>
      </DialogHeader>
      <div className="min-h-0 overflow-y-auto px-lg pb-2xl sm:px-2xl">
        <MemoryBrowser workspaceId={workspaceId} fleetId={fleetId} />
      </div>
    </DialogContent>
  </Dialog>;
}

function MemoryBrowser({workspaceId, fleetId}: {workspaceId: string; fleetId: string}) {
  const [state, setState] = useState<MemoryState>({kind: "loading"});
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let current = true;
    void listFleetMemoriesAction(workspaceId, fleetId).then(result => {
      if (!current) return;
      if (result.ok) setState({kind: "ready", entries: result.data.items});
      else setState({kind: "error", message: presentErrorString({message: result.error, errorCode: result.errorCode, action: "load saved memories"})});
    }).catch(() => {if (current) setState({kind: "error", message: "Saved memories could not be loaded."});});
    return () => {current = false;};
  }, [workspaceId, fleetId, attempt]);
  if (state.kind === "loading") return <MemorySpinner />;
  if (state.kind === "error") return <Alert variant="destructive">
    {state.message}<Button variant="link" onClick={() => {setState({kind: "loading"}); setAttempt(value => value + 1);}}>Try again</Button>
  </Alert>;
  return <MemoryPanel workspaceId={workspaceId} fleetId={fleetId} entries={state.entries} />;
}

function MemorySpinner() {
  return <div className="grid place-items-center rounded-control bg-background py-5xl">
    <Spinner size="lg" srLabel="Loading saved memories" />
  </div>;
}
