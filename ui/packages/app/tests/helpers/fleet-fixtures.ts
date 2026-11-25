import type { Schedule } from "@/lib/api/schedule-types";
import type { WorkspaceModel } from "@/lib/api/workspace-models";
import { NANOS_PER_USD } from "@/lib/types";

export const WORKSPACE = "0199a801-1000-7000-8000-000000000001";
export const FLEET_ID = "0199a801-1000-7000-8000-000000000004";
export const OPERATION_ID = "0199a801-1000-7000-8000-000000000005";
export const SCHEDULE_ID = "0199a801-1000-7000-8000-000000000008";
export const TURN_ID = "0199a801-1000-7000-8000-000000000009";
export const TRIGGER_ID = "0199a801-1000-7000-8000-00000000000a";
export const GRANT_ID = "0199a801-1000-7000-8000-00000000000b";
export const APP_ORIGIN = "https://app.example.test";
export const MODEL_REF = "model:runtime";
export const SCHEDULE: Schedule = {schedule_id: SCHEDULE_ID, fleet_id: FLEET_ID,
  cron: "0 9 * * *", timezone: "Asia/Kolkata", message: "Review support failures — 東京 🦉", status: "active", sync: "synced",
  description: "At 9:00 AM (Asia/Kolkata)", run_state: "scheduled", next_run_at: "2026-11-05T03:30:00Z"};
export const FAILED_SCHEDULE: Schedule = {...SCHEDULE, sync: "failed", run_state: "next_run_unavailable", next_run_at: null};
export const PENDING_SCHEDULE: Schedule = {...SCHEDULE, sync: "syncing", run_state: "activation_pending", next_run_at: null};
/** `GET F` as the daemon sends it: counters are decimal strings. */
export const FLEET_DETAIL = {id: FLEET_ID, name: "repairer", status: "active", etag: '"fleet-one"', model_ref: MODEL_REF,
  http_egress: "restricted" as const, package_domains: false, schedules: [] as Schedule[], triggers: [], grants: [],
  memory_access: {read: true, publish: false}, created_at: "1970-01-01T00:00:00Z", updated_at: "1970-01-01T00:00:00Z",
  events_processed: "3", budget_used_nanos: "9007199254740993", pending_approvals: 2};
export const MODEL: WorkspaceModel = {ref: MODEL_REF, label: "Runtime model", provider: "openai", model: "gpt-4o",
  context_window_tokens: 128000, max_output_tokens: 8192, eligibility: {runtime: true},
  pricing: {input_nanos_per_million: String(2.5 * NANOS_PER_USD), output_nanos_per_million: String(10 * NANOS_PER_USD), runtime_nanos_per_second: "1"}};

export function response(value: unknown, status = 200, headers?: HeadersInit): Response {
  return new Response(status === 204 ? null : JSON.stringify(value), {status, headers: {"Content-Type": "application/json", ...headers}});
}
