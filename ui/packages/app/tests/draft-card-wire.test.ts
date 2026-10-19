// @vitest-environment node
import { expect, it } from "vitest";
import { DraftCardsSchema, DRAFT_STATE } from "@/lib/api/draft-types";
import { DRAFT_ID, FLEET_ID } from "./helpers/draft-fixtures";

const EDITING_CARD = {
  id: DRAFT_ID, title: "Private setup", state: DRAFT_STATE.editing,
  etag: '"draft-card"', updated_at: "1970-01-01T00:00:00Z",
  candidate_revision: null, setup_revision: null, publication_revision: null,
};
const parseCard = (fields: object) => DraftCardsSchema.parse({items: [fields], next_cursor: null}).items[0];

it("opens editing chats when the API omits an absent fleet link and retention deadline", () => {
  expect(parseCard(EDITING_CARD)).toEqual({...EDITING_CARD, linked_fleet_id: null, expires_at: null});
});

it("preserves explicit empty metadata and a closed chat's fleet link and deadline", () => {
  expect(parseCard({...EDITING_CARD, linked_fleet_id: null, expires_at: null}))
    .toEqual({...EDITING_CARD, linked_fleet_id: null, expires_at: null});
  const closed = {...EDITING_CARD, state: DRAFT_STATE.closed, linked_fleet_id: FLEET_ID, expires_at: "1970-01-01T00:00:00.123Z"};
  expect(parseCard(closed)).toEqual(closed);
});

it("refuses malformed optional metadata instead of treating it as absent", () => {
  for (const invalid of [{linked_fleet_id: "invalid"}, {expires_at: "123"}, {expires_at: 0.5}]) {
    expect(() => parseCard({...EDITING_CARD, ...invalid})).toThrow();
  }
});
