"use client";

import type { ReactNode } from "react";
import { createContext, useContext, useMemo } from "react";
import Link from "next/link";
import { NotePencilIcon } from "@phosphor-icons/react/ssr";
import { Button, DropdownMenuItem } from "@agentsfleet/design-system";
import { useCurrentUser } from "@/lib/auth/client";
import { FleetStreamProvider } from "../FleetStreamProvider";
import { useFleetModel, type FleetModelOwner } from "./useFleetModel";

type SetupLocation = {workspace: string; fleet: string; revision: string};
type WorkspaceProps = SetupLocation & {account: string | null; model?: string; viewer: string | null; header: ReactNode; children: ReactNode};
type FleetSetupState = {location: SetupLocation; modelOwner: FleetModelOwner | null; modelChanging: boolean; modelState: ReturnType<typeof useFleetModel>};
const SetupContext = createContext<FleetSetupState | null>(null);
export const useFleetSetup = () => useContext(SetupContext);

export function FleetSetupWorkspace({account, workspace, fleet, revision, model = "platform-default", viewer, header, children}: WorkspaceProps) {
  const {userId, isLoaded} = useCurrentUser();
  const subject = isLoaded ? userId : viewer;
  const modelOwner = useMemo(() => account && subject ? {account, workspace, subject, fleet, revision, model} : null,
    [account, workspace, subject, fleet, revision, model]);
  const modelState = useFleetModel(modelOwner);
  if (isLoaded && !userId) return null;
  return <FleetStreamProvider key={`${account}:${workspace}:${subject}`}>
    <SetupContext value={{location: {workspace, fleet, revision}, modelOwner, modelState, modelChanging: modelState.saving || modelState.pending}}>
      {header}{children}
    </SetupContext>
  </FleetStreamProvider>;
}

export function EditSetupButton(props: SetupLocation | {inline: true}) {
  const setup = useFleetSetup();
  const inline = "inline" in props;
  const variant = inline ? "link" : "outline";
  const location = inline ? setup?.location : props;
  if (!location) return <>Edit setup</>;
  if (setup?.modelChanging) return <Button variant={variant} size={inline ? "sm" : "icon"} aria-label="Edit setup" disabled>{inline ? "Edit setup" : <NotePencilIcon size={16} aria-hidden="true" />}</Button>;
  return <Button variant={variant} size={inline ? "eyebrow" : "icon"} aria-label="Edit setup" title={inline ? undefined : "Edit setup"} asChild
    className={inline ? "h-auto px-0 align-baseline text-eyebrow" : undefined}>
    <Link href={editSetupHref(location)}>{inline ? "Edit setup" : <NotePencilIcon size={16} aria-hidden="true" />}</Link>
  </Button>;
}

export function EditSetupMenuItem(location: SetupLocation) {
  const disabled = useFleetSetup()?.modelChanging ?? false;
  return <DropdownMenuItem asChild disabled={disabled}><Link href={editSetupHref(location)}>
    <NotePencilIcon size={16} aria-hidden="true" />Edit setup
  </Link></DropdownMenuItem>;
}

function editSetupHref(location: SetupLocation): string {
  const query = new URLSearchParams({fleet: location.fleet, setup_revision: location.revision});
  return `/w/${location.workspace}/fleets/new?${query}`;
}
