// @vitest-environment node
import { afterEach, beforeEach, expect, it } from "vitest";
import { sidebarDrafts } from "@/lib/api/draft-summary";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { DRAFT, FLEET, FLEET_ID, PUBLICATION, REVIEW, response } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
const CARD = {...DRAFT, linked_fleet_id: FLEET_ID, updated_at: "2026-10-19T00:00:00Z", expires_at: null,
  candidate_revision: 1, setup_revision: 1, publication_revision: 1};
const NEXT_REVISION = "0199a801-1000-7000-8000-00000000000a";
const NEXT_PUBLICATION = {...PUBLICATION, revision_id: NEXT_REVISION, head_revision_id: NEXT_REVISION};
const PROPOSED = {...DRAFT, linked_fleet_id: FLEET_ID, etag: '"candidate-two"',
  candidate: DRAFT.candidate ? {...DRAFT.candidate, revision: "2"} : undefined,
  review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
beforeEach(() => {fixture.mount();});
afterEach(() => {fixture.dispose();});

it("retains the latest proposal after leaving its chat without moving its neighbours", async () => {
  const neighbour = {...CARD, id: NEXT_REVISION};
  fixture.override = ({path}) => path.endsWith("/drafts") ? response({items: [neighbour, CARD], next_cursor: null}) : undefined;
  fixture.current = PROPOSED;
  const client = await fixture.ready(true);
  client.fresh();
  expect(client.getSnapshot().cards.map(card => card.id)).toEqual([neighbour.id, CARD.id]);
  expect(sidebarDrafts(client.getSnapshot().cards, null)[1]?.detail).toContain("Proposed v2");
});

it("records the applied setup revision for both the open chat and retained history", async () => {
  fixture.current = PROPOSED;
  fixture.override = ({path, method}) => {
    if (path.endsWith("/drafts")) return response({items: [CARD], next_cursor: null});
    if (path.endsWith("/revisions") && method === "POST") {
      fixture.fleet = {...FLEET, setup_revision: "2"};
      return response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"setup-two"', etag: '"fleet-two"', schedules: []}, 201);
    }
    return undefined;
  };
  const client = await fixture.ready(true);
  await client.apply();
  expect(client.getSnapshot().cards[0]?.setup_revision).toBe(2);
  expect(client.getSnapshot().fleet?.setup_revision).toBe("2");
  expect(client.getSnapshot().savedSetupRevision).toBe("2");
  client.fresh();
  expect(client.getSnapshot().savedSetupRevision).toBeNull();
  expect(sidebarDrafts(client.getSnapshot().cards, null)[0]?.detail).toContain("Saved v2");
});

it("retains the actual Save response's revision when the chat is left", async () => {
  fixture.override = ({path, method}) => {
    if (path.endsWith("/drafts")) return response({items: [CARD], next_cursor: null});
    if (path.endsWith("/fleets") && method === "POST") {
      fixture.current = {...DRAFT, state: "closed", linked_fleet_id: FLEET_ID};
      return response({...FLEET, setup_revision: "4"}, 201);
    }
    return undefined;
  };
  const client = await fixture.ready(true);
  await client.save(); client.fresh();
  expect(sidebarDrafts(client.getSnapshot().cards, null)[0]?.detail).toContain("Saved v4");
});

it("refreshes a publication's actual ordinal rather than incrementing a local counter", async () => {
  let published = false;
  fixture.override = ({path, method}) => {
    if (path.endsWith("/drafts")) return response({items: [{...CARD, publication_revision: published ? 7 : 1}], next_cursor: "older"});
    if (path.endsWith("/fleet-libraries") && method === "POST") {
      published = true;
      fixture.current = {...DRAFT, etag: '"published-seven"', publication: {...NEXT_PUBLICATION, canonical_digest: "seven"}};
      return response(NEXT_PUBLICATION, 201);
    }
    return undefined;
  };
  const client = await fixture.ready(true);
  await client.publish("Build repair", false);
  expect(client.getSnapshot().cards[0]?.publication_revision).toBe(7);
  expect(client.getSnapshot().moreChats).toBe(true);
  client.fresh();
  expect(sidebarDrafts(client.getSnapshot().cards, null)[0]?.detail).toContain("Published v7");
  expect(fixture.calls.filter(call => call.path.endsWith("/drafts"))).toHaveLength(2);
});

it("never presents a previous publication ordinal when the current chat has changed", async () => {
  fixture.override = ({path}) => path.endsWith("/drafts") ? response({items: [CARD], next_cursor: null}) : undefined;
  fixture.current = {...DRAFT, etag: '"published-again"', publication: {...NEXT_PUBLICATION, canonical_digest: "new"}};
  const client = await fixture.ready(true);
  const view = client.getSnapshot();
  expect(sidebarDrafts(view.cards, view.draft)[0]?.detail).toContain("Published");
  expect(sidebarDrafts(view.cards, view.draft)[0]?.detail).not.toContain("Published v1");
});
