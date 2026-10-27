import type { ThreadMessageLike } from "@assistant-ui/react";
import type { DraftView } from "./draft-state";
import type { Message } from "./draft-types";
import type { SetupActivity } from "./draft-activity";
import type { ContextProgress } from "../context-progress";
import { isRunning, MESSAGE_ROLE, MESSAGE_STATE } from "./draft-types";
import { isStopped } from "./draft-recovery";
import { setupActivityText } from "./draft-activity";

type ProjectMessage = typeof threadMessage;

/** One conversation owns its cache; unchanged rows keep their runtime identity during streaming. */
export function createDraftThreadProjection(): (view: DraftView) => ThreadMessageLike[] {
  const cache = new WeakMap<Message, Map<string, {live: string | undefined; context: ContextProgress | undefined; stopped: boolean; saving: boolean; request?: string; row: ThreadMessageLike}>>();
  const activities = new WeakMap<SetupActivity, ThreadMessageLike>();
  const project: ProjectMessage = (message, view, cutoff, request, prefix) => {
    const live = message.role === MESSAGE_ROLE.assistant && isRunning(message) && view.liveReply?.sequence === message.sequence ? view.liveReply.text : undefined;
    const stopped = isStopped(message, cutoff);
    const context = compactionFor(message, view, cutoff);
    const saving = isSaving(message, view);
    const entries = cache.get(message) ?? new Map();
    const old = entries.get(prefix);
    if (old && old.live === live && old.context === context && old.stopped === stopped && old.saving === saving && old.request === request) return old.row;
    const row = threadMessage(message, view, cutoff, request, prefix);
    entries.set(prefix, {live, context, stopped, saving, request, row}); cache.set(message, entries);
    return row;
  };
  let previous: {id: string | undefined; time: number}[] = [];
  let sortedIds: (string | undefined)[] = [];
  const order = (rows: ThreadMessageLike[]): ThreadMessageLike[] => {
    const keys = rows.map(row => ({id: row.id, time: row.createdAt?.getTime() ?? Number.POSITIVE_INFINITY}));
    if (keys.length !== previous.length || keys.some((key, index) => key.id !== previous[index]?.id || key.time !== previous[index]?.time)) {
      sortedIds = sortRows(rows).map(row => row.id); previous = keys;
    }
    const byId = new Map(rows.map(row => [row.id, row]));
    // The membership comparison above keeps every cached identity in this map.
    return sortedIds.map(id => byId.get(id) as ThreadMessageLike);
  };
  return view => draftThreadMessages(view, project, activity => {
    const old = activities.get(activity);
    if (old) return old;
    const row = activityMessage(activity); activities.set(activity, row); return row;
  }, order);
}

/** A pure projection: live text can never replace a settled durable answer. */
export function draftThreadMessages(view: DraftView, project: ProjectMessage = threadMessage, projectActivity = activityMessage, order = sortRows): ThreadMessageLike[] {
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
      content: [{type: "text", text: view.liveReply?.text ?? ""}], status: {type: "running"},
      metadata: {custom: {saving: view.saveOperation === pending.operation && !view.error}}});
  }
  const previousView = {...view, liveReply: null, contextProgress: null, saveOperation: null};
  const previous = (view.previousDrafts ?? []).flatMap(history => history.messages.map(message =>
    project(message, previousView, history.cancelledThrough, undefined, `${history.id}:`)));
  const activity = (view.activities ?? []).map(projectActivity);
  return order([...previous, ...rows, ...activity]);
}

function sortRows(rows: ThreadMessageLike[]): ThreadMessageLike[] {
  return rows.sort((left, right) => (left.createdAt?.getTime() ?? Number.POSITIVE_INFINITY) - (right.createdAt?.getTime() ?? Number.POSITIVE_INFINITY));
}

function activityMessage(row: SetupActivity): ThreadMessageLike {
  return {id: `activity:${row.event_id}`, role: "system", createdAt: new Date(row.created_at),
    content: [{type: "text", text: setupActivityText(row)}], metadata: {custom: {activity: true}}};
}

function threadMessage(message: Message, view: DraftView, cutoff: string, request?: string, prefix = ""): ThreadMessageLike {
  const failure = message.failure;
  const stopped = isStopped(message, cutoff);
  const pending = isRunning(message) && !stopped;
  const saving = isSaving(message, view);
  const live = pending && message.role === MESSAGE_ROLE.assistant && view.liveReply?.sequence === message.sequence ? view.liveReply.text : undefined;
  const text = [live ?? message.text ?? "",
    message.role === MESSAGE_ROLE.assistant && (message.state === MESSAGE_STATE.cancelled || (stopped && isRunning(message))) ? "Request stopped." : ""].filter(Boolean).join("\n\n");
  return {id: `${prefix}${message.sequence}:${message.role}`, role: message.role, content: [{type: "text", text}], createdAt: new Date(message.created_at),
    metadata: {custom: {saving, queued: pending && message.state === MESSAGE_STATE.queued,
      contextProgress: compactionFor(message, view, cutoff),
      failure: message.role === MESSAGE_ROLE.assistant ? failure : undefined, retryText: failure ? request : undefined}},
    ...(message.role === MESSAGE_ROLE.assistant ? {status: pending || saving ? {type: "running"} : {type: "complete", reason: "stop"}} : {})};
}

function isSaving(message: Message, view: DraftView): boolean {
  return message.role === MESSAGE_ROLE.assistant && message.operation_id === view.saveOperation && !view.error;
}

function compactionFor(message: Message, view: DraftView, cutoff: string): ContextProgress | undefined {
  return message.role === MESSAGE_ROLE.assistant && isRunning(message) && !isStopped(message, cutoff)
    && view.contextProgress?.sequence === message.sequence ? view.contextProgress.progress : undefined;
}
