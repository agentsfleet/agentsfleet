import { addTaskActivity, readTaskActivity } from "../task-activity";
import type { FleetTurn } from "./fleet-stream-row";

export function applyTaskActivity(previous: FleetTurn[], frame: {turn_id: string; kind: string; progress: unknown}): FleetTurn[] {
  const activity = readTaskActivity(frame);
  const index = previous.findIndex(turn => turn.id === frame.turn_id);
  const turn = previous[index];
  if (!activity || !turn) return previous;
  const taskActivity = addTaskActivity(turn.taskActivity, activity);
  if (taskActivity === turn.taskActivity) return previous;
  const next = [...previous];
  next[index] = {...turn, taskActivity};
  return next;
}
