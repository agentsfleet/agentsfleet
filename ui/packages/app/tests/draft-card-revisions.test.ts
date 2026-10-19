// @vitest-environment node
import { expect, it } from "vitest";
import { DraftCardsSchema } from "@/lib/api/draft-types";
import { sidebarDrafts } from "@/lib/api/draft-summary";
import { DRAFT_ID, FLEET_ID } from "./helpers/draft-fixtures";

const CARD = {id: DRAFT_ID, title: "Watch releases", state: "editing", etag: '"one"',
  linked_fleet_id: null, updated_at: "2026-10-19T00:00:00Z", expires_at: null,
  candidate_revision: null, setup_revision: null, publication_revision: null};
const parse = (card: unknown) => DraftCardsSchema.parse({items: [card], next_cursor: null}).items;
const REVISIONS = ["candidate_revision", "setup_revision", "publication_revision"] as const;

it("preserves all three recorded revisions and explicit nulls", () => {
  expect(parse(CARD)).toEqual([CARD]);
  const versioned = {...CARD, candidate_revision: 3, setup_revision: 2, publication_revision: 7};
  expect(parse(versioned)).toEqual([versioned]);
});

it.each(REVISIONS)("refuses absent or inexact %s", field => {
  for (const invalid of [undefined, "1", 0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1]) {
    expect(() => parse({...CARD, [field]: invalid})).toThrow();
  }
});

it("explains proposed, saved and published versions without equating their counters", () => {
  expect(sidebarDrafts(parse(CARD), null)[0]?.detail).toBe("In progress");
  expect(sidebarDrafts(parse({...CARD, candidate_revision: 3}), null)[0]?.detail).toBe("Setup ready · Proposed v3");
  expect(sidebarDrafts(parse({...CARD, state: "closed", linked_fleet_id: FLEET_ID,
    candidate_revision: 3, setup_revision: 2, publication_revision: 7}), null)[0]?.detail)
    .toBe("Fleet saved · Saved v2 · Published v7");
});
