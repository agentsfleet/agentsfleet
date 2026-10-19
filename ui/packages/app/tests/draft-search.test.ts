// @vitest-environment node
import { afterEach, expect, it, vi } from "vitest";
import { DraftSearch } from "@/lib/api/draft-search";
import { DRAFT_SEARCH_QUERY_MAX } from "@/lib/api/draft-vocabulary";
import { DRAFT, WORKSPACE, response } from "./helpers/draft-fixtures";

const CARD = {...DRAFT, candidate_revision: 1, setup_revision: null, publication_revision: null,
  linked_fleet_id: null, updated_at: "2026-10-19T00:00:00Z", expires_at: null};
const SECOND = {...CARD, id: "0199a801-1000-7000-8000-00000000000a", title: "Second chat"};
const THIRD = {...CARD, id: "0199a801-1000-7000-8000-00000000000b", title: "Third chat"};
const CURSOR = "opaque+query/cursor";
const owners: DraftSearch[] = [];
const owner = () => {const search = new DraftSearch(WORKSPACE); owners.push(search); return search;};
const page = (items: unknown[], next_cursor: string | null = null) => response({items, next_cursor});
afterEach(() => {owners.splice(0).forEach(search => search.cancel()); vi.restoreAllMocks();});

it("keeps the trimmed literal query on every page and coalesces repeated more actions", async () => {
  const pending = Promise.withResolvers<Response>();
  const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(page([CARD], CURSOR)).mockImplementationOnce(() => pending.promise);
  const search = owner();
  await search.search("  failures_%  ");
  const first = search.more();
  expect(search.more()).toBe(first);
  pending.resolve(page([CARD, SECOND, SECOND]));
  await first;
  expect(search.getSnapshot().items.map(card => card.id)).toEqual([CARD.id, SECOND.id]);
  const queries = fetcher.mock.calls.map(([input]) => new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url, "https://app.test"));
  expect(queries.map(url => url.searchParams.get("q"))).toEqual(["failures_%", "failures_%"]);
  expect(queries[1]?.searchParams.get("starting_after")).toBe(CURSOR);
  expect(queries.every(url => url.searchParams.get("limit") === "50")).toBe(true);
  await search.more(); expect(fetcher).toHaveBeenCalledTimes(2);
});

it("ignores a superseded private query's late access refusal", async () => {
  const stale = Promise.withResolvers<Response>();
  const fetcher = vi.spyOn(globalThis, "fetch").mockImplementationOnce(() => stale.promise).mockResolvedValueOnce(page([SECOND]));
  const search = owner(); const first = search.search("older");
  const signal = fetcher.mock.calls[0]?.[1]?.signal;
  await search.search("newer");
  expect(signal?.aborted).toBe(true);
  stale.resolve(response({}, 403)); await first;
  expect(search.getSnapshot()).toMatchObject({query: "newer", status: "ready", items: [expect.objectContaining({id: SECOND.id})]});
});

it("ignores a prior continuation after changing query and never restores a deleted match", async () => {
  const stale = Promise.withResolvers<Response>();
  const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(page([CARD], CURSOR))
    .mockImplementationOnce(() => stale.promise).mockResolvedValueOnce(page([SECOND], CURSOR)).mockResolvedValueOnce(page([SECOND, THIRD]));
  const search = owner(); await search.search("older"); const more = search.more();
  await search.search("newer"); search.remove(SECOND.id);
  stale.resolve(page([CARD])); await more;
  expect(search.getSnapshot().items).toEqual([]);
  await search.more();
  expect(search.getSnapshot().items.map(card => card.id)).toEqual([THIRD.id]);
  expect(fetcher).toHaveBeenCalledTimes(4);
});

it("retains matches and the exact continuation through a temporary page failure", async () => {
  const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(page([CARD], CURSOR))
    .mockResolvedValueOnce(response({}, 503)).mockResolvedValueOnce(page([SECOND]));
  const search = owner(); await search.search("failures");
  await expect(search.more()).rejects.toMatchObject({status: 503});
  expect(search.getSnapshot()).toMatchObject({status: "ready", items: [expect.objectContaining({id: CARD.id})], next_cursor: CURSOR});
  await search.more();
  expect(fetcher.mock.calls[1]?.[0]).toBe(fetcher.mock.calls[2]?.[0]);
  expect(search.getSnapshot().items).toHaveLength(2);
});

it("replaces a refreshed page atomically and holds its old continuation until that response lands", async () => {
  const stale = Promise.withResolvers<Response>();
  const refreshed = Promise.withResolvers<Response>();
  const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(page([CARD], CURSOR))
    .mockImplementationOnce(() => stale.promise).mockImplementationOnce(() => refreshed.promise);
  const search = owner(); await search.search("failures");
  const items = search.getSnapshot().items;
  const more = search.more(); const refresh = search.search("failures");
  expect(search.getSnapshot()).toMatchObject({status: "ready", next_cursor: CURSOR});
  expect(search.getSnapshot().items).toBe(items);
  expect(search.more()).toBe(refresh);
  stale.resolve(page([SECOND])); await more;
  expect(search.getSnapshot().items).toBe(items);
  refreshed.resolve(page([THIRD])); await refresh;
  expect(search.getSnapshot()).toMatchObject({status: "ready", items: [expect.objectContaining({id: THIRD.id})], next_cursor: null});
  expect(fetcher).toHaveBeenCalledTimes(3);
});

it("keeps matches after a failed refresh but clears them if retry discovers access was revoked", async () => {
  vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(page([CARD], CURSOR))
    .mockResolvedValueOnce(response({}, 503)).mockResolvedValueOnce(response({}, 403));
  const search = owner(); await search.search("failures"); await search.search("failures");
  expect(search.getSnapshot()).toMatchObject({status: "failed", items: [expect.objectContaining({id: CARD.id})], next_cursor: CURSOR});
  await search.search("failures");
  expect(search.getSnapshot()).toMatchObject({status: "inaccessible", items: [], next_cursor: null});
});

it.each([401, 403, 404, 410])("clears private results on a current %i refusal", async status => {
  const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(page([CARD], CURSOR)).mockResolvedValueOnce(response({}, status));
  const search = owner(); await search.search("failures"); await search.more();
  expect(search.getSnapshot()).toMatchObject({status: "inaccessible", items: [], next_cursor: null});
  await search.more(); expect(fetcher).toHaveBeenCalledTimes(2);
});

it.each(["", "   ", "has\0nul", "x".repeat(DRAFT_SEARCH_QUERY_MAX + 1)])("refuses invalid query %s before the request", async query => {
  const fetcher = vi.spyOn(globalThis, "fetch"); const search = owner();
  await search.search(query);
  expect(search.getSnapshot()).toMatchObject({status: "failed", items: [], next_cursor: null});
  expect(fetcher).not.toHaveBeenCalled();
});

it("distinguishes an unreadable first response from no matches and can retry", async () => {
  vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(response({items: [{}], next_cursor: null})).mockResolvedValueOnce(page([]));
  const search = owner(); await search.search("failures");
  expect(search.getSnapshot()).toMatchObject({status: "failed", items: []});
  await search.search("failures"); expect(search.getSnapshot()).toMatchObject({status: "ready", items: []});
});

it("publishes no late result after cancellation and releases subscribers", async () => {
  const pending = Promise.withResolvers<Response>();
  vi.spyOn(globalThis, "fetch").mockImplementationOnce(() => pending.promise).mockResolvedValueOnce(page([SECOND]));
  const search = owner(); const listener = vi.fn(); const unsubscribe = search.subscribe(listener);
  const opening = search.search("failures"); expect(listener).toHaveBeenCalledTimes(1);
  search.cancel(); pending.resolve(page([CARD])); await opening;
  expect(search.getSnapshot().items).toEqual([]); expect(listener).toHaveBeenCalledTimes(1);
  unsubscribe(); await search.search("newer"); expect(listener).toHaveBeenCalledTimes(1);
});
