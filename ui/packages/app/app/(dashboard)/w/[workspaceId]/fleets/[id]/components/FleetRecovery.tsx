"use client";

import type { Fleet } from "@/lib/api/draft-types";
import { lazy, Suspense } from "react";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { START_KIND } from "@/lib/api/draft-vocabulary";
import { scheduleOwed } from "@/lib/api/schedule-status";

// Healthy fleets do not need recovery controls or the summary's schedule parser.
const FleetRecoveryContent = lazy(() => import("./FleetRecoveryContent").then((module) => ({default: module.FleetRecoveryContent})));

export function FleetRecovery({workspace, fleet, showSaved = false}: {workspace: string; fleet: Fleet; showSaved?: boolean}) {
  const setupPending = fleet.status === AGENTSFLEET_STATUS.INSTALLING || fleet.status === AGENTSFLEET_STATUS.ACTIVE &&
    fleet.start.kind === START_KIND.runNow && fleet.start.state !== "accepted";
  if (!showSaved && !setupPending && !fleet.schedules.some(scheduleOwed)) return null;
  return <Suspense fallback={null}>
    <FleetRecoveryContent workspace={workspace} fleet={fleet} setupPending={setupPending || showSaved} />
  </Suspense>;
}
