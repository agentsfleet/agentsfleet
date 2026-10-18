import type { Draft, DraftCard, Message } from "./draft-types";
import { DRAFT_STATE, MESSAGE_ROLE } from "./draft-types";

const DEFAULT_TITLE = "new fleet";
export const CHAT_TITLE_LIMIT = 100;
const CHAT_STATE_LABEL = {editing: "In progress", ready: "Setup ready", saving: "Saving fleet", saved: "Fleet saved", closed: "Chat closed"} as const;
export type SidebarDraft = Pick<DraftCard, "id" | "title"> & {readonly detail: string};

export function chatTitle(title: string, id: string, description = ""): string {
  if (title.trim() && title.trim().toLowerCase() !== DEFAULT_TITLE) return title;
  const purpose = description.replace(/\s+/g, " ").trim();
  return purpose ? Array.from(purpose).slice(0, CHAT_TITLE_LIMIT).join("") : `Setup chat ${id.slice(-6).toUpperCase()}`;
}

export function sidebarDrafts(cards: readonly DraftCard[], draft: Draft | null, firstRequest = ""): readonly SidebarDraft[] {
  const current = draft ? {
    id: draft.id, title: chatTitle(draft.title, draft.id, firstRequest || draft.candidate?.definition.description),
    detail: [chatState(draft.state, !!draft.candidate, !!draft.linked_fleet_id), draft.candidate ? `Setup v${draft.candidate.revision}` : "",
      draft.publication ? "Published" : ""].filter(Boolean).join(" · "),
  } : null;
  const rows = cards.map((card) => current?.id === card.id ? current : {
    id: card.id, title: chatTitle(card.title, card.id), detail: chatState(card.state, false, !!card.linked_fleet_id),
  });
  return current && !cards.some((card) => card.id === current.id) ? [current, ...rows] : rows;
}

function chatState(state: DraftCard["state"], candidate: boolean, linked: boolean): string {
  if (state === DRAFT_STATE.closed) return linked ? CHAT_STATE_LABEL.saved : CHAT_STATE_LABEL.closed;
  if (state === DRAFT_STATE.saving) return CHAT_STATE_LABEL.saving;
  return candidate ? CHAT_STATE_LABEL.ready : CHAT_STATE_LABEL.editing;
}

export function firstChatRequest(messages: readonly Message[]): string {
  return messages.find((message) => message.role === MESSAGE_ROLE.user)?.text ?? "";
}
