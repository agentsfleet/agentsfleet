"use client";

import type { ReactNode } from "react";
import type { WorkspaceModel } from "@/lib/api/workspace-models";
import type { FleetSettingsValue } from "./FleetSettings";
import type { FleetListSeed } from "./FleetListsProvider";
import { useEffect, useMemo, useRef } from "react";
import { useRouter } from "next/navigation";
import { RouteLoading } from "@/components/layout/RouteLoading";
import { useCurrentUser } from "@/lib/auth/client";
import { runtimeModelLabel } from "@/lib/api/workspace-models";
import { FleetStreamProvider } from "../conversation/stream/FleetStreamProvider";
import { FleetModelContext, type FleetModelChoice } from "../models/FleetModelContext";
import { FleetSettingsProvider, useFleetSettingsOwner } from "./FleetSettings";
import { FleetListsProvider } from "./FleetListsProvider";

type Props = {
  account: string | null; workspace: string; fleet: FleetSettingsValue & FleetListSeed & {id: string};
  models: readonly WorkspaceModel[]; viewer: string | null; children: ReactNode;
};

/** The fleet page's client owners, keyed by who is signed in: its stream, its changeable fields and the lists beside its chat. */
export function FleetWorkspace({account, workspace, fleet, models, viewer, children}: Props) {
  const {userId, isLoaded} = useCurrentUser();
  const subject = isLoaded ? userId : viewer;
  if (isLoaded && !userId) return null;
  if (viewer && subject !== viewer) return <RefreshViewer key={subject} />;
  return <FleetWorkspaceContent key={`${account}:${workspace}:${subject}:${fleet.id}`} workspace={workspace} fleet={fleet} models={models}
    initialLists={subject !== null && subject === viewer ? fleet : undefined}>
    {children}
  </FleetWorkspaceContent>;
}

/** Replace the whole previous viewer's page, including the server-supplied chat and header. */
function RefreshViewer() {
  const router = useRouter();
  const requested = useRef(false);
  useEffect(() => {
    if (!requested.current) {
      requested.current = true;
      router.refresh();
    }
  }, [router]);
  return <RouteLoading />;
}

function FleetWorkspaceContent({workspace, fleet, models, children, initialLists}: Omit<Props, "account" | "viewer"> & {initialLists?: FleetListSeed}) {
  const settings = useFleetSettingsOwner(workspace, fleet.id, fleet);
  const {value, pending, error, change} = settings;
  const choice = useMemo<FleetModelChoice>(() => ({
    models, selected: value.model_ref, label: runtimeModelLabel(models, value.model_ref), changing: pending === "model",
    error: error?.kind === "model" ? error.message : null,
    select: ref => {void change("model", {model_ref: ref});},
  }), [models, value.model_ref, pending, error, change]);
  return <FleetStreamProvider>
    <FleetSettingsProvider value={settings}><FleetModelContext value={choice}>
      <FleetListsProvider key={`${!!initialLists?.schedules}:${!!initialLists?.triggers}:${!!initialLists?.grants}`}
        workspace={workspace} fleet={fleet.id} initial={initialLists}>{children}</FleetListsProvider>
    </FleetModelContext></FleetSettingsProvider>
  </FleetStreamProvider>;
}
