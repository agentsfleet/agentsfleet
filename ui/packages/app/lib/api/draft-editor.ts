import { z } from "zod";
import type { DraftView } from "./draft-state";
import { DRAFT_STATE, DraftSchema, FleetSchema, isRunning, MessageSchema } from "./draft-types";
import { ActivityRowSchema, readSetupActivityPage } from "./draft-activity";
import { setupRequest, SetupUnavailable } from "./draft-request";

const ConversationSchema = z.object({draft: DraftSchema, messages: z.array(MessageSchema).max(25),
  messages_starting_after: z.string().nullable(), truncated: z.boolean()});
const EditorSchema = z.object({fleet: FleetSchema, conversations: z.array(ConversationSchema).max(3),
  activities: z.object({items: z.array(ActivityRowSchema).max(25), next_cursor: z.string().nullable()}), next_cursor: z.string().nullable()});
type Editor = z.infer<typeof EditorSchema>;

export function readEditor(base: string, fleet: string, signal: AbortSignal, after?: string): Promise<Editor> {
  const query = after ? `?${new URLSearchParams({starting_after: after})}` : "";
  return setupRequest({path: `${base}/fleets/${fleet}/authoring-context${query}`}, EditorSchema, signal);
}

export function editorPatch(editor: Editor, restoreCurrent = true) {
  const fleet = editor.fleet;
  const previousDrafts: NonNullable<DraftView["previousDrafts"]>[number][] = [];
  let current: Partial<DraftView> = {};
  for (const item of editor.conversations) {
    const {draft, messages} = item;
    if (draft.linked_fleet_id !== fleet.id) throw new SetupUnavailable("The setup chat does not belong to this fleet.", 409);
    if (restoreCurrent && draft.state === DRAFT_STATE.editing && draft.review?.effective.target_setup_revision === fleet.setup_revision && !current.draft) {
      current = {draft, messages, model: draft.authoring_model_ref, messagesStartingAfter: item.messages_starting_after};
    } else {
      if (messages.some(isRunning)) throw new SetupUnavailable("Finish or stop your earlier setup request before opening a new edit.", 409);
      previousDrafts.push({id: draft.id, cancelledThrough: draft.cancelled_through, messages, truncated: item.truncated});
    }
  }
  return {...current, previousDrafts: previousDrafts.sort((a, b) => a.id.localeCompare(b.id)), fleet, schedules: fleet.schedules,
    activities: editor.activities.items, activityCursor: editor.activities.next_cursor, conversationCursor: editor.next_cursor,
    truncated: editor.conversations.some(item => item.truncated)};
}

/** Older conversations merge without replacing a proposal or a live reply. */
export async function earlierConversations(base: string, read: () => DraftView, signal: AbortSignal): Promise<Partial<DraftView>> {
  const view = read();
  if (!view.fleet) return {};
  if (!view.conversationCursor) {
    if (!view.activityCursor) return {};
    const page = await readSetupActivityPage(base, view.fleet.id, signal, view.activityCursor);
    signal.throwIfAborted();
    if (read().fleet?.id !== view.fleet.id) return {};
    return {activities: page.items, activityCursor: page.next_cursor};
  }
  const page = editorPatch(await readEditor(base, view.fleet.id, signal, view.conversationCursor), false);
  signal.throwIfAborted();
  const current = read();
  if (current.fleet?.id !== view.fleet.id) return {};
  const previous = new Map((current.previousDrafts ?? []).map(item => [item.id, item]));
  for (const item of page.previousDrafts) if (item.id !== current.draft?.id && !previous.has(item.id)) previous.set(item.id, item);
  return {previousDrafts: [...previous.values()].sort((a, b) => a.id.localeCompare(b.id)), conversationCursor: page.conversationCursor,
    truncated: current.truncated || page.truncated};
}
