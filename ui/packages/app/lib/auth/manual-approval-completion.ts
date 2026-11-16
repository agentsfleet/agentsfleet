import * as v from "valibot";

export type ManualCompletion = { kind: "completed" } | { kind: "failed"; message: string };
type ReadOutcome = "waiting" | "completed" | "failed";

const POLL_MS = 2_000;
const JITTER_MS = 400;
const REQUEST_TIMEOUT_MS = 15_000;
const MAX_WAIT_MS = 300_000;
const MAX_READS = 150;
const MAX_RESPONSE_BYTES = 4_096;
const SESSION_CONSUMED_CODE = "UZ-AUTH-012";
const EXPIRED_MESSAGE = "This sign-in expired. Start a new sign-in from your terminal.";
const FAILED_MESSAGE = "This sign-in could not be confirmed. Check your terminal, or start a new sign-in.";
const RefusalSchema = v.object({ error_code: v.string() });

/** The session read exposes completion without releasing any credential material. */
export async function watchManualApprovalCompletion(sessionId: string, expiresAt: string,
  signal: AbortSignal, update: (result: ManualCompletion) => void): Promise<void> {
  const expiry = Date.parse(expiresAt);
  const deadline = Number.isFinite(expiry) ? Math.min(expiry, Date.now() + MAX_WAIT_MS) : Date.now();
  const stopped = (): boolean => signal.aborted;
  for (let attempt = 0; attempt < MAX_READS && !stopped(); attempt += 1) {
    const remaining = deadline - Date.now();
    if (remaining <= 0) { update({ kind: "failed", message: EXPIRED_MESSAGE }); return; }
    await wait(Math.min(POLL_MS + Math.random() * JITTER_MS, remaining), signal);
    if (stopped()) return;
    if (Date.now() >= deadline) { update({ kind: "failed", message: EXPIRED_MESSAGE }); return; }
    try {
      const owner = AbortSignal.any([signal, AbortSignal.timeout(Math.max(1,
        Math.min(REQUEST_TIMEOUT_MS, deadline - Date.now())))]);
      const result = await readCompletion(sessionId, owner);
      if (stopped()) return;
      if (result === "completed") { update({ kind: "completed" }); return; }
      if (result === "failed") { update({ kind: "failed", message: FAILED_MESSAGE }); return; }
    } catch {
      // A lost status read must leave the verification code available.
      if (stopped()) return;
    }
  }
  if (!stopped()) update({ kind: "failed", message: FAILED_MESSAGE });
}

async function readCompletion(sessionId: string, signal: AbortSignal): Promise<ReadOutcome> {
  const response = await fetch(`/backend/v1/auth/sessions/${encodeURIComponent(sessionId)}`, {
    method: "GET", headers: { Accept: "application/json" }, cache: "no-store", redirect: "error", signal,
  });
  if (response.redirected) { await response.body?.cancel(); return "failed"; }
  if (response.status === 410) {
    const body = v.safeParse(RefusalSchema, await boundedJson(response, signal));
    return body.success && body.output.error_code === SESSION_CONSUMED_CODE ? "completed" : "failed";
  }
  await response.body?.cancel();
  return response.status >= 400 && response.status < 500 && response.status !== 429 ? "failed" : "waiting";
}

async function boundedJson(response: Response, signal: AbortSignal): Promise<unknown> {
  const reader = response.body?.getReader();
  if (!reader) throw new Error("Missing sign-in status response");
  const cancel = (): void => { void reader.cancel().catch(() => undefined); };
  signal.addEventListener("abort", cancel, { once: true });
  const decoder = new TextDecoder();
  let size = 0;
  let text = "";
  try {
    signal.throwIfAborted();
    for (let part = await reader.read(); !part.done; part = await reader.read()) {
      size += part.value.byteLength;
      if (size > MAX_RESPONSE_BYTES) throw new Error("Sign-in status response exceeds its limit");
      text += decoder.decode(part.value, { stream: true });
    }
    signal.throwIfAborted();
    return JSON.parse(text + decoder.decode());
  } finally {
    signal.removeEventListener("abort", cancel);
    try { await reader.cancel(); } catch { /* Release the lock even when the stream has failed. */ }
    finally { reader.releaseLock(); }
  }
}

function wait(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise(resolve => {
    const finish = (): void => { clearTimeout(timer); signal.removeEventListener("abort", finish); resolve(); };
    const timer = setTimeout(finish, ms);
    signal.addEventListener("abort", finish, { once: true });
    if (signal.aborted) finish();
  });
}
