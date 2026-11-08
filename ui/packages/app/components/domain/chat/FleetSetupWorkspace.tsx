"use client";

import type { ReactNode } from "react";
import { createContext, useContext, useMemo } from "react";
import Link from "next/link";
import { PencilIcon } from "lucide-react";
import { ChatSummaryProvider } from "./ChatSummary";
import { Button } from "@agentsfleet/design-system";
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
      <ChatSummaryProvider>{header}{children}</ChatSummaryProvider>
    </SetupContext>
  </FleetStreamProvider>;
}

export function EditSetupButton(props: SetupLocation | {inline: true}) {
  const setup = useFleetSetup();
  const inline = "inline" in props;
  const location = inline ? setup?.location : props;
  if (!location) return <>Edit setup</>;
  const query = new URLSearchParams({fleet: location.fleet, setup_revision: location.revision});
  if (setup?.modelChanging) return <Button variant={inline ? "link" : "outline"} size={inline ? "sm" : "icon"} aria-label="Edit setup" disabled>{inline ? "Edit setup" : <PencilIcon size={16} aria-hidden="true" />}</Button>;
  return <Button variant={inline ? "link" : "outline"} size={inline ? "eyebrow" : "icon"} aria-label="Edit setup" title="Edit setup" asChild
    className={inline ? "h-auto px-0 align-baseline text-eyebrow" : undefined}>
    <Link href={`/w/${location.workspace}/fleets/new?${query}`}>{inline ? "Edit setup" : <PencilIcon size={16} aria-hidden="true" />}</Link>
  </Button>;
}
