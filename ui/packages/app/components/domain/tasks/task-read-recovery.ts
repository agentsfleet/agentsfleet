import type { DurableTaskRead } from "@/lib/api/tasks";
import { fastBackoffMs } from "@/lib/streaming/fleet-stream-reconnect";

export const DURABLE_TASK_READ_ATTEMPTS = 4;

/** A user retry or native revision starts a new bounded recovery cycle. */
export async function recoverDurableTaskRead<T>(read: (signal: AbortSignal) => Promise<DurableTaskRead<T>>, signal: AbortSignal, observe: (result: DurableTaskRead<T>) => void): Promise<void> {
  for (let attempt = 0; attempt < DURABLE_TASK_READ_ATTEMPTS && !cancelled(signal); attempt += 1) {
    const result = await read(signal);
    if (cancelled(signal)) return;
    observe(result);
    if (!retryableDurableTaskRead(result)) return;
    if (attempt + 1 < DURABLE_TASK_READ_ATTEMPTS) await waitForDurableTaskRetry(signal, attempt);
  }
}

export function retryableDurableTaskRead<T>(result: DurableTaskRead<T>): boolean {
  return result.kind === "failed" && result.reason !== "access" && result.reason !== "missing";
}

/** Browser stream policy supplies capped exponential equal jitter; no server retry bundle. */
export function waitForDurableTaskRetry(signal: AbortSignal, attempt: number): Promise<void> {
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
