import type { DraftView } from "./draft-state";
import type { Message } from "./draft-types";
import { DRAFT_STATE, isRunning } from "./draft-types";

export const UNKNOWN_USAGE_REASON = "provider_usage_unknown";

export function isStopped(message: Message, cancelledThrough: string): boolean {
  return BigInt(message.sequence) <= BigInt(cancelledThrough);
}

/** Cached failure actions can predate Stop; the current draft watermark wins. */
export function chatRecovery(view: DraftView): {pending: boolean; blocked: boolean; canStop: boolean} {
  const cutoff = view.draft?.cancelled_through ?? "0";
  let pending = false;
  let blocked = false;
  for (const message of view.messages) {
    if (isStopped(message, cutoff)) continue;
    pending ||= isRunning(message);
    blocked ||= message.failure?.reason === UNKNOWN_USAGE_REASON;
  }
  const editing = view.draft?.state === DRAFT_STATE.editing && !view.inaccessible;
  return {pending, blocked, canStop: editing && (pending || blocked)};
}
