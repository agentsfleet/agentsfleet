// Shared list choices stay free of schema and transport dependencies so the sidebar stays light.
import type { DraftCard, WorkspaceModel } from "./draft-types";

export const DRAFT_LIST = {models: "models", chats: "chats"} as const;
export const DRAFT_LIST_STATUS = {loading: "loading", ready: "ready", unavailable: "unavailable"} as const;
export const MODEL_LIST_RETRYING = "retrying";
export const LIST_PAGE_LIMIT = 50;
export type DraftList = (typeof DRAFT_LIST)[keyof typeof DRAFT_LIST];
export type DraftListStatus = (typeof DRAFT_LIST_STATUS)[keyof typeof DRAFT_LIST_STATUS];
export type DraftListsView = {
  readonly models: readonly WorkspaceModel[];
  readonly cards: readonly DraftCard[];
  readonly moreModels: boolean;
  readonly moreChats: boolean;
};

export function defaultChatModel(models: readonly WorkspaceModel[], current: string): string {
  return models.find(model => model.ref === current && model.eligibility.setup)?.ref
    ?? models.find(model => model.eligibility.setup)?.ref ?? "";
}

const PLATFORM_DEFAULT = "platform-default";

/** A saved fleet may omit its model when it uses the workspace platform default. */
export function runtimeModelLabel(models: readonly WorkspaceModel[] | null, selected: unknown): string {
  const ref = typeof selected === "string" ? selected : PLATFORM_DEFAULT;
  return models?.find(model => model.ref === ref)?.label
    ?? (ref === PLATFORM_DEFAULT ? "Platform default model" : ref);
}
