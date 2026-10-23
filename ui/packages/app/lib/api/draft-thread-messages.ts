import type { ThreadMessageLike } from "@assistant-ui/react";
import type { DraftView } from "./draft-state";
import type { Message } from "./draft-types";
import { isRunning, MESSAGE_ROLE, MESSAGE_STATE } from "./draft-types";
import { isStopped, UNKNOWN_USAGE_REASON } from "./draft-recovery";

const FAILURE_COPY: Readonly<Record<string, string>> = {
  insufficient_balance: "The account balance could not cover this setup turn.",
  model_unavailable: "The selected setup model is no longer available.",
  context_too_large: "The conversation and instructions exceed the model’s context limit.",
  provider_failed: "The setup turn failed after model usage was recorded. No detailed cause was recorded.",
  provider_usage_unknown: "The model request ended without confirmed usage. Completion could not be verified; usage is still being reconciled.",
  attempt_timed_out: "The setup turn exceeded its time limit.",
  policy_changed: "The setup or its access policy changed during this turn.",
};
const CAUSE_COPY = {
  deadline: "The model did not finish before its request deadline.",
  cancelled: "The model request was stopped.",
  provider_interrupted: "The connection to the model ended before completion was confirmed.",
  completion_unverified: "The model reply did not confirm a completed answer.",
} as const;

/** A pure projection: live text can never replace a settled durable answer. */
export function draftThreadMessages(view: DraftView): ThreadMessageLike[] {
  const cutoff = view.draft?.cancelled_through ?? "0";
  const assistants = new Set(view.messages.filter(message => message.role === MESSAGE_ROLE.assistant).map(message => message.sequence));
  const rows = view.messages.flatMap(message => {
    const row = threadMessage(message, view, cutoff);
    if (message.role !== MESSAGE_ROLE.user || !isRunning(message) || assistants.has(message.sequence)) return [row];
    return [row, threadMessage({...message, role: MESSAGE_ROLE.assistant, text: undefined}, view, cutoff)];
  });
  const pending = view.optimistic;
  if (pending && !view.messages.some(message => message.operation_id === pending.operation)) {
    rows.push({id: `pending:${pending.operation}`, role: MESSAGE_ROLE.user,
      content: [{type: "text", text: pending.text}], createdAt: new Date(pending.createdAt)});
    rows.push({id: `pending-answer:${pending.operation}`, role: MESSAGE_ROLE.assistant,
      content: [{type: "text", text: view.liveReply?.text ?? ""}], status: {type: "running"}});
  }
  return rows;
}

function threadMessage(message: Message, view: DraftView, cutoff: string): ThreadMessageLike {
  const failure = message.failure;
  const reason = failure?.reason;
  const stopped = isStopped(message, cutoff);
  const pending = isRunning(message) && !stopped;
  const live = pending && message.role === MESSAGE_ROLE.assistant && view.liveReply?.sequence === message.sequence ? view.liveReply.text : undefined;
  const cause = failure?.cause ? CAUSE_COPY[failure.cause] : undefined;
  const recovery = stopped && reason === UNKNOWN_USAGE_REASON
    ? `${cause ?? "The model request ended without confirmed usage."} This turn is stopped; earlier usage is still being reconciled.`
    : cause ? `${cause}${reason === UNKNOWN_USAGE_REASON ? " Usage is still being reconciled." : ""}`
    : failure ? FAILURE_COPY[failure.reason] ?? "The setup turn failed. No detailed cause was recorded." : "";
  const text = [live ?? message.text ?? "",
    message.role === MESSAGE_ROLE.assistant && failure ? `${failure.error_code}: ${recovery}` : "",
    message.role === MESSAGE_ROLE.assistant && (message.state === MESSAGE_STATE.cancelled || (stopped && isRunning(message))) ? "Authoring stopped." : ""].filter(Boolean).join("\n\n");
  return {id: `${message.sequence}:${message.role}`, role: message.role, content: [{type: "text", text}], createdAt: new Date(message.created_at),
    metadata: {custom: {queued: pending && message.state === MESSAGE_STATE.queued}},
    ...(message.role === MESSAGE_ROLE.assistant ? {status: pending ? {type: "running"} : {type: "complete", reason: "stop"}} : {})};
}
