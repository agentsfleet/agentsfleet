"use client";

import type { ReactNode } from "react";
import type { WorkspaceModel } from "@/lib/api/workspace-models";
import type { FleetSettingsValue } from "./FleetSettings";
import { useMemo } from "react";
import { useCurrentUser } from "@/lib/auth/client";
import { runtimeModelLabel } from "@/lib/api/workspace-models";
import { FleetStreamProvider } from "../conversation/stream/FleetStreamProvider";
import { FleetModelContext, type FleetModelChoice } from "../models/FleetModelContext";
import { FleetSettingsProvider, useFleetSettingsOwner } from "./FleetSettings";
import { FleetListsProvider } from "./FleetListsProvider";

type Props = {
  account: string | null; workspace: string; fleet: FleetSettingsValue & {id: string};
  models: readonly WorkspaceModel[]; viewer: string | null; children: ReactNode;
};

/** The fleet page's client owners, keyed by who is signed in: its stream, its changeable fields and the lists beside its chat. */
export function FleetWorkspace({account, workspace, fleet, models, viewer, children}: Props) {
  const {userId, isLoaded} = useCurrentUser();
  const subject = isLoaded ? userId : viewer;
  const settings = useFleetSettingsOwner(workspace, fleet.id, fleet);
  const {value, pending, error, change} = settings;
  const choice = useMemo<FleetModelChoice>(() => ({
    models, selected: value.model_ref, label: runtimeModelLabel(models, value.model_ref), changing: pending === "model",
    error: error?.kind === "model" ? error.message : null,
    select: ref => {void change("model", {model_ref: ref});},
  }), [models, value.model_ref, pending, error, change]);
  if (isLoaded && !userId) return null;
  return <FleetStreamProvider key={`${account}:${workspace}:${subject}`}>
    <FleetSettingsProvider value={settings}><FleetModelContext value={choice}>
      <FleetListsProvider workspace={workspace} fleet={fleet.id}>{children}</FleetListsProvider>
    </FleetModelContext></FleetSettingsProvider>
  </FleetStreamProvider>;
}
