"use client";

import type { ReactNode } from "react";
import type { FleetStatusSettable } from "@/lib/api/fleets";
import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { ArchiveIcon, DotsThreeIcon, StopCircleIcon, PlayIcon, TrashIcon } from "@phosphor-icons/react/ssr";
import { Button, ConfirmDialog, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@agentsfleet/design-system";
import { FleetLifecycleDialog } from "@/components/layout/FleetLifecycleDialog";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { presentErrorString, UNCONFIRMED_REQUEST_MESSAGE } from "@/lib/errors";
import { setFleetStatusAction } from "../../actions";

type Props = {
  children?: ReactNode; workspaceId: string;
  fleet: {id: string; name: string; status: string; etag: string};
};
type Control = {
  target: FleetStatusSettable; label: string; description: string;
  errorVerb: "stop this fleet" | "resume this fleet";
};
const STOP: Control = {target: "stopped", label: "Stop", description: "Temporarily stop execution. You can resume this fleet later.", errorVerb: "stop this fleet"};
const RESUME: Control = {target: "active", label: "Resume", description: "Return this fleet to active execution.", errorVerb: "resume this fleet"};

export function FleetActions({workspaceId, fleet, children}: Props) {
  const router = useRouter();
  const [control, setControl] = useState<Control | null>(null);
  const [lifecycle, setLifecycle] = useState<"archive" | "delete" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const owner = useRef<AbortController | null>(null);
  useEffect(() => () => owner.current?.abort(), [workspaceId, fleet.id]);
  const archived = fleet.status === AGENTSFLEET_STATUS.KILLED;
  const known = Object.values(AGENTSFLEET_STATUS).some(status => status === fleet.status);
  const available = fleet.status === AGENTSFLEET_STATUS.ACTIVE ? STOP
    : fleet.status === AGENTSFLEET_STATUS.STOPPED || fleet.status === AGENTSFLEET_STATUS.PAUSED ? RESUME : null;

  async function confirm(action: Control) {
    if (owner.current) return;
    const current = new AbortController(); owner.current = current; setError(null);
    try {
      const result = await setFleetStatusAction(workspaceId, fleet.id, action.target, fleet.etag);
      if (current.signal.aborted) return;
      if (result.ok && result.data.status === action.target) {setControl(null); router.refresh(); return;}
      if (!result.ok) {
        if (result.status === 409 || result.status === 412) router.refresh();
        setError(presentErrorString({errorCode: result.errorCode, message: result.error, action: action.errorVerb}));
      } else setError(UNCONFIRMED_REQUEST_MESSAGE);
    } catch {
      if (!current.signal.aborted) setError(UNCONFIRMED_REQUEST_MESSAGE);
    } finally {owner.current = null;}
  }

  return <>
    <DropdownMenu>
      <DropdownMenuTrigger asChild><Button variant="ghost" size="icon-compact" aria-label="Fleet actions"><DotsThreeIcon aria-hidden="true" /></Button></DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-60 max-w-full p-sm">
        {children}
        {children ? <DropdownMenuSeparator /> : null}
        {available ? <DropdownMenuItem onSelect={() => {setError(null); setControl(available);}}>
          {available === STOP ? <StopCircleIcon aria-hidden="true" /> : <PlayIcon aria-hidden="true" />}{available.label}
        </DropdownMenuItem> : null}
        {known && !archived ? <DropdownMenuItem onSelect={() => setLifecycle("archive")}><ArchiveIcon aria-hidden="true" />Archive</DropdownMenuItem> : null}
        {known ? <><DropdownMenuSeparator /><DropdownMenuItem className="text-destructive focus:text-destructive" onSelect={() => setLifecycle("delete")}><TrashIcon aria-hidden="true" />Permanently delete</DropdownMenuItem></> : null}
      </DropdownMenuContent>
    </DropdownMenu>
    <ConfirmDialog open={control !== null} onOpenChange={() => setControl(null)} title={control ? `${control.label} this fleet?` : ""}
      description={control?.description} confirmLabel={control?.label} intent={control === STOP ? "destructive" : "default"}
      onConfirm={control ? () => confirm(control) : undefined} errorMessage={error} />
    {lifecycle ? <FleetLifecycleDialog workspace={workspaceId} fleet={fleet} action={lifecycle}
      onClose={() => {setLifecycle(null); router.refresh();}} onRemoved={() => router.replace(`/w/${workspaceId}/fleets/new`)} /> : null}
  </>;
}
