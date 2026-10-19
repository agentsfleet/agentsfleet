// @vitest-environment node
import { afterEach, expect, it, vi } from "vitest";
import { DraftLists } from "@/lib/api/draft-lists";
import { DRAFT, FLEET, WORKSPACE, response } from "./helpers/draft-fixtures";

const CARD = {id: DRAFT.id, title: DRAFT.title, state: DRAFT.state, etag: DRAFT.etag,
  candidate_revision: 1, setup_revision: null, publication_revision: null,
  linked_fleet_id: null, updated_at: "2026-10-19T00:00:00Z", expires_at: null};
const SECOND = {...CARD, id: "0199a801-1000-7000-8000-00000000000a"};
const page = (items: unknown[], next_cursor: string | null = null) => response({items, next_cursor});
const signal = () => new AbortController().signal;
afterEach(() => {vi.restoreAllMocks();});

it("adds the actual summary of a newly created chat and preserves the original continuation", async () => {
  const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(page([])).mockResolvedValueOnce(page([SECOND], "older"))
    .mockResolvedValueOnce(page([CARD, SECOND], "newer-boundary"));
  const lists = new DraftLists(`/live/v1/workspaces/${WORKSPACE}`);
  await lists.open(signal());
  expect((await lists.refreshDraft(CARD.id, signal())).cards).toEqual([CARD, SECOND]);
  expect(lists.view.moreChats).toBe(true);
  expect(fetcher).toHaveBeenCalledTimes(3);
});

it("refreshes one existing summary without dropping neighboring chats or moving its continuation", async () => {
  const updated = {...CARD, setup_revision: 4, linked_fleet_id: FLEET.id};
  vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(page([])).mockResolvedValueOnce(page([SECOND, CARD], "older"))
    .mockResolvedValueOnce(page([updated], "different-boundary"));
  const lists = new DraftLists(`/live/v1/workspaces/${WORKSPACE}`);
  await lists.open(signal());
  expect((await lists.refreshDraft(CARD.id, signal())).cards).toEqual([SECOND, updated]);
  expect(lists.view.moreChats).toBe(true);
});

it.each(["clear", "remove"] as const)("never restores a chat when %s supersedes its pending summary", async action => {
  const pending = Promise.withResolvers<Response>();
  vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(page([])).mockResolvedValueOnce(page([CARD]))
    .mockImplementationOnce(() => pending.promise);
  const lists = new DraftLists(`/live/v1/workspaces/${WORKSPACE}`);
  await lists.open(signal());
  const refreshing = lists.refreshDraft(CARD.id, signal());
  if (action === "clear") lists.clear(); else lists.remove(CARD.id);
  pending.resolve(page([CARD]));
  expect((await refreshing).cards).toEqual([]);
});

it("keeps an observed publication through a title change and invalidates an unknown later publication", async () => {
  const publication = {library_id: "library", revision_id: SECOND.id, canonical_digest: "published"};
  const published = {...CARD, publication_revision: 7};
  vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(page([])).mockResolvedValueOnce(page([published, SECOND]));
  const lists = new DraftLists(`/live/v1/workspaces/${WORKSPACE}`);
  await lists.open(signal());
  expect(lists.updateDraft({...DRAFT, publication}).cards[0]?.publication_revision).toBe(7);
  const renamed = {...DRAFT, title: "Daily brief", etag: '"renamed"', publication};
  expect(lists.updateDraft(renamed).cards[0]?.publication_revision).toBe(7);
  expect(lists.updateDraft({...renamed, publication: {...publication, revision_id: DRAFT.id}}).cards[0]?.publication_revision).toBeNull();
  expect(lists.view.cards[1]).toEqual(SECOND);
});

it.each(["0", "9007199254740993"])("keeps inexact or invalid revision %s out of recorded summaries", async revision => {
  vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(page([])).mockResolvedValueOnce(page([CARD, SECOND]));
  const lists = new DraftLists(`/live/v1/workspaces/${WORKSPACE}`);
  await lists.open(signal());
  expect(lists.updateSetup(CARD.id, {...FLEET, setup_revision: revision}).cards[0]?.setup_revision).toBeNull();
  expect(lists.updateDraft({...DRAFT, candidate: DRAFT.candidate ? {...DRAFT.candidate, revision} : undefined}).cards[0]?.candidate_revision).toBeNull();
});
