import type { DraftView } from "./draft-state";
import { DRAFT_STATE, isRunning, MESSAGE_ROLE, type Message } from "./draft-types";
import { isStopped } from "./draft-recovery";

// The accepted flag is the authority used by both the daemon and its prompt.
// Allow an imperative followed by another instruction, never a condition or question.
const SAVE_COMMAND = /^(?:please\s+)?(?:go ahead and\s+)?save(?:\s+(?:this|it|the fleet|this fleet|fleet|the setup|this setup|setup|the changes|these changes|changes))?(?:\s+now)?(?:\s+please)?(?:\s+and\s+[^?\n]+)?[.!]*$/i;
const CONDITIONAL = /\b(?:if|unless|after|before|when|until)\b/i;

export function saveRequested(text: string): boolean {
  return !CONDITIONAL.test(text) && SAVE_COMMAND.test(text.trim());
}

export function latestSaveAnswer(messages: readonly Message[]): Message | undefined {
  const user = messages.findLast(message => message.role === MESSAGE_ROLE.user);
  return user?.save_requested ? messages.find(message => message.role === MESSAGE_ROLE.assistant && message.operation_id === user.operation_id) : undefined;
}

/** Progress is reconstructed from durable messages when a tab reopens. */
export function savingOperation(view: DraftView): string | null {
  if (view.optimistic?.save) return view.optimistic.operation;
  const user = view.messages.findLast(message => message.role === MESSAGE_ROLE.user);
  if (!user?.save_requested || isStopped(user, view.draft?.cancelled_through ?? "0")) return null;
  const answer = view.messages.find(message => message.role === MESSAGE_ROLE.assistant && message.operation_id === user.operation_id);
  if (!answer || isRunning(answer)) return user.operation_id;
  return view.draft?.state !== DRAFT_STATE.editing && !view.fleet ? user.operation_id : null;
}
