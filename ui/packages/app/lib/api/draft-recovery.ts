import type { DraftView } from "./draft-state";
import type { Message } from "./draft-types";
import { DRAFT_STATE, isRunning } from "./draft-types";

export const UNKNOWN_USAGE_REASON = "provider_usage_unknown";

export function isStopped(message: Message, cancelledThrough: string): boolean {
  return BigInt(message.sequence) <= BigInt(cancelledThrough);
}

/** A turn still running holds the chat; a failure, including unaccounted usage, does not. */
export function chatRecovery(view: DraftView): {pending: boolean; canStop: boolean} {
  const cutoff = view.draft?.cancelled_through ?? "0";
  const pending = view.messages.some((message) => !isStopped(message, cutoff) && isRunning(message));
  const editing = view.draft?.state === DRAFT_STATE.editing && !view.inaccessible;
  return {pending, canStop: editing && pending};
}
