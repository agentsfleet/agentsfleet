import type { EventDetail } from "@/lib/api/events";
import { SCHEDULE, WORKSPACE } from "./draft-fixtures";

export const SCHEDULE_SNAPSHOT = {schedule_id: SCHEDULE.schedule_id, description: SCHEDULE.description,
  message: SCHEDULE.message, status: SCHEDULE.status, sync: SCHEDULE.sync};

export const SCHEDULE_EVENT: EventDetail = {workspace_id: WORKSPACE, fleet_id: SCHEDULE.fleet_id,
  event_id: "0199a801-1000-7000-8000-000000000009", actor: `cron:${SCHEDULE.schedule_id}`, event_type: "cron",
  status: "processed", request_json: JSON.stringify({message: SCHEDULE.message,
    schedule: {schedule_id: SCHEDULE.schedule_id, description: SCHEDULE.description}}), response_text: null, tool_calls: null,
  tokens: null, wall_ms: null, cost_nanos: null, failure_label: null, failure_detail: null, resumes_event_id: null,
  created_at: "2026-11-04T03:30:00Z", updated_at: "2026-11-04T03:31:00Z"};
