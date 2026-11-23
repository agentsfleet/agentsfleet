import type { DraftView } from "./draft-state";
import type { Message } from "./draft-types";
import { DRAFT_STATE, isRunning, MESSAGE_ROLE, MESSAGE_STATE } from "./draft-types";

export const UNKNOWN_USAGE_REASON = "provider_usage_unknown";

/** A turn still running holds the chat; a failure, including unaccounted usage, does not. */
export function chatRecovery(view: DraftView): {pending: boolean; canStop: boolean} {
  const pending = view.messages.some(isRunning);
  const editing = view.draft?.state === DRAFT_STATE.editing && !view.inaccessible;
  return {pending, canStop: editing && pending};
}

/** The turn of every queued or running request, once each and oldest first: what Stop cancels. */
export function unfinishedTurns(messages: readonly Message[]): string[] {
  return [...new Set(messages.filter(isRunning).map((message) => message.turn_id))];
}

/** A settled failure can resend the exact text of its own turn; a stopped turn cannot. */
export function retryableDraftTurn(view: DraftView): Message | undefined {
  if (view.draft?.state !== DRAFT_STATE.editing || view.inaccessible || view.busy || view.pending || view.optimistic || chatRecovery(view).pending) return;
  const reply = view.messages.findLast(message => message.role === MESSAGE_ROLE.assistant);
  if (reply?.failure?.recovery_action !== "retry_message" || reply.state === MESSAGE_STATE.cancelled) return;
  return view.messages.find(message => message.role === MESSAGE_ROLE.user && message.sequence === reply.sequence && message.operation_id === reply.operation_id && message.text);
}
