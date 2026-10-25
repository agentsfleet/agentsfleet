import type { ThreadMessageLike } from "@assistant-ui/react";
import type { DraftView } from "./draft-state";
import type { Message } from "./draft-types";
import type { ContextProgress } from "../context-progress";
import { isRunning, MESSAGE_ROLE, MESSAGE_STATE } from "./draft-types";
import { isStopped } from "./draft-recovery";

type ProjectMessage = typeof threadMessage;

/** One conversation owns its cache; unchanged rows keep their runtime identity during streaming. */
export function createDraftThreadProjection(): (view: DraftView) => ThreadMessageLike[] {
  const cache = new WeakMap<Message, {live: string | undefined; context: ContextProgress | undefined; stopped: boolean; request?: string; row: ThreadMessageLike}>();
  const project: ProjectMessage = (message, view, cutoff, request) => {
    const live = message.role === MESSAGE_ROLE.assistant && isRunning(message) && view.liveReply?.sequence === message.sequence ? view.liveReply.text : undefined;
    const stopped = isStopped(message, cutoff);
    const context = compactionFor(message, view, cutoff);
    const old = cache.get(message);
    if (old && old.live === live && old.context === context && old.stopped === stopped && old.request === request) return old.row;
    const row = threadMessage(message, view, cutoff, request);
    cache.set(message, {live, context, stopped, request, row});
    return row;
  };
  return view => draftThreadMessages(view, project);
}

/** A pure projection: live text can never replace a settled durable answer. */
export function draftThreadMessages(view: DraftView, project: ProjectMessage = threadMessage): ThreadMessageLike[] {
  const cutoff = view.draft?.cancelled_through ?? "0";
  const assistants = new Set<string>();
  const requests = new Map<string, string>();
  for (const message of view.messages) {
    if (message.role === MESSAGE_ROLE.assistant) assistants.add(message.sequence);
    else if (message.text) requests.set(message.sequence, message.text);
  }
  const rows = view.messages.flatMap(message => {
    const row = project(message, view, cutoff, requests.get(message.sequence));
    if (message.role !== MESSAGE_ROLE.user || !isRunning(message) || assistants.has(message.sequence)) return [row];
    return [row, project({...message, role: MESSAGE_ROLE.assistant, text: undefined}, view, cutoff)];
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

function threadMessage(message: Message, view: DraftView, cutoff: string, request?: string): ThreadMessageLike {
  const failure = message.failure;
  const stopped = isStopped(message, cutoff);
  const pending = isRunning(message) && !stopped;
  const live = pending && message.role === MESSAGE_ROLE.assistant && view.liveReply?.sequence === message.sequence ? view.liveReply.text : undefined;
  const text = [live ?? message.text ?? "",
    message.role === MESSAGE_ROLE.assistant && (message.state === MESSAGE_STATE.cancelled || (stopped && isRunning(message))) ? "Request stopped." : ""].filter(Boolean).join("\n\n");
  return {id: `${message.sequence}:${message.role}`, role: message.role, content: [{type: "text", text}], createdAt: new Date(message.created_at),
    metadata: {custom: {queued: pending && message.state === MESSAGE_STATE.queued,
      contextProgress: compactionFor(message, view, cutoff),
      failure: message.role === MESSAGE_ROLE.assistant ? failure : undefined, retryText: failure ? request : undefined}},
    ...(message.role === MESSAGE_ROLE.assistant ? {status: pending ? {type: "running"} : {type: "complete", reason: "stop"}} : {})};
}

function compactionFor(message: Message, view: DraftView, cutoff: string): ContextProgress | undefined {
  return message.role === MESSAGE_ROLE.assistant && isRunning(message) && !isStopped(message, cutoff)
    && view.contextProgress?.sequence === message.sequence ? view.contextProgress.progress : undefined;
}
