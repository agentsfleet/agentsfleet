"use client";

import type { FleetDetail } from "@/lib/types";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import nextDynamic from "next/dynamic";
import { ArchiveIcon, DotsThreeIcon, PencilSimpleIcon, TrashIcon } from "@phosphor-icons/react/ssr";
import { Button, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@agentsfleet/design-system";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { useFleetSettings } from "@/components/domain/fleet/FleetSettings";
import { FleetTitle, useFleetRename } from "./FleetTitle";
import { MemoryMenu, SavedMemoriesDialog, useMemorySharing } from "./MemoryMenu";

type Props = {
  workspaceId: string; canGrant?: boolean;
  fleet: Pick<FleetDetail, "id" | "name" | "status" | "memory_access">;
};
type FleetDialog = "archive" | "delete" | "memory";
const FleetLifecycleDialog = nextDynamic(() => import("@/components/layout/FleetLifecycleDialog").then(module => module.FleetLifecycleDialog), {ssr: false});

export function FleetActions({workspaceId, fleet, canGrant = false}: Props) {
  const router = useRouter();
  const [dialog, setDialog] = useState<FleetDialog | null>(null);
  const actions = useRef<HTMLButtonElement>(null);
  const afterMenu = useRef<(() => void) | null>(null);
  const active = fleet.status === AGENTSFLEET_STATUS.ACTIVE;
  const canDelete = active || fleet.status === AGENTSFLEET_STATUS.ARCHIVED;
  const settings = useFleetSettings();
  const name = settings?.value.name ?? fleet.name;
  const rename = useFleetRename(name, active && canGrant ? settings : null);
  const sharing = useMemorySharing(workspaceId, fleet.id, active && canGrant ? fleet.memory_access : undefined);
  return <>
    <FleetTitle name={name} rename={rename} />
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button ref={actions} variant="ghost" size="icon-header" aria-label="Fleet actions" title="Fleet actions"
          className="group/fleet-actions hover:bg-transparent focus-visible:ring-0 focus-visible:ring-offset-0">
          <span className="grid size-8 place-items-center rounded-control group-hover/fleet-actions:bg-muted group-data-[state=open]/fleet-actions:bg-muted group-focus-visible/fleet-actions:ring-2 group-focus-visible/fleet-actions:ring-ring">
            <DotsThreeIcon aria-hidden="true" />
          </span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" collisionPadding={8}
        className="max-h-[var(--radix-dropdown-menu-content-available-height)] w-76 max-w-[var(--radix-dropdown-menu-content-available-width)] overflow-y-auto p-xs"
        onCloseAutoFocus={event => {
          const enter = afterMenu.current;
          afterMenu.current = null;
          if (enter) {event.preventDefault(); enter();}
        }}>
        {rename.canRename ? <>
          <DropdownMenuItem onSelect={() => {afterMenu.current = rename.begin;}}><PencilSimpleIcon aria-hidden="true" />Rename fleet</DropdownMenuItem>
          <DropdownMenuSeparator className="mx-sm" />
        </> : null}
        {active ? <>
          <MemoryMenu sharing={sharing} onBrowse={() => {afterMenu.current = () => setDialog("memory");}} />
          <DropdownMenuSeparator className="mx-sm" />
          <DropdownMenuItem onSelect={() => setDialog("archive")}><ArchiveIcon aria-hidden="true" />Archive</DropdownMenuItem>
        </> : null}
        {canDelete ? <DropdownMenuItem className="text-destructive focus:text-destructive" onSelect={() => setDialog("delete")}>
          <TrashIcon aria-hidden="true" />Permanently delete
        </DropdownMenuItem> : null}
      </DropdownMenuContent>
    </DropdownMenu>
    {dialog === "memory" ? <SavedMemoriesDialog workspaceId={workspaceId} fleetId={fleet.id} onClose={() => setDialog(null)}
      restoreFocus={() => actions.current?.focus({preventScroll: true})} /> : null}
    {dialog === "archive" || dialog === "delete" ? <FleetLifecycleDialog workspace={workspaceId} fleet={{id: fleet.id, name}} action={dialog}
      onClose={() => {setDialog(null); router.refresh();}} onRemoved={() => router.replace(`/w/${workspaceId}/fleets/new`)} /> : null}
  </>;
}
