import { z } from "zod";
import { Instant } from "./decode";
import { fleetPath } from "./fleet-paths";
import { nanosSchema } from "./nanos";

export const DURABLE_TASK_STATUS = {
  ready: "ready", running: "running", waiting: "waiting", blocked: "blocked",
  paused: "paused", done: "done", cancelled: "cancelled", exhausted: "exhausted",
} as const;
const Status = z.enum(Object.values(DURABLE_TASK_STATUS));
const Count = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);
const Identity = z.uuidv7();
const GitOid = z.string().regex(/^[0-9a-f]{40}$/u);
const Target = z.discriminatedUnion("kind", [z.object({kind: z.literal("api_only")}), z.object({
  kind: z.literal("existing_pr"), repository: z.string(), number: Count, branch: z.string(),
  head: GitOid, tree: GitOid, url: z.url().refine(value => value.startsWith("https://github.com/")),
  workflows: z.array(z.object({id: Count, path: z.string(), digest: z.string()})),
})]);
const Dependency = z.object({active: z.boolean(), head: GitOid, updated_at: Instant, checks: z.array(z.object({
  workflow_id: Count, run_id: Count.nullable(), run_number: Count.nullable(), run_attempt: Count.nullable(),
  status: z.string(), conclusion: z.string().nullable(),
}))});
const Statement = z.object({name: z.string().min(1), question: z.object({
  type: z.literal("noul"), instructions: z.string().min(1),
  criteria: z.object({true: z.string().min(1), false: z.string().min(1)}),
})});
const Summary = z.object({
  id: Identity, title: z.string().min(1), status: Status, resource_version: Count,
  generation: Count, accepted_revision_id: Identity, created_at: Instant, expires_at: Instant,
});
// The panel reads one page; a continuation would mean an incomplete
// response, not no work, so a page that names one is refused.
const Page = z.object({items: z.array(Summary), next_cursor: z.null()});
const Detail = z.object({
  publication: z.object({effect_id: Identity, ordinal: Count,
    state: z.enum(["prepared", "issued", "confirmed", "unknown", "refused"]),
    expected_head: GitOid, tested_tree: GitOid, commit: GitOid.nullable(), pull_request_url: z.string(),
  }).nullable().optional(),
  dependency: Dependency.nullable().optional(),
  title: z.string().min(1), resource_version: Count, setup_revision: Count,
  etag: z.string().regex(/^"[0-9a-f]{64}"$/u).optional(),
  context: z.object({
    target: Target.optional(),
    durable_task_id: Identity, generation: Count, stop_generation: Count,
    accepted_revision_id: Identity, objective: z.string(), goal_digest: z.string(),
    questions: z.array(Statement).min(1), state: Status,
    covered_input_sequence: Count, effect_cursor: Count, confirmed_effect_watermark: Count,
    checkpoint_id: Identity.nullable(), expires_at: Instant,
    limits: z.object({model_nanos: nanosSchema, wall_seconds: Count, running_seconds: Count,
      runs: Count, publications: Count, decision_calls: Count}),
    usage: z.object({model_nanos: nanosSchema,
      running_nanos: Count, runs: Count, publications: Count, decision_calls: Count}),
  }),
});

export type DurableTaskStatus = z.infer<typeof Status>;
export type DurableTaskSummary = z.infer<typeof Summary>;
export type DurableTaskPage = z.infer<typeof Page>;
export type DurableTaskDetail = z.infer<typeof Detail>;
export type DurableTaskControl = "paused" | "ready" | "cancelled";
export type DurableTaskChange = {kind: "confirmed"; value: DurableTaskDetail} | {kind: "failed"; reason: "access" | "changed" | "limited" | "unconfirmed" | "invalid"};
export type DurableTaskReadFailure = "access" | "missing" | "timeout" | "unreachable" | "invalid";
export type DurableTaskRead<T> = {kind: "ready"; value: T} | {kind: "failed"; reason: DurableTaskReadFailure};

/** Browser reads stay on authenticated same-origin routes; no bearer enters the UI. */
export function readDurableTasks(workspaceId: string, fleetId: string, signal: AbortSignal): Promise<DurableTaskRead<DurableTaskPage>> {
  return read(`/live${fleetPath(workspaceId, fleetId)}/durable_tasks?limit=100`, Page, signal);
}

export function readDurableTask(workspaceId: string, fleetId: string, durableTaskId: string, signal: AbortSignal): Promise<DurableTaskRead<DurableTaskDetail>> {
  return read(`/live${fleetPath(workspaceId, fleetId)}/durable_tasks/${encodeURIComponent(durableTaskId)}`, Detail, signal, true);
}

async function read<T>(path: string, schema: z.ZodType<T>, signal: AbortSignal, tagged = false): Promise<DurableTaskRead<T>> {
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
export function controlDurableTask(workspace: string, fleet: string, task: DurableTaskDetail, state: DurableTaskControl,
  signal: AbortSignal): Promise<DurableTaskChange> {
  return changeDurableTask(workspace, fleet, task, {state}, signal);
}

export function adoptDurableTaskGoal(workspace: string, fleet: string, task: DurableTaskDetail, revision: string, operation: string,
  signal: AbortSignal): Promise<DurableTaskChange> {
  return changeDurableTask(workspace, fleet, task, {goal_revision_id: revision, operation_id: operation}, signal);
}

async function changeDurableTask(workspace: string, fleet: string, task: DurableTaskDetail,
  mutation: {state: DurableTaskControl} | {goal_revision_id: string; operation_id: string}, signal: AbortSignal): Promise<DurableTaskChange> {
  if (!task.etag) return {kind: "failed", reason: "changed"};
  try {
    const response = await fetch(`/live${fleetPath(workspace, fleet)}/durable_tasks/${encodeURIComponent(task.context.durable_task_id)}`, {
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
