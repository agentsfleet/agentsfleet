import type { TaskDetail } from "../src/commands/fleet/task-read.ts";
export const WORKSPACE = "01910000-0000-7000-8000-000000000001";
export const FLEET = "01910000-0000-7000-8000-000000000002";
export const EVENT = "01910000-0000-7000-8000-000000000003";
export const TASK = "01910000-0000-7000-8000-000000000004";
export const HEAD = "a".repeat(40);
export const COMMIT = "b".repeat(40);
export const INSTANT = "2026-11-03T10:00:00Z";
export function taskDetail(): TaskDetail {
  return {title: "Repair the callback", resource_version: 1, setup_revision: 2, etag: `"${"a".repeat(64)}"`, context: {
    task_id: TASK, generation: 1, stop_generation: 0, state: "waiting", accepted_revision_id: TASK,
    objective: "Repair", goal_digest: "goal", questions: [{name: "repair", question: {type: "noul", instructions: "Fixed?", criteria: {true: "Fixed", false: "Broken"}}}],
    covered_input_sequence: 1, effect_cursor: 1, confirmed_effect_watermark: 0, checkpoint_id: null, expires_at: INSTANT,
    target: {kind: "existing_pr", repository: "agentsfleet/agentsfleet", number: 803, branch: "work", head: HEAD, tree: HEAD,
      url: "https://github.com/agentsfleet/agentsfleet/pull/803", workflows: [{id: 1, path: "check.yml", digest: "digest"}]},
    limits: {model_nanos: "100", wall_seconds: 100, running_seconds: 100, leases: 2, publications: 2, decision_calls: 2, fallback_reads: 2},
    usage: {model_nanos: "0", reserved_model_nanos: "0", running_nanos: 0, leases: 1, publications: 0, decision_calls: 1, fallback_reads: 0},
  }};
}
