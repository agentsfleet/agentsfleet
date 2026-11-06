import type { TaskReference } from "./task-references";
import type { TaskDetail, TaskRead } from "@/lib/api/tasks";
import { readTask } from "@/lib/api/tasks";
import { TASK_READ_ATTEMPTS, retryableTaskRead, waitForTaskRetry } from "./task-read-recovery";

type Observer = (task: TaskReference, result: TaskRead<TaskDetail> | null) => void;
const READERS = 3;

/** One bounded owner supplies both the task rows and publication outputs. */
export async function loadTaskDetails(workspace: string, fleet: string, tasks: readonly TaskReference[], signal: AbortSignal, observe: Observer): Promise<void> {
  let pending = tasks;
  for (let attempt = 0; attempt < TASK_READ_ATTEMPTS && pending.length && !cancelled(signal); attempt += 1) {
    pending = await batch(workspace, fleet, pending, signal, observe);
    if (pending.length && attempt + 1 < TASK_READ_ATTEMPTS && !cancelled(signal)) await waitForTaskRetry(signal, attempt);
  }
}

async function batch(workspace: string, fleet: string, tasks: readonly TaskReference[], signal: AbortSignal, observe: Observer): Promise<TaskReference[]> {
  const pending = tasks.values();
  const retry: TaskReference[] = [];
  const worker = async () => {
    while (!cancelled(signal)) {
      const entry = pending.next();
      if (entry.done) return;
      const task = entry.value;
      observe(task, null);
      const result = owned(task, await readTask(workspace, fleet, task.id, signal));
      if (cancelled(signal)) return;
      observe(task, result);
      if (retryableTaskRead(result)) retry.push(task);
    }
  };
  await Promise.all(Array.from({length: Math.min(READERS, tasks.length)}, worker));
  return retry;
}

function cancelled(signal: AbortSignal): boolean {
  return signal.aborted;
}

function owned(task: TaskReference, result: TaskRead<TaskDetail>): TaskRead<TaskDetail> {
  if (result.kind === "ready" && (result.value.context.task_id !== task.id || (task.kind === "current" && result.value.resource_version < task.resourceVersion))) {
    return {kind: "failed", reason: "invalid"};
  }
  return result;
}
