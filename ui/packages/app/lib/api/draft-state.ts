import type { Definition, Draft, DraftCard, Fleet, Message, Schedule, WorkspaceModel } from "./draft-types";
import { DRAFT_LIST_STATUS } from "./draft-list-types";
import type { DraftReply } from "./draft-replies";
import type { DraftCompaction } from "./draft-live";
import type { SetupActivity } from "./draft-activity";

export type DraftIdentity = {readonly account: string; readonly workspace: string; readonly subject: string};
export type DraftView = {
  readonly draft: Draft | null;
  readonly messages: readonly Message[];
  readonly previousDrafts?: readonly {readonly id: string; readonly cancelledThrough: string; readonly messages: readonly Message[]}[];
  readonly activities?: readonly SetupActivity[];
  readonly liveReply?: DraftReply | null;
  readonly contextProgress?: DraftCompaction | null;
  readonly optimistic?: {readonly operation: string; readonly text: string; readonly createdAt: string} | null;
  readonly models: readonly WorkspaceModel[];
  readonly cards: readonly DraftCard[];
  readonly moreModels: boolean;
  readonly moreChats: boolean;
  readonly listStatus: typeof DRAFT_LIST_STATUS[keyof typeof DRAFT_LIST_STATUS];
  readonly model: string;
  readonly preview: {readonly definition: Definition; readonly instanceName: string; readonly revision: string; readonly setupRevision?: string} | null;
  readonly savedSetupRevision: string | null;
  readonly saveOperation?: string | null;
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
