import {rowToEvent, type FleetEvent} from "@/lib/streaming/fleet-stream-row";
import {SCHEDULE, WORKSPACE, FLEET_ID} from "@/tests/helpers/draft-fixtures";
import {SCHEDULE_EVENT} from "@/tests/helpers/schedule-activity";
import type {Schedule} from "@/lib/api/draft-types";

export function visualWakeup(started: number): Schedule {
  return {...SCHEDULE, fleet_id: FLEET_ID, status: "active", sync: "synced", run_state: "scheduled",
    cron: "*/30 * * * *", timezone: "UTC", description: "Every 30 minutes", message: "Check pull request #58 and the image build.",
    next_run_at: new Date(started + 30 * 60_000).toISOString()};
}

export function visualWakeupEvents(started: number): FleetEvent[] {
  const schedule = visualWakeup(started);
  const at = (offset: number) => new Date(started + offset).toISOString();
  const base = {...SCHEDULE_EVENT, workspace_id: WORKSPACE, fleet_id: FLEET_ID};
  return [
    {...base, event_id: "schedule-request", actor: "steer:fixture-owner", event_type: "steer",
      request_json: JSON.stringify({message: "Watch pull request #58. When it merges, check the image build and let me know in Slack."}),
      response_text: "I’ll watch the pull request and check the build after it merges.", created_at: at(-30 * 60_000), updated_at: at(-30 * 60_000)},
    {...base, event_id: "schedule-created", actor: "system", event_type: "activity", status: "recorded",
      request_json: JSON.stringify({kind: "schedule_changed", change: "created", schedule_id: schedule.schedule_id,
        description: schedule.description, message: schedule.message, status: "active", sync: "synced"}),
      created_at: at(-30 * 60_000 + 1000), updated_at: at(-30 * 60_000 + 1000)},
    {...base, event_id: "schedule-fired", request_json: JSON.stringify({message: schedule.message,
      schedule: {schedule_id: schedule.schedule_id, description: schedule.description}}),
      response_text: "Pull request #58 is still open. No new build to check.\n\nI’ll only notify you when something needs your attention.",
      wall_ms: 8000, cost_nanos: "120000000", created_at: at(-10_000), updated_at: at(-2000)},
  ].map(rowToEvent);
}
