import * as v from "valibot";

const identity = v.pipe(v.string(), v.regex(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u));
const count = v.pipe(v.number(), v.safeInteger(), v.minValue(0), v.maxValue(200_000));
const label = v.pipe(v.string(), v.minLength(1), v.maxLength(256));
const progress = v.object({
  activity_id: identity, task_id: v.nullable(identity), source_admission_id: v.nullable(identity),
  operation: v.picklist(["intent", "goal", "capture", "restore"]),
  phase: v.picklist(["starting", "recorded", "unavailable", "preparing", "uploading", "verifying", "saved", "restoring", "restored", "failed"]),
  provider: v.nullable(label), model: v.nullable(label), completed: count, total: v.nullable(count),
});
const record = v.object({kind: v.picklist(["task_decision_started", "task_decision_completed", "task_transfer_progress"]), progress});
const history = v.object({items: v.pipe(v.array(record), v.maxLength(128)), truncated: v.boolean()});
export type TaskActivity = v.InferOutput<typeof record>;
export type TaskActivityHistory = v.InferOutput<typeof history>;

export function readTaskActivity(value: unknown): TaskActivity | undefined {
  const parsed = v.safeParse(record, value);
  return parsed.success ? parsed.output : undefined;
}

export function readTaskActivityHistory(value: unknown): TaskActivityHistory | undefined {
  const parsed = v.safeParse(history, value);
  return parsed.success ? parsed.output : undefined;
}

const rank = {starting: 0, preparing: 0, uploading: 1, restoring: 1, verifying: 2,
  unavailable: 3, failed: 3, recorded: 4, saved: 4, restored: 4} as const;

/** A reconnect retains confirmed stages. Delayed counters cannot move backward. */
export function addTaskActivity(previous: TaskActivityHistory | undefined, next: TaskActivity): TaskActivityHistory {
  const items = previous?.items ?? [];
  const index = items.findIndex(item => item.kind === next.kind && item.progress.activity_id === next.progress.activity_id
    && item.progress.phase === next.progress.phase && item.progress.operation === next.progress.operation);
  const existing = items[index];
  if (previous && existing && (existing.progress.completed > next.progress.completed || JSON.stringify(existing) === JSON.stringify(next))) return previous;
  const updated = index < 0 ? [...items, next] : items.map((item, at) => at === index ? next : item);
  return {items: updated.slice(-128), truncated: previous?.truncated === true || updated.length > 128};
}

/** Saved ordering leads; live stages newer than the read remain visible. */
export function mergeTaskActivity(previous: TaskActivityHistory | undefined, saved: TaskActivityHistory | undefined): TaskActivityHistory | undefined {
  if (!saved) return previous;
  if (!previous) return saved;
  let merged = saved;
  for (const item of previous.items) merged = addTaskActivity(merged, item);
  if (previous.truncated && !merged.truncated) merged = {...merged, truncated: true};
  return JSON.stringify(merged) === JSON.stringify(previous) ? previous : merged;
}

/** One compact current state per operation; details retain every public stage. */
export function latestTaskActivity(history: TaskActivityHistory): TaskActivity[] {
  const latest = new Map<string, TaskActivity>();
  for (const item of history.items) {
    const key = `${item.progress.activity_id}:${item.progress.operation}`;
    const previous = latest.get(key);
    if (!previous || rank[item.progress.phase] >= rank[previous.progress.phase]) latest.set(key, item);
  }
  return [...latest.values()];
}

const labels = {
  starting: "Assessing", recorded: "Assessment recorded", unavailable: "Assessment unavailable",
  preparing: "Preparing recovery files", uploading: "Saving encrypted files", verifying: "Verifying recovery data",
  saved: "Recovery point saved", restoring: "Restoring saved progress", restored: "Saved progress restored", failed: "Recovery interrupted",
} as const;

export function taskActivityLabel(item: TaskActivity): string {
  const p = item.progress;
  const purpose = p.operation === "intent" ? "request intent" : p.operation === "goal" ? "goal progress" : "";
  return `${labels[p.phase]}${purpose ? ` · ${purpose}` : ""}${p.model ? ` · ${p.provider ?? ""} / ${p.model}` : ""}`;
}
