"use client";

import type {Schedule} from "@/lib/api/draft-types";
import {useFleetDurableTaskView} from "../tasks/FleetTaskProvider";
import {RecordedWakeups} from "./RecordedWakeups";

const EMPTY: readonly Schedule[] = [];
export function WakeupStrip() {
  const view = useFleetDurableTaskView();
  return view?.subject ? <RecordedWakeups key={`${view.subject}:${view.workspaceId}:${view.fleetId}`}
    workspace={view.workspaceId} schedules={view.schedules ?? EMPTY} /> : null;
}
