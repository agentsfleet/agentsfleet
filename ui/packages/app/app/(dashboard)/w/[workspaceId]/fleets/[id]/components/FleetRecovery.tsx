"use client";

import type { Fleet } from "@/lib/api/draft-types";
import { lazy, Suspense } from "react";
import { scheduleOwed, setupRecovery } from "@/lib/api/schedule-status";

// Healthy fleets do not need recovery controls or the summary's schedule parser.
const FleetRecoveryContent = lazy(() => import("./FleetRecoveryContent").then((module) => ({default: module.FleetRecoveryContent})));

export function FleetRecovery({workspace, fleet, showSaved = false}: {workspace: string; fleet: Fleet; showSaved?: boolean}) {
  const setupPending = setupRecovery(fleet).pending;
  if (!showSaved && !setupPending && !fleet.schedules.some(scheduleOwed)) return null;
  return <Suspense fallback={null}>
    <FleetRecoveryContent workspace={workspace} fleet={fleet} setupPending={setupPending || showSaved} />
  </Suspense>;
}
