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
  etag: z.string().regex(/^"[0-9a-f]{64}"$/u).optional(),
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
export type TaskControl = "paused" | "ready" | "cancelled";
export type TaskChange = {kind: "confirmed"; value: TaskDetail} | {kind: "failed"; reason: "access" | "changed" | "limited" | "unconfirmed" | "invalid"};
export type TaskReadFailure = "access" | "missing" | "timeout" | "unreachable" | "invalid";
export type TaskRead<T> = {kind: "ready"; value: T} | {kind: "failed"; reason: TaskReadFailure};

/** Browser reads stay on authenticated same-origin routes; no bearer enters the UI. */
export function readTasks(workspaceId: string, fleetId: string, signal: AbortSignal): Promise<TaskRead<TaskPage>> {
  return read(`/live${fleetPath(workspaceId, fleetId)}/tasks?limit=100`, Page, signal);
}

export function readTask(workspaceId: string, fleetId: string, taskId: string, signal: AbortSignal): Promise<TaskRead<TaskDetail>> {
  return read(`/live${fleetPath(workspaceId, fleetId)}/tasks/${encodeURIComponent(taskId)}`, Detail, signal, true);
}

async function read<T>(path: string, schema: z.ZodType<T>, signal: AbortSignal, tagged = false): Promise<TaskRead<T>> {
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
  if (tagged && typeof value === "object" && value !== null && !Array.isArray(value)) {
    const etag = response.headers.get("etag");
    value = {...value, etag: etag ?? undefined};
  }
  const result = schema.safeParse(value);
  return result.success ? {kind: "ready", value: result.data} : {kind: "failed", reason: "invalid"};
}

/** Mutations use the exact read version, never retry automatically, and wait for native confirmation. */
export function controlTask(workspace: string, fleet: string, task: TaskDetail, state: TaskControl,
  signal: AbortSignal): Promise<TaskChange> {
  return changeTask(workspace, fleet, task, {state}, signal);
}

export function adoptTaskGoal(workspace: string, fleet: string, task: TaskDetail, revision: string, operation: string,
  signal: AbortSignal): Promise<TaskChange> {
  return changeTask(workspace, fleet, task, {goal_revision_id: revision, operation_id: operation}, signal);
}

async function changeTask(workspace: string, fleet: string, task: TaskDetail,
  mutation: {state: TaskControl} | {goal_revision_id: string; operation_id: string}, signal: AbortSignal): Promise<TaskChange> {
  if (!task.etag) return {kind: "failed", reason: "changed"};
  try {
    const response = await fetch(`/live${fleetPath(workspace, fleet)}/tasks/${encodeURIComponent(task.context.task_id)}`, {
      method: "PATCH", redirect: "manual", cache: "no-store", signal: AbortSignal.any([signal, AbortSignal.timeout(30_000)]),
      headers: {"Content-Type": "application/json", "If-Match": task.etag},
      body: JSON.stringify({...mutation, expected_generation: task.context.generation}),
    });
    if (response.status === 401 || response.status === 403 || response.type === "opaqueredirect") return {kind: "failed", reason: "access"};
    if ([404, 409, 412, 428].includes(response.status)) return {kind: "failed", reason: "changed"};
    if (response.status === 429) return {kind: "failed", reason: "limited"};
    if (!response.ok) return {kind: "failed", reason: "unconfirmed"};
    const value: unknown = await response.json();
    if (typeof value !== "object" || value === null || Array.isArray(value)) return {kind: "failed", reason: "invalid"};
    const result = Detail.safeParse({...value, etag: response.headers.get("etag")});
    return result.success ? {kind: "confirmed", value: result.data} : {kind: "failed", reason: "invalid"};
  } catch { return {kind: "failed", reason: "unconfirmed"}; }
}
