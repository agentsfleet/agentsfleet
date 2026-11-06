"use client";

import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import type { TaskActivityHistory } from "@/lib/task-activity";
import { taskActivityLabel } from "@/lib/task-activity";

/** Only explicit task identity associates an event's observations with this work. */
export function workActivity(events: readonly FleetEvent[], task: string): TaskActivityHistory {
  // Observation sequences order one event; never compare them across events.
  const event = events.findLast(event => event.taskActivity?.items.some(item => item.progress.task_id === task));
  return {items: event?.taskActivity?.items.filter(item => item.progress.task_id === task) ?? [],
    truncated: event?.taskActivity?.truncated ?? false};
}
export function WorkAction({history}: {history: TaskActivityHistory}) {
  const latest = history.items.at(-1);
  return <span className="block pt-xs text-eyebrow text-muted-foreground">{latest
    ? `Latest recorded action: ${taskActivityLabel(latest)}`
    : "Open for the objective, saved evidence and next dependency."}</span>;
}
