import type { Draft, DraftCard, Message } from "./draft-types";
import { DRAFT_STATE, MESSAGE_ROLE } from "./draft-vocabulary";

const DEFAULT_TITLE = "new fleet";
export const CHAT_TITLE_LIMIT = 100;
const CHAT_STATE_LABEL = {editing: "In progress", ready: "Setup ready", saving: "Saving fleet", saved: "Fleet saved", closed: "Chat closed"} as const;
export const CHAT_ACTIVITY = {draft: "draft", saved: "saved", pending: "pending", failed: "failed"} as const;
export type ChatActivity = typeof CHAT_ACTIVITY[keyof typeof CHAT_ACTIVITY];
export type SidebarDraft = Pick<DraftCard, "id" | "title"> & {readonly detail: string; readonly activity: ChatActivity; readonly linkedFleet?: string | null};

export function chatTitle(title: string, id: string, description = ""): string {
  if (title.trim() && title.trim().toLowerCase() !== DEFAULT_TITLE) return title;
  const purpose = description.replace(/\s+/g, " ").trim();
  return purpose ? Array.from(purpose).slice(0, CHAT_TITLE_LIMIT).join("") : `Setup chat ${id.slice(-6).toUpperCase()}`;
}

export function sidebarDrafts(cards: readonly DraftCard[], draft: Draft | null, firstRequest = "", setupRevision: string | null = null, activity?: ChatActivity): readonly SidebarDraft[] {
  const recorded = cards.find(card => card.id === draft?.id);
  const current = draft ? {
    id: draft.id, title: chatTitle(draft.title, draft.id, firstRequest || draft.candidate?.definition.description),
    linkedFleet: draft.linked_fleet_id ?? null,
    activity: activity ?? chatActivity(draft.state, !!draft.linked_fleet_id),
    detail: chatDetail({state: draft.state, linked: !!draft.linked_fleet_id, candidate: draft.candidate?.revision ?? null,
      setup: setupRevision ?? recorded?.setup_revision ?? null, publication: recorded?.publication_revision ?? null, published: !!draft.publication}),
  } : null;
  const rows = cards.map((card) => current?.id === card.id ? current : {
    id: card.id, title: chatTitle(card.title, card.id), linkedFleet: card.linked_fleet_id, detail: chatDetail({state: card.state, linked: !!card.linked_fleet_id,
      candidate: card.candidate_revision, setup: card.setup_revision, publication: card.publication_revision, published: false}),
    activity: chatActivity(card.state, !!card.linked_fleet_id),
  });
  return current && !cards.some((card) => card.id === current.id) ? [current, ...rows] : rows;
}

function chatActivity(state: DraftCard["state"], linked: boolean): ChatActivity {
  return state === DRAFT_STATE.saving ? CHAT_ACTIVITY.pending : state === DRAFT_STATE.closed && linked ? CHAT_ACTIVITY.saved : CHAT_ACTIVITY.draft;
}

type ChatFacts = {state: DraftCard["state"]; linked: boolean; candidate: string | number | null;
  setup: string | number | null; publication: number | null; published: boolean};

function chatDetail(facts: ChatFacts): string {
  const saved = facts.state === DRAFT_STATE.closed && facts.linked && facts.setup !== null;
  return [saved ? "" : chatState(facts.state, facts.candidate !== null, facts.linked),
    facts.state !== DRAFT_STATE.closed && facts.candidate !== null ? `Proposed v${facts.candidate}` : "",
    facts.setup !== null ? `Saved v${facts.setup}` : "",
    facts.publication !== null ? `Published v${facts.publication}` : facts.published ? "Published" : ""].filter(Boolean).join(" · ");
}

function chatState(state: DraftCard["state"], candidate: boolean, linked: boolean): string {
  if (state === DRAFT_STATE.closed) return linked ? CHAT_STATE_LABEL.saved : CHAT_STATE_LABEL.closed;
  if (state === DRAFT_STATE.saving) return CHAT_STATE_LABEL.saving;
  return candidate ? CHAT_STATE_LABEL.ready : CHAT_STATE_LABEL.editing;
}

export function firstChatRequest(messages: readonly Message[]): string {
  return messages.find((message) => message.role === MESSAGE_ROLE.user)?.text ?? "";
}
