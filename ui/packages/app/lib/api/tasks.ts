import { z } from "zod";
import { Instant } from "./decode";
import { fleetPath } from "./fleet-paths";
import { nanosSchema } from "./nanos";

export const TASK_STATUS = {
  ready: "ready", running: "running", waiting: "waiting", blocked: "blocked",
  paused: "paused", done: "done", cancelled: "cancelled", exhausted: "exhausted",
} as const;
const Status = z.enum(Object.values(TASK_STATUS));
const Count = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);
const Identity = z.uuidv7();
const Statement = z.object({name: z.string().min(1), question: z.object({
  type: z.literal("noul"), instructions: z.string().min(1),
  criteria: z.object({true: z.string().min(1), false: z.string().min(1)}),
})});
const Summary = z.object({
  id: Identity, title: z.string().min(1), status: Status, resource_version: Count,
  generation: Count, accepted_revision_id: Identity, created_at: Instant, expires_at: Instant,
});
// Native admission permits sixteen unfinished tasks. Asking for 100 returns
// the whole panel; a continuation would mean an incomplete response, not no work.
const Page = z.object({items: z.array(Summary).max(100), next_cursor: z.null()});
const Detail = z.object({
  title: z.string().min(1), resource_version: Count, setup_revision: Count,
  context: z.object({
    task_id: Identity, generation: Count, stop_generation: Count,
    accepted_revision_id: Identity, objective: z.string(), goal_digest: z.string(),
    questions: z.array(Statement).min(1).max(6), state: Status,
    covered_input_sequence: Count, effect_cursor: Count, confirmed_effect_watermark: Count,
    checkpoint_id: Identity.nullable(), expires_at: Instant,
    limits: z.object({model_nanos: nanosSchema, wall_seconds: Count, running_seconds: Count,
      leases: Count, publications: Count, decision_calls: Count, fallback_reads: Count}),
    usage: z.object({model_nanos: nanosSchema, reserved_model_nanos: nanosSchema,
      running_nanos: Count, leases: Count, publications: Count, decision_calls: Count, fallback_reads: Count}),
  }),
});

export type TaskStatus = z.infer<typeof Status>;
export type TaskSummary = z.infer<typeof Summary>;
export type TaskPage = z.infer<typeof Page>;
export type TaskDetail = z.infer<typeof Detail>;
export type TaskReadFailure = "access" | "missing" | "timeout" | "unreachable" | "invalid";
export type TaskRead<T> = {kind: "ready"; value: T} | {kind: "failed"; reason: TaskReadFailure};

/** Browser reads stay on authenticated same-origin routes; no bearer enters the UI. */
export function readTasks(workspaceId: string, fleetId: string, signal: AbortSignal): Promise<TaskRead<TaskPage>> {
  return read(`/live${fleetPath(workspaceId, fleetId)}/tasks?limit=100`, Page, signal);
}

export function readTask(workspaceId: string, fleetId: string, taskId: string, signal: AbortSignal): Promise<TaskRead<TaskDetail>> {
  return read(`/live${fleetPath(workspaceId, fleetId)}/tasks/${encodeURIComponent(taskId)}`, Detail, signal);
}

async function read<T>(path: string, schema: z.ZodType<T>, signal: AbortSignal): Promise<TaskRead<T>> {
  const timeout = AbortSignal.timeout(10_000);
  const bounded = AbortSignal.any([signal, timeout]);
  let response: Response;
  try {
    response = await fetch(path, {signal: bounded, redirect: "manual", cache: "no-store"});
  } catch {
    return {kind: "failed", reason: timeout.aborted ? "timeout" : "unreachable"};
  }
  if (response.status === 401 || response.status === 403 || response.type === "opaqueredirect") return {kind: "failed", reason: "access"};
  if (response.status === 404) return {kind: "failed", reason: "missing"};
  if (!response.ok) return {kind: "failed", reason: "unreachable"};
  let value: unknown;
  try { value = await response.json(); }
  catch { return {kind: "failed", reason: timeout.aborted ? "timeout" : "invalid"}; }
  const result = schema.safeParse(value);
  return result.success ? {kind: "ready", value: result.data} : {kind: "failed", reason: "invalid"};
}
