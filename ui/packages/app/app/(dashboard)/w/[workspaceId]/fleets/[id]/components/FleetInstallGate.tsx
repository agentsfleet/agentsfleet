"use client";

import { useRouter } from "next/navigation";
import type { ReactNode } from "react";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import { workspacePath } from "@/lib/workspace-routes";
import { InstallStreamSteps } from "../../new/InstallStreamSteps";
import { InstallShell } from "../../new/install-state-list";

type Props = {
  workspaceId: string;
  fleetId: string;
  fleetName: string;
  // The fleet's status as server-rendered. When it is `installing` this gate
  // shows the live install states first; any other status reveals the fleet's
  // full surface directly.
  status: string;
  children: ReactNode;
};

// Detail-page gate: a still-provisioning fleet shows the live install states
// here first — the same SSE stream + step model the install page uses, so
// "Open fleet" on a fleet that is still installing lands here and resolves in
// place. InstallStreamSteps drives the steps; on `install:ready` it surfaces
// "Open fleet", which refreshes the server data so the page re-renders with the
// fleet now `active` and its full surface (the steer/chat + panels) revealed.
export function FleetInstallGate({ workspaceId, fleetId, fleetName, status, children }: Props) {
  const router = useRouter();

  if (status !== AGENTSFLEET_STATUS.INSTALLING) return <>{children}</>;

  return (
    <InstallShell title={`installing · ${fleetName}`} onBack={() => router.push(workspacePath(workspaceId, "fleets"))}>
      <InstallStreamSteps
        workspaceId={workspaceId}
        fleetId={fleetId}
        fleetName={fleetName}
        onOpen={() => router.refresh()}
      />
    </InstallShell>
  );
}
