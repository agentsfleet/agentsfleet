import { addDurableTaskActivity, readDurableTaskActivity } from "../task-activity";
import type { FleetTurn } from "./fleet-stream-row";

export function applyDurableTaskActivity(previous: FleetTurn[], frame: {turn_id: string; kind: string; progress: unknown}): FleetTurn[] {
  const activity = readDurableTaskActivity(frame);
  const index = previous.findIndex(turn => turn.id === frame.turn_id);
  const turn = previous[index];
  if (!activity || !turn) return previous;
  const durableTaskActivity = addDurableTaskActivity(turn.durableTaskActivity, activity);
  if (durableTaskActivity === turn.durableTaskActivity) return previous;
  const next = [...previous];
  next[index] = {...turn, durableTaskActivity};
  return next;
}
