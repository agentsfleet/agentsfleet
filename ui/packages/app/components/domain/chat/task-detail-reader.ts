import type { TaskReference } from "./task-references";
import { readTask, type TaskDetail, type TaskRead } from "@/lib/api/tasks";

type Observer = (task: TaskReference, result: TaskRead<TaskDetail> | null) => void;
const READERS = 3;
const RETRY_MS = 5000;

/** One bounded owner supplies both the task rows and publication outputs. */
export async function loadTaskDetails(workspace: string, fleet: string, tasks: readonly TaskReference[], signal: AbortSignal, observe: Observer): Promise<void> {
  let pending = tasks;
  while (pending.length && !cancelled(signal)) {
    pending = await batch(workspace, fleet, pending, signal, observe);
    if (pending.length && !cancelled(signal)) await recoverAfter(signal);
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
      if (result.kind === "failed" && result.reason !== "access" && result.reason !== "missing") retry.push(task);
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

function recoverAfter(signal: AbortSignal): Promise<void> {
  return new Promise(resolve => {
    const finish = () => {clearTimeout(timer); signal.removeEventListener("abort", finish); resolve();};
    const timer = setTimeout(finish, RETRY_MS);
    signal.addEventListener("abort", finish, {once: true});
  });
}
