import type { TaskSummary } from "@/lib/api/tasks";
import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";

export type TaskReference = {kind: "current"; id: string; resourceVersion: number} | {kind: "historical"; id: string};
export function currentTaskRef(task: TaskSummary): TaskReference {
  return {kind: "current", id: task.id, resourceVersion: task.resource_version};
}

/** Saved native activity is the only discovery source for completed task IDs. */
export function recentTaskIds(turns: readonly FleetTurn[], current: readonly TaskSummary[]): string[] {
  const live = new Set(current.map(task => task.id));
  const ids = new Set<string>();
  for (const turn of turns) for (const item of turn.taskActivity?.items ?? []) {
    const id = item.progress.task_id;
    if (!id || live.has(id)) continue;
    ids.delete(id);
    ids.add(id);
  }
  return [...ids].slice(-16).reverse();
}
