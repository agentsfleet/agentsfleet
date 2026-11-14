import type { TaskRead } from "@/lib/api/tasks";
import { fastBackoffMs } from "@/lib/streaming/fleet-stream-reconnect";

export const TASK_READ_ATTEMPTS = 4;

/** A user retry or native revision starts a new bounded recovery cycle. */
export async function recoverTaskRead<T>(read: (signal: AbortSignal) => Promise<TaskRead<T>>, signal: AbortSignal, observe: (result: TaskRead<T>) => void): Promise<void> {
  for (let attempt = 0; attempt < TASK_READ_ATTEMPTS && !cancelled(signal); attempt += 1) {
    const result = await read(signal);
    if (cancelled(signal)) return;
    observe(result);
    if (!retryableTaskRead(result)) return;
    if (attempt + 1 < TASK_READ_ATTEMPTS) await waitForTaskRetry(signal, attempt);
  }
}

export function retryableTaskRead<T>(result: TaskRead<T>): boolean {
  return result.kind === "failed" && result.reason !== "access" && result.reason !== "missing";
}

/** Browser stream policy supplies capped exponential equal jitter; no server retry bundle. */
export function waitForTaskRetry(signal: AbortSignal, attempt: number): Promise<void> {
  if (signal.aborted) return Promise.resolve();
  return new Promise(resolve => {
    const finish = () => {clearTimeout(timer); signal.removeEventListener("abort", finish); resolve();};
    const timer = setTimeout(finish, fastBackoffMs(attempt));
    signal.addEventListener("abort", finish, {once: true});
  });
}

function cancelled(signal: AbortSignal): boolean {
  return signal.aborted;
}
