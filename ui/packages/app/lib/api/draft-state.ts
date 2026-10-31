import type { Definition, Draft, DraftCard, Fleet, Message, Schedule, WorkspaceModel } from "./draft-types";
import { DRAFT_LIST_STATUS } from "./draft-list-types";
import type { DraftReply } from "./draft-replies";
import type { DraftCompaction } from "./draft-live";
import type { SetupActivity } from "./draft-activity";
import { latestSaveAnswer, savingOperation } from "./draft-chat-save";
import { DRAFT_STATE } from "./draft-types";
import { presentErrorString } from "@/lib/errors";

export type DraftIdentity = {readonly account: string; readonly workspace: string; readonly subject: string};
export type DraftView = {
  readonly draft: Draft | null;
  readonly messages: readonly Message[];
  readonly previousDrafts?: readonly {readonly id: string; readonly cancelledThrough: string; readonly messages: readonly Message[]; readonly truncated?: boolean}[];
  readonly conversationCursor?: string | null;
  readonly activityCursor?: string | null;
  readonly messagesStartingAfter?: string | null;
  readonly activities?: readonly SetupActivity[];
  readonly liveReply?: DraftReply | null;
  readonly contextProgress?: DraftCompaction | null;
  readonly optimistic?: {readonly operation: string; readonly save?: boolean; readonly text: string; readonly createdAt: string} | null;
  readonly models: readonly WorkspaceModel[];
  readonly cards: readonly DraftCard[];
  readonly moreModels: boolean;
  readonly moreChats: boolean;
  readonly listStatus: typeof DRAFT_LIST_STATUS[keyof typeof DRAFT_LIST_STATUS];
  readonly model: string;
  readonly preview: {readonly definition: Definition; readonly instanceName: string; readonly revision: string; readonly setupRevision?: string} | null;
  readonly savedSetupRevision: string | null;
  readonly saveOperation?: string | null;
  readonly setupAction?: "reading" | "saving" | "publishing" | null;
  readonly fleet: Fleet | null;
  /** The schedules the last Apply returned; any not `synced` may never fire. */
  readonly schedules: readonly Schedule[];
  readonly busy: boolean;
  readonly pending: boolean;
  readonly inaccessible: boolean;
  readonly error: string | null;
  readonly notice: string | null;
  readonly truncated: boolean;
  readonly focusRevision: number;
};
export type DrawerEdit = {readonly definition: Definition; readonly instanceName: string; readonly runtimeModel?: string};

export const EMPTY_DRAFT_VIEW: DraftView = {draft: null, messages: [], models: [], cards: [], moreModels: false, moreChats: false, listStatus: DRAFT_LIST_STATUS.loading,
  model: "", fleet: null, schedules: [], busy: false, pending: false, inaccessible: false, error: null, notice: null,
  truncated: false, preview: null, savedSetupRevision: null, focusRevision: 0};

export function projectDraftView(previous: DraftView, patch: Partial<DraftView>): DraftView {
  const next = {...previous, ...patch};
  if (patch.activities && next.fleet?.id === previous.fleet?.id && next.fleet) {
    const rows = new Map((previous.activities ?? []).map(row => [row.event_id, row]));
    for (const row of patch.activities) if (!rows.has(row.event_id)) rows.set(row.event_id, row);
    next.activities = [...rows.values()];
  }
  return {...next, saveOperation: savingOperation(next)};
}

export function messagePatch(view: DraftView, draft: Draft, messages: readonly Message[], truncated: boolean): Partial<DraftView> {
  const answer = latestSaveAnswer(messages);
  const optimistic = messages.some(message => message.operation_id === view.optimistic?.operation) ? null : view.optimistic;
  return {draft, model: draft.authoring_model_ref, messages, optimistic, truncated,
    ...(draft.state === DRAFT_STATE.editing && answer?.save_error_code ? {error: presentErrorString({errorCode: answer.save_error_code, action: "save this setup"})} : {})};
}
