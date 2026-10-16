"use client";

import type { Fleet } from "@/lib/api/draft-types";
import Link from "next/link";
import { Button, DashboardPanel, DashboardPanelTitle } from "@agentsfleet/design-system";
import { START_KIND } from "@/lib/api/draft-types";
import { SetupChecks } from "./SetupChecks";

export function SetupProgress({fleet, workspace, onContinue, busy}: {fleet: Fleet; workspace: string; onContinue: () => void; busy: boolean}) {
  const active = fleet.status === "active";
  return <DashboardPanel className="space-y-md" aria-label="Saved fleet setup">
    <DashboardPanelTitle>{active ? `${fleet.name} is ready` : `${fleet.name} setup is waiting`}</DashboardPanelTitle>
    <SetupChecks checks={fleet.setup.checks} label="Saved fleet readiness" />
    {fleet.start.kind === START_KIND.runNow ? <p className="text-sm">Initial run: {fleet.start.state}.
      {fleet.start.state === "accepted" ? " Accepted work can still be running." : " Setup and durable admission must finish before acceptance."}</p> : null}
    <div className="flex flex-wrap gap-sm">
      {!active || fleet.start.kind === START_KIND.runNow && fleet.start.state !== "accepted" ? <Button onClick={onContinue} disabled={busy}>Continue setup</Button> : null}
      <Button variant="outline" asChild><Link href={`/w/${workspace}/fleets/${fleet.id}`}>Open fleet</Link></Button>
    </div>
  </DashboardPanel>;
}
