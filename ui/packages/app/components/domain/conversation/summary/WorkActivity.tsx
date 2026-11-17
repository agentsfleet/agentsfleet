import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import type { TaskActivityHistory } from "@/lib/task-activity";

/** Only explicit task identity associates an event's observations with this work. */
export function workActivity(events: readonly FleetEvent[], task: string): TaskActivityHistory {
  // Observation sequences order one event; never compare them across events.
  const event = events.findLast(event => event.taskActivity?.items.some(item => item.progress.task_id === task));
  return {items: event?.taskActivity?.items.filter(item => item.progress.task_id === task) ?? [],
    truncated: event?.taskActivity?.truncated ?? false};
}
