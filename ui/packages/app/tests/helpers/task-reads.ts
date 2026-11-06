import type { TaskDetail, TaskStatus, TaskSummary } from "@/lib/api/tasks";

export const TASK_IDS = [
  "019a42b0-0000-7000-8000-000000000123",
  "019a42b0-0000-7000-8000-000000000124",
  "019a42b0-0000-7000-8000-000000000125",
] as const;
export const SETUP_ID = "019a42b0-0000-7000-8000-000000000001";
export const INSTANT = "2026-11-03T10:00:00Z";

/** Responses from the native task API, not evidence of task creation. */
export function summary(id: string = TASK_IDS[0], status: TaskStatus = "waiting", version = 1): TaskSummary {
  return {id, title: `PR #${id.slice(-3)} repair`, status, resource_version: version, generation: 1,
    accepted_revision_id: SETUP_ID, created_at: INSTANT, expires_at: INSTANT};
}

export function detail(task: TaskSummary = summary(), checkpoint: string | null = null): TaskDetail {
  return {title: task.title, resource_version: task.resource_version, setup_revision: 2, context: {
    task_id: task.id, generation: task.generation, stop_generation: 0, accepted_revision_id: SETUP_ID,
    objective: "Repair the callback redirect without weakening the check.", goal_digest: "goal-123",
    questions: [{name: "repair", question: {type: "noul", instructions: "Does the regression pass on the proposed head?",
      criteria: {true: "The exact proposed head passes the regression.", false: "The head has no passing regression evidence."}}}],
    state: task.status, covered_input_sequence: 1, effect_cursor: 0, confirmed_effect_watermark: 0,
    checkpoint_id: checkpoint, expires_at: INSTANT,
    limits: {model_nanos: "100000000", wall_seconds: 86400, running_seconds: 3600, leases: 12,
      publications: 4, decision_calls: 64, fallback_reads: 24},
    usage: {model_nanos: "12000000", reserved_model_nanos: "20000000", running_nanos: 42000000000,
      leases: 2, publications: 0, decision_calls: 3, fallback_reads: 0},
  }};
}
