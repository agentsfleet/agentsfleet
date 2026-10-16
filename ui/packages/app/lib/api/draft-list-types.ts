// Shared list choices stay free of schema and transport dependencies so the sidebar stays light.
import type { DraftCard, WorkspaceModel } from "./draft-types";

export const DRAFT_LIST = {models: "models", chats: "chats"} as const;
export const DRAFT_LIST_STATUS = {loading: "loading", ready: "ready", unavailable: "unavailable"} as const;
export const MODEL_PURPOSE = {authoring: "authoring"} as const;
export const LIST_PAGE_LIMIT = 50;
export type DraftList = (typeof DRAFT_LIST)[keyof typeof DRAFT_LIST];
export type DraftListsView = {
  readonly models: readonly WorkspaceModel[];
  readonly cards: readonly DraftCard[];
  readonly moreModels: boolean;
  readonly moreChats: boolean;
};
