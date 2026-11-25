import type { DurableTaskDetail, DurableTaskStatus, DurableTaskSummary } from "@/lib/api/tasks";

export const DURABLE_TASK_IDS = [
  "019a42b0-0000-7000-8000-000000000123",
  "019a42b0-0000-7000-8000-000000000124",
  "019a42b0-0000-7000-8000-000000000125",
] as const;
export const INSTANT = "2026-11-03T10:00:00Z";

/** Responses from the native task API, not evidence of task creation. */
export function summary(id: string = DURABLE_TASK_IDS[0], status: DurableTaskStatus = "waiting", version = 1): DurableTaskSummary {
  return {id, title: `PR #${id.slice(-3)} repair`, status, resource_version: version, generation: 1,
    created_at: INSTANT, expires_at: INSTANT};
}

export function detail(task: DurableTaskSummary = summary(), checkpoint: string | null = null): DurableTaskDetail {
  return {title: task.title, resource_version: task.resource_version, context: {
    durable_task_id: task.id, generation: task.generation, stop_generation: 0,
    objective: "Repair the callback redirect without weakening the check.", goal_digest: "goal-123",
    questions: [{name: "repair", question: {type: "noul", instructions: "Does the regression pass on the proposed head?",
      criteria: {true: "The exact proposed head passes the regression.", false: "The head has no passing regression evidence."}}}],
    state: task.status, covered_input_sequence: 1, effect_cursor: 0, confirmed_effect_watermark: 0,
    checkpoint_id: checkpoint, expires_at: INSTANT,
    limits: {model_nanos: "100000000", wall_seconds: 86400, running_seconds: 3600, runs: 12,
      publications: 4, decision_calls: 64},
    usage: {model_nanos: "12000000", running_nanos: 42000000000,
      runs: 2, publications: 0, decision_calls: 3},
  }};
}
