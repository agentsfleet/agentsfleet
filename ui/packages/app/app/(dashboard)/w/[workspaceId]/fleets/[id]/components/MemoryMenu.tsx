"use client";

import { useEffect, useId, useRef, useState } from "react";
import { BookOpenIcon, ChevronRightIcon } from "lucide-react";
import nextDynamic from "next/dynamic";
import { Alert, Button, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle,
  DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, SwitchIndicator } from "@agentsfleet/design-system";
import type { MemoryAccess, MemoryEntry } from "@/lib/types";
import { listFleetMemoriesAction, setMemoryAccessAction } from "../../actions";
import { presentErrorString } from "@/lib/errors";

const MemoryPanel = nextDynamic(() => import("./MemoryPanel"), {loading: () => <output>Loading memories…</output>});

type Props = {workspaceId: string; fleetId: string; access?: MemoryAccess; canGrant: boolean};
const SAVED_MEMORIES = "Saved memories";
const GRANTS = [
  {grant: "read", label: "Learn from this workspace", description: "Use memories shared by other fleets here."},
  {grant: "publish", label: "Share what this fleet learns", description: "Let other fleets here use its shared memories."},
] as const;

export function MemoryMenu({workspaceId, fleetId, access, canGrant}: Props) {
  const [open, setOpen] = useState(false);
  return <>
    <DropdownMenuSeparator />
    <DropdownMenuLabel variant="name">Workspace memory</DropdownMenuLabel>
    {access && canGrant ? <MemorySharing key={fleetId} workspaceId={workspaceId} fleetId={fleetId} initial={access} /> : null}
    <DropdownMenuItem onSelect={event => {event.preventDefault(); setOpen(true);}}>
      <BookOpenIcon aria-hidden="true" /><span className="flex-1">{SAVED_MEMORIES}</span><ChevronRightIcon aria-hidden="true" />
    </DropdownMenuItem>
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="ph-no-capture ph-no-record max-w-2xl overflow-y-auto">
        <DialogHeader><DialogTitle>{SAVED_MEMORIES}</DialogTitle>
          <DialogDescription>What this fleet remembers, including memories shared with this workspace.</DialogDescription></DialogHeader>
        {open ? <MemoryBrowser workspaceId={workspaceId} fleetId={fleetId} /> : null}
      </DialogContent>
    </Dialog>
  </>;
}

function MemorySharing({workspaceId, fleetId, initial}: {workspaceId: string; fleetId: string; initial: MemoryAccess}) {
  const descriptionId = useId();
  const [access, setAccess] = useState(initial);
  const [notice, setNotice] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const inFlight = useRef(false);
  useEffect(() => {if (!inFlight.current) setAccess(initial);}, [initial]);
  const flip = async (grant: keyof MemoryAccess): Promise<void> => {
    inFlight.current = true; setSaving(true); setNotice(null);
    try {
      const result = await setMemoryAccessAction(workspaceId, fleetId, {[grant]: !access[grant]});
      if (result.ok) setAccess(result.data);
      else setNotice(presentErrorString({message: result.error, errorCode: result.errorCode, action: "change memory sharing"}));
    } catch {setNotice("Memory sharing could not be changed. Try again.");}
    finally {inFlight.current = false; setSaving(false);}
  };
  return <>
    {GRANTS.map(({grant, label, description}) => <DropdownMenuItem key={grant} role="menuitemcheckbox"
      aria-label={label} aria-describedby={`${descriptionId}-${grant}`} aria-checked={access[grant]} disabled={saving}
      className="items-start gap-md py-md" onSelect={event => {event.preventDefault(); void flip(grant);}}>
      <span className="min-w-0 flex-1"><span className="block">{label}</span><span id={`${descriptionId}-${grant}`} className="mt-xs block text-label text-muted-foreground">{description}</span></span>
      <SwitchIndicator checked={access[grant]} />
    </DropdownMenuItem>)}
    {notice ? <Alert variant="destructive">{notice}</Alert> : null}
  </>;
}

function MemoryBrowser({workspaceId, fleetId}: {workspaceId: string; fleetId: string}) {
  const [entries, setEntries] = useState<MemoryEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let current = true;
    setLoading(true); setError(null);
    void listFleetMemoriesAction(workspaceId, fleetId).then(result => {
      if (!current) return;
      if (result.ok) setEntries(result.data.items);
      else setError(presentErrorString({message: result.error, errorCode: result.errorCode, action: "load saved memories"}));
    }).catch(() => {if (current) setError("Saved memories could not be loaded.");})
      .finally(() => {if (current) setLoading(false);});
    return () => {current = false;};
  }, [workspaceId, fleetId, attempt]);
  if (loading) return <output>Loading memories…</output>;
  if (error) return <Alert variant="destructive">{error}<Button variant="link" onClick={() => setAttempt(value => value + 1)}>Try again</Button></Alert>;
  return <MemoryPanel workspaceId={workspaceId} fleetId={fleetId} entries={entries} />;
}
