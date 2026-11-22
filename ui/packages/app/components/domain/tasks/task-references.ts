import type { DurableTaskSummary } from "@/lib/api/tasks";
import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";

export type DurableTaskReference = {kind: "current"; id: string; resourceVersion: number} | {kind: "historical"; id: string};
export function currentDurableTaskRef(task: DurableTaskSummary): DurableTaskReference {
  return {kind: "current", id: task.id, resourceVersion: task.resource_version};
}

/** Saved native activity is the only discovery source for completed task IDs. */
export function recentDurableTaskIds(turns: readonly FleetTurn[], current: readonly DurableTaskSummary[]): string[] {
  const live = new Set(current.map(task => task.id));
  const ids = new Set<string>();
  for (const turn of turns) for (const item of turn.durableTaskActivity?.items ?? []) {
    const id = item.progress.durable_task_id;
    if (!id || live.has(id)) continue;
    ids.delete(id);
    ids.add(id);
  }
  return [...ids].reverse();
}
