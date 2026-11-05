import * as v from "valibot";

const identity = v.pipe(v.string(), v.regex(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u));
const count = v.pipe(v.number(), v.safeInteger(), v.minValue(0), v.maxValue(200_000));
const label = v.pipe(v.string(), v.minLength(1), v.maxLength(256));
const byteCount = v.pipe(v.number(), v.safeInteger(), v.minValue(0), v.maxValue(68_719_476_736));
const progress = v.object({
  sequence: v.optional(v.pipe(v.number(), v.safeInteger(), v.minValue(0))),
  activity_id: identity, task_id: v.nullable(identity), source_admission_id: v.nullable(identity),
  operation: v.picklist(["intent", "goal", "capture", "restore"]),
  phase: v.picklist(["starting", "recorded", "unavailable", "preparing", "scanning", "uploading", "verifying", "saved", "restoring", "restored", "failed"]),
  provider: v.nullable(label), model: v.nullable(label), completed: count, total: v.nullable(count),
  bytes: v.optional(v.object({acknowledged: byteCount, missing: byteCount, reused: byteCount})),
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

const rank = {starting: 0, preparing: 0, scanning: 1, uploading: 2, verifying: 3, restoring: 4,
  unavailable: 5, failed: 5, recorded: 6, saved: 6, restored: 6} as const;

function activityKey(item: TaskActivity): string {
  const p = item.progress;
  return `${item.kind}:${p.activity_id}:${p.operation}:${p.phase}`;
}

function activityOrder(left: TaskActivity, right: TaskActivity): number {
  // UUIDs identify operations; only the server observation sequence orders them.
  const a = left.progress.sequence;
  const b = right.progress.sequence;
  if (a === undefined) return b === undefined ? 0 : -1;
  return b === undefined ? 1 : a - b;
}

/** A reconnect retains confirmed stages. Delayed counters cannot move backward. */
export function addTaskActivity(previous: TaskActivityHistory | undefined, next: TaskActivity): TaskActivityHistory {
  const items = previous?.items ?? [];
  const index = items.findIndex(item => item.kind === next.kind && item.progress.activity_id === next.progress.activity_id
    && item.progress.phase === next.progress.phase && item.progress.operation === next.progress.operation);
  const existing = items[index];
  if (previous && existing && ((existing.progress.sequence !== undefined && (next.progress.sequence === undefined || existing.progress.sequence > next.progress.sequence))
    || existing.progress.completed > next.progress.completed
    || (existing.progress.bytes?.acknowledged ?? 0) > (next.progress.bytes?.acknowledged ?? 0)
    || (existing.progress.bytes?.reused ?? 0) > (next.progress.bytes?.reused ?? 0)
    || JSON.stringify(existing) === JSON.stringify(next))) return previous;
  const updated = index < 0 ? [...items, next] : items.map((item, at) => at === index ? next : item);
  return {items: updated.sort(activityOrder).slice(-128), truncated: previous?.truncated === true || updated.length > 128};
}

/** Saved windows lead. Historical live tails require a shared ordering anchor. */
export function mergeTaskActivity(previous: TaskActivityHistory | undefined, saved: TaskActivityHistory | undefined): TaskActivityHistory | undefined {
  if (!saved || saved.items.length === 0) return previous ?? saved;
  if (!previous) return saved;
  const keys = new Set(saved.items.map(activityKey));
  const anchor = previous.items.findLastIndex(item => keys.has(activityKey(item)));
  let merged = saved;
  for (const [index, item] of previous.items.entries()) {
    // Disjoint unsequenced windows cannot establish which live items are newer.
    if (item.progress.sequence !== undefined || keys.has(activityKey(item)) || (anchor >= 0 && index > anchor)) {
      merged = addTaskActivity(merged, item);
    }
  }
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
  preparing: "Preparing recovery files", scanning: "Scanning frozen recovery files", uploading: "Saving encrypted files", verifying: "Verifying recovery data",
  saved: "Recovery point saved", restoring: "Restoring saved progress", restored: "Saved progress restored", failed: "Recovery interrupted",
} as const;

export function taskActivityLabel(item: TaskActivity): string {
  const p = item.progress;
  const purpose = p.operation === "intent" ? "request intent" : p.operation === "goal" ? "goal progress" : "";
  return `${labels[p.phase]}${purpose ? ` · ${purpose}` : ""}${p.model ? ` · ${p.provider ?? ""} / ${p.model}` : ""}`;
}
