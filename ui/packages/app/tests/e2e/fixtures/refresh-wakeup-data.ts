import {rowToTurn, type FleetTurn} from "@/lib/streaming/fleet-stream-row";
import {SCHEDULE, WORKSPACE, FLEET_ID} from "@/tests/helpers/draft-fixtures";
import {SCHEDULE_TURN} from "@/tests/helpers/schedule-activity";
import type {Schedule} from "@/lib/api/draft-types";

export function visualWakeup(started: number): Schedule {
  return {...SCHEDULE, fleet_id: FLEET_ID, status: "active", sync: "synced", run_state: "scheduled",
    cron: "*/30 * * * *", timezone: "UTC", description: "Every 30 minutes", message: "Check pull request #58",
    next_run_at: new Date(started + 30 * 60_000).toISOString()};
}

export function visualWakeupTurns(started: number): FleetTurn[] {
  const schedule = visualWakeup(started);
  const next = schedule.next_run_at!.slice(11, 16);
  const at = (offset: number) => new Date(started + offset).toISOString();
  const base = {...SCHEDULE_TURN, workspace_id: WORKSPACE, fleet_id: FLEET_ID};
  return [
    {...base, turn_id: "schedule-request", actor: "steer:fixture-owner", event_type: "steer",
      request_json: JSON.stringify({message: "Watch pull request #58. When it merges, check the image build and let me know in Slack."}),
      response_text: "I’ll watch the pull request and check the build after it merges.", created_at: at(-30 * 60_000), updated_at: at(-30 * 60_000)},
    {...base, turn_id: "schedule-created", actor: "system", event_type: "activity", status: "recorded",
      request_json: JSON.stringify({kind: "schedule_changed", change: "created", schedule_id: schedule.schedule_id,
        description: schedule.description, message: schedule.message, status: "active", sync: "synced"}),
      created_at: at(-30 * 60_000 + 1000), updated_at: at(-30 * 60_000 + 1000)},
    {...base, turn_id: "schedule-fired", request_json: JSON.stringify({message: schedule.message,
      schedule: {schedule_id: schedule.schedule_id, description: schedule.description}}),
      tool_calls: {omitted_call_count: 0, calls: [
        {call_id: "pull-request", name: "exec_command", arguments: {cmd: "gh pr view 58 --json state"}, status: "succeeded", exit_code: 0, duration_ms: 420, output_head: '{"state":"OPEN"}'},
        {call_id: "workflow", name: "exec_command", arguments: {cmd: "gh run list --limit 1 --json conclusion"}, status: "succeeded", exit_code: 0, duration_ms: 380, output_head: '[{"conclusion":"success"}]'},
      ]},
      response_text: `Pull request #58 is still open. No new build to check.\n\nNext check at **${next} UTC**. I’ll notify you in Slack after the merge and build checks.`,
      wall_ms: 8000, cost_nanos: "120000000", created_at: at(-10_000), updated_at: at(-2000)},
  ].map(rowToTurn);
}
