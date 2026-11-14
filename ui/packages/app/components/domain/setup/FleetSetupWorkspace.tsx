"use client";

import type { ReactNode } from "react";
import { useMemo } from "react";
import { useCurrentUser } from "@/lib/auth/client";
import { FleetStreamProvider } from "../conversation/stream/FleetStreamProvider";
import { useFleetModel } from "../models/useFleetModel";
import { FleetSetupContext, type SetupLocation } from "./FleetSetupContext";

type WorkspaceProps = SetupLocation & {account: string | null; model?: string; viewer: string | null; header: ReactNode; children: ReactNode};

export function FleetSetupWorkspace({account, workspace, fleet, revision, model = "platform-default", viewer, header, children}: WorkspaceProps) {
  const {userId, isLoaded} = useCurrentUser();
  const subject = isLoaded ? userId : viewer;
  const modelOwner = useMemo(() => account && subject ? {account, workspace, subject, fleet, revision, model} : null,
    [account, workspace, subject, fleet, revision, model]);
  const modelState = useFleetModel(modelOwner);
  if (isLoaded && !userId) return null;
  return <FleetStreamProvider key={`${account}:${workspace}:${subject}`}>
    <FleetSetupContext value={{location: {workspace, fleet, revision}, modelOwner, modelState, modelChanging: modelState.saving || modelState.pending}}>
      {header}{children}
    </FleetSetupContext>
  </FleetStreamProvider>;
}

