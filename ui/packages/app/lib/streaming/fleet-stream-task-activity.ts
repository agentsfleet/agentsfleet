import { addTaskActivity, readTaskActivity } from "../task-activity";
import type { FleetEvent } from "./fleet-stream-row";

export function applyTaskActivity(previous: FleetEvent[], frame: {event_id: string; kind: string; progress: unknown}): FleetEvent[] {
  const activity = readTaskActivity(frame);
  const index = previous.findIndex(event => event.id === frame.event_id);
  const event = previous[index];
  if (!activity || !event) return previous;
  const taskActivity = addTaskActivity(event.taskActivity, activity);
  if (taskActivity === event.taskActivity) return previous;
  const next = [...previous];
  next[index] = {...event, taskActivity};
  return next;
}
