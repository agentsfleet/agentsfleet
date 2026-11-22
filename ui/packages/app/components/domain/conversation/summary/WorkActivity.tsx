import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";
import type { DurableTaskActivityHistory } from "@/lib/task-activity";

/** Only explicit task identity associates a turn's observations with this work. */
export function workActivity(turns: readonly FleetTurn[], task: string): DurableTaskActivityHistory {
  // Observation sequences order one turn; never compare them across turns.
  const turn = turns.findLast(turn => turn.durableTaskActivity?.items.some(item => item.progress.durable_task_id === task));
  return {items: turn?.durableTaskActivity?.items.filter(item => item.progress.durable_task_id === task) ?? [],
    truncated: turn?.durableTaskActivity?.truncated ?? false};
}
