import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";
import type { TaskActivityHistory } from "@/lib/task-activity";

/** Only explicit task identity associates a turn's observations with this work. */
export function workActivity(turns: readonly FleetTurn[], task: string): TaskActivityHistory {
  // Observation sequences order one turn; never compare them across turns.
  const turn = turns.findLast(turn => turn.taskActivity?.items.some(item => item.progress.task_id === task));
  return {items: turn?.taskActivity?.items.filter(item => item.progress.task_id === task) ?? [],
    truncated: turn?.taskActivity?.truncated ?? false};
}
