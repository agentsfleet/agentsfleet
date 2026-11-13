"use client";

import type { ReactNode } from "react";
import { useState } from "react";
import { useRouter } from "next/navigation";
import nextDynamic from "next/dynamic";
import { ArchiveIcon, DotsThreeIcon, TrashIcon } from "@phosphor-icons/react/ssr";
import { Button, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@agentsfleet/design-system";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";

type Props = {
  children?: ReactNode; workspaceId: string;
  fleet: {id: string; name: string; status: string; etag: string};
};
const FleetLifecycleDialog = nextDynamic(() => import("@/components/layout/FleetLifecycleDialog").then(module => module.FleetLifecycleDialog), {ssr: false});

export function FleetActions({workspaceId, fleet, children}: Props) {
  const router = useRouter();
  const [lifecycle, setLifecycle] = useState<"archive" | "delete" | null>(null);
  const archived = fleet.status === AGENTSFLEET_STATUS.ARCHIVED;
  const known = Object.values(AGENTSFLEET_STATUS).some(status => status === fleet.status);
  return <>
    <DropdownMenu>
      <DropdownMenuTrigger asChild><Button variant="ghost" size="icon-header" aria-label="Fleet actions"><DotsThreeIcon aria-hidden="true" /></Button></DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-60 max-w-full p-sm">
        {children}
        {children ? <DropdownMenuSeparator /> : null}
        {known && !archived ? <DropdownMenuItem onSelect={() => setLifecycle("archive")}><ArchiveIcon aria-hidden="true" />Archive</DropdownMenuItem> : null}
        {known ? <><DropdownMenuSeparator /><DropdownMenuItem className="text-destructive focus:text-destructive" onSelect={() => setLifecycle("delete")}><TrashIcon aria-hidden="true" />Permanently delete</DropdownMenuItem></> : null}
      </DropdownMenuContent>
    </DropdownMenu>
    {lifecycle ? <FleetLifecycleDialog workspace={workspaceId} fleet={fleet} action={lifecycle}
      onClose={() => {setLifecycle(null); router.refresh();}} onRemoved={() => router.replace(`/w/${workspaceId}/fleets/new`)} /> : null}
  </>;
}
