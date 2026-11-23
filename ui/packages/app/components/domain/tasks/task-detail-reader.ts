import type { DurableTaskReference } from "./task-references";
import type { DurableTaskDetail, DurableTaskRead } from "@/lib/api/tasks";
import { readDurableTask } from "@/lib/api/tasks";
import { DURABLE_TASK_READ_ATTEMPTS, retryableDurableTaskRead, waitForDurableTaskRetry } from "./task-read-recovery";

type Observer = (task: DurableTaskReference, result: DurableTaskRead<DurableTaskDetail> | null) => void;
const READERS = 3;

/** One bounded owner supplies both the task rows and publication outputs. */
export async function loadDurableTaskDetails(workspace: string, fleet: string, tasks: readonly DurableTaskReference[], signal: AbortSignal, observe: Observer): Promise<void> {
  let pending = tasks;
  for (let attempt = 0; attempt < DURABLE_TASK_READ_ATTEMPTS && pending.length && !cancelled(signal); attempt += 1) {
    pending = await batch(workspace, fleet, pending, signal, observe);
    if (pending.length && attempt + 1 < DURABLE_TASK_READ_ATTEMPTS && !cancelled(signal)) await waitForDurableTaskRetry(signal, attempt);
  }
}

async function batch(workspace: string, fleet: string, tasks: readonly DurableTaskReference[], signal: AbortSignal, observe: Observer): Promise<DurableTaskReference[]> {
  const pending = tasks.values();
  const retry: DurableTaskReference[] = [];
  const worker = async () => {
    while (!cancelled(signal)) {
      const entry = pending.next();
      if (entry.done) return;
      const task = entry.value;
      observe(task, null);
      const result = owned(task, await readDurableTask(workspace, fleet, task.id, signal));
      if (cancelled(signal)) return;
      observe(task, result);
      if (retryableDurableTaskRead(result)) retry.push(task);
    }
  };
  await Promise.all(Array.from({length: Math.min(READERS, tasks.length)}, worker));
  return retry;
}

function cancelled(signal: AbortSignal): boolean {
  return signal.aborted;
}

function owned(task: DurableTaskReference, result: DurableTaskRead<DurableTaskDetail>): DurableTaskRead<DurableTaskDetail> {
  if (result.kind === "ready" && (result.value.context.durable_task_id !== task.id || (task.kind === "current" && result.value.resource_version < task.resourceVersion))) {
    return {kind: "failed", reason: "invalid"};
  }
  return result;
}
