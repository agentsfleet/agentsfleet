// Private chat summaries use only recorded versions; no transcript is needed.

import { Effect, Schema } from "effect";
import { ValidationError } from "../../errors/index.ts";
import { isInstant } from "../../lib/instant.ts";
import { DRAFT_STATE, decode } from "./protocol.ts";

export const CHAT_PAGE_MAX = 100;
const SEARCH_MAX = 200;
const TITLE_MAX = 100;
export const CHAT_TEXT_KIND = {title: "title", search: "search"} as const;
const text = Schema.String.check(Schema.isMinLength(1));
const instant = Schema.String.check(Schema.makeFilter(isInstant));
const revision = Schema.NullOr(Schema.Number.check(Schema.makeFilter(value => Number.isSafeInteger(value) && value > 0)));
const card = Schema.Struct({
  id: text, title: Schema.String, etag: text, state: Schema.Literals(Object.values(DRAFT_STATE)),
  linked_fleet_id: Schema.optionalKey(Schema.NullOr(text)), updated_at: instant,
  candidate_revision: revision, setup_revision: revision, publication_revision: revision,
});
const page = Schema.Struct({items: Schema.Array(card).check(Schema.isMaxLength(CHAT_PAGE_MAX)), next_cursor: Schema.NullOr(text)});
export type ChatCard = typeof card.Type;
export const decodeChatPage = (value: unknown) => decode(page, value);
export const decodeChatTitle = (id: string, value: unknown) => decode(Schema.Struct({id: Schema.Literal(id), title: text, etag: text}), value);

export function chatText(value: string, kind: keyof typeof CHAT_TEXT_KIND) {
  const trimmed = value.trim();
  const max = kind === CHAT_TEXT_KIND.title ? TITLE_MAX : SEARCH_MAX;
  return trimmed.length > 0 && [...trimmed].length <= max && !trimmed.includes("\0")
    ? Effect.succeed(trimmed) : Effect.fail(new ValidationError({
      detail: `Chat ${kind} must contain 1..${max} characters and no NUL`,
      suggestion: `pass a non-empty ${kind} within that limit`,
    }));
}

export function chatStatus(row: ChatCard): string {
  if (row.state === DRAFT_STATE.closed) return row.linked_fleet_id ? "Fleet saved" : "Chat closed";
  return row.state === DRAFT_STATE.saving ? "Saving fleet" : row.candidate_revision === null ? "In progress" : "Setup ready";
}

export function chatVersions(row: ChatCard): string {
  return [row.candidate_revision === null ? null : `Proposed v${row.candidate_revision}`,
    row.setup_revision === null ? null : `Saved v${row.setup_revision}`,
    row.publication_revision === null ? null : `Published v${row.publication_revision}`].filter(value => value !== null).join(" · ");
}
