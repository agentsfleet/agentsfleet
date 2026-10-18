// @vitest-environment node
import type { DraftCard } from "@/lib/api/draft-types";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DraftLists } from "@/lib/api/draft-lists";
import { DraftClient } from "@/lib/api/drafts";
import { DRAFT_LIST, LIST_PAGE_LIMIT, MODEL_PURPOSE } from "@/lib/api/draft-list-types";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { APP_ORIGIN, DRAFT_ID, MODEL, MODEL_REF, WORKSPACE, response } from "./helpers/draft-fixtures";

type Page = {items: unknown[]; next_cursor: string | null};
type Route = (url: URL) => Response | Promise<Response>;
const MODELS = "/models";
const DRAFTS = "/drafts";
const CURSOR = "cursor-one";
const CARD = (id: string): DraftCard => ({id, title: `Chat ${id.slice(-1)}`, state: DRAFT_STATE.editing, etag: '"card"',
  linked_fleet_id: null, updated_at: "1970-01-01T00:00:00Z", expires_at: null});
const FIRST_CARD = CARD(DRAFT_ID);
const SECOND_CARD = CARD("0199a801-1000-7000-8000-00000000000a");
const urls: URL[] = [];
const clients: DraftClient[] = [];
let models: Route = () => response({items: [MODEL], next_cursor: null});
let drafts: Route = () => response({items: [FIRST_CARD], next_cursor: null});

beforeEach(() => {
  urls.length = 0;
  models = () => response({items: [MODEL], next_cursor: null});
  drafts = () => response({items: [FIRST_CARD], next_cursor: null});
  vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
    const url = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url, APP_ORIGIN);
    urls.push(url);
    if (url.pathname.endsWith(MODELS)) return models(url);
    if (url.pathname.endsWith(DRAFTS)) return drafts(url);
    throw new Error(`Unexpected list request ${url.pathname}`);
  }));
});
afterEach(() => { for (const client of clients.splice(0)) client.dispose(); vi.unstubAllGlobals(); });

async function opened(): Promise<DraftClient> {
  const client = new DraftClient({account: "owner-account", workspace: WORKSPACE, subject: "creator"});
  clients.push(client);
  await client.initialize();
  return client;
}
const page = (items: unknown[], next_cursor: string | null): Response => response({items, next_cursor} satisfies Page);
const requests = (path: string): URL[] => urls.filter((url) => url.pathname.endsWith(path));
const deferred = () => { let resolve: (value: Response) => void = () => undefined;
  const promise = new Promise<Response>((done) => { resolve = done; }); return {promise, resolve}; };

describe("private chat lists", () => {
  it("retains the list outcome when starting a fresh conversation", async () => {
    const client = await opened();
    drafts = () => response({}, 503);
    await expect(client.initialize()).rejects.toMatchObject({status: 503});
    expect(client.getSnapshot()).toMatchObject({listStatus: "unavailable"});
    client.fresh();
    expect(client.getSnapshot()).toMatchObject({listStatus: "unavailable"});
    drafts = () => page([], null);
    await client.initialize();
    await expect(client.retry()).rejects.toMatchObject({status: 409});
    expect(client.getSnapshot()).toMatchObject({listStatus: "ready", cards: []});
  });
  it("opens with one bounded page of authoring models and one of chats", async () => {
    const client = await opened();
    const [model] = requests(MODELS);
    expect(model?.searchParams.get("purpose")).toBe(MODEL_PURPOSE.authoring);
    expect(model?.searchParams.get("limit")).toBe(String(LIST_PAGE_LIMIT));
    expect(model?.searchParams.has("starting_after")).toBe(false);
    expect(requests(DRAFTS).map((url) => url.searchParams.get("limit"))).toEqual([String(LIST_PAGE_LIMIT)]);
    expect(client.getSnapshot()).toMatchObject({model: MODEL_REF, moreModels: false, moreChats: false, cards: [FIRST_CARD]});
  });

  it("loads the next chat page only when asked, from the cursor the last page returned", async () => {
    drafts = (url) => url.searchParams.get("starting_after") === CURSOR ? page([FIRST_CARD, SECOND_CARD], null) : page([FIRST_CARD], CURSOR);
    const client = await opened();
    expect(requests(DRAFTS)).toHaveLength(1);
    expect(client.getSnapshot().moreChats).toBe(true);
    await client.loadMore(DRAFT_LIST.chats);
    expect(requests(DRAFTS)[1]?.searchParams.get("starting_after")).toBe(CURSOR);
    expect(client.getSnapshot().cards.map((card) => card.id)).toEqual([FIRST_CARD.id, SECOND_CARD.id]);
    expect(client.getSnapshot().moreChats).toBe(false);
    await client.loadMore(DRAFT_LIST.chats);
    expect(requests(DRAFTS)).toHaveLength(2);
  });

  it("keeps the authoring purpose on every model page", async () => {
    models = (url) => url.searchParams.has("starting_after") ? page([{...MODEL, ref: "model:second"}], null) : page([MODEL], CURSOR);
    const client = await opened();
    await client.loadMore(DRAFT_LIST.models);
    expect(requests(MODELS).map((url) => url.searchParams.get("purpose"))).toEqual([MODEL_PURPOSE.authoring, MODEL_PURPOSE.authoring]);
    expect(client.getSnapshot().models.map((model) => model.ref)).toEqual([MODEL_REF, "model:second"]);
    expect(client.getSnapshot()).toMatchObject({model: MODEL_REF, moreModels: false});
  });

  it("skips filtered-out model pages to the first choice, within a fixed bound", async () => {
    let served = 0;
    models = () => { served += 1; return served < 3 ? page([], `${CURSOR}-${served}`) : page([MODEL], null); };
    expect((await opened()).getSnapshot()).toMatchObject({model: MODEL_REF, moreModels: false});
    expect(requests(MODELS)).toHaveLength(3);
    urls.length = 0; models = () => page([], CURSOR);
    const empty = await opened();
    expect(requests(MODELS)).toHaveLength(4);
    expect(empty.getSnapshot()).toMatchObject({models: [], model: "", moreModels: true});
  });

  it("shares one request between concurrent loads of the same list", async () => {
    const next = deferred();
    drafts = (url) => url.searchParams.has("starting_after") ? next.promise : page([FIRST_CARD], CURSOR);
    const client = await opened();
    const first = client.loadMore(DRAFT_LIST.chats);
    const second = client.loadMore(DRAFT_LIST.chats);
    next.resolve(page([SECOND_CARD], null));
    await Promise.all([first, second]);
    expect(requests(DRAFTS)).toHaveLength(2);
    expect(client.getSnapshot().cards).toHaveLength(2);
  });

  it("drops a page that a reopen replaced while it was loading", async () => {
    const stale = deferred();
    drafts = (url) => url.searchParams.has("starting_after") ? stale.promise : page([FIRST_CARD], CURSOR);
    const client = await opened();
    const loading = client.loadMore(DRAFT_LIST.chats);
    drafts = () => page([FIRST_CARD], null);
    await client.initialize();
    stale.resolve(page([SECOND_CARD], null));
    await loading;
    expect(client.getSnapshot().cards.map((card) => card.id)).toEqual([FIRST_CARD.id]);
    expect(client.getSnapshot().moreChats).toBe(false);
  });

  it("drops an older model page after reopening with a different model", async () => {
    const stale = deferred();
    models = (url) => url.searchParams.has("starting_after") ? stale.promise : page([MODEL], CURSOR);
    const client = await opened();
    const loading = client.loadMore(DRAFT_LIST.models);
    models = () => page([{...MODEL, ref: "model:current"}], null);
    await client.initialize();
    stale.resolve(page([{...MODEL, ref: "model:stale"}], null)); await loading;
    expect(client.getSnapshot()).toMatchObject({model: "model:current", moreModels: false});
    expect(client.getSnapshot().models.map((model) => model.ref)).toEqual(["model:current"]);
  });

  it.each([[DRAFT_LIST.chats, 403], [DRAFT_LIST.models, 410], [DRAFT_LIST.chats, 503]])(
    "ignores a superseded %s page failure (%i) after successful refresh", async (list, status) => {
      const stale = deferred();
      const route: Route = (url) => url.searchParams.has("starting_after") ? stale.promise
        : page(list === DRAFT_LIST.models ? [MODEL] : [FIRST_CARD], CURSOR);
      if (list === DRAFT_LIST.models) models = route; else drafts = route;
      const client = await opened();
      const loading = client.loadMore(list);
      models = () => page([MODEL], null); drafts = () => page([FIRST_CARD], null);
      await client.initialize();
      stale.resolve(response({}, status));
      await expect(loading).resolves.toBeUndefined();
      expect(client.getSnapshot()).toMatchObject({inaccessible: false, listStatus: "ready", cards: [FIRST_CARD], models: [MODEL], error: null});
    });

  it("re-throws a failed page for the caller to show, without clearing what is already loaded", async () => {
    drafts = (url) => url.searchParams.has("starting_after") ? response({}, 503) : page([FIRST_CARD], CURSOR);
    const client = await opened();
    await expect(client.loadMore(DRAFT_LIST.chats)).rejects.toMatchObject({status: 503});
    expect(client.getSnapshot()).toMatchObject({cards: [FIRST_CARD], moreChats: true, inaccessible: false, busy: false});
    expect(client.getSnapshot().error).toBeNull();
  });

  it("clears private lists when a page reveals lost access, and loads nothing after", async () => {
    drafts = (url) => url.searchParams.has("starting_after") ? response({}, 403) : page([FIRST_CARD], CURSOR);
    const client = await opened();
    await expect(client.loadMore(DRAFT_LIST.chats)).rejects.toMatchObject({status: 403});
    expect(client.getSnapshot()).toMatchObject({inaccessible: true, cards: [], models: [], moreChats: false, moreModels: false});
    const before = urls.length;
    await client.loadMore(DRAFT_LIST.chats);
    expect(urls).toHaveLength(before);
  });
});


it("retains the newer opening when an earlier opening resolves last", async () => {
  const older = Promise.withResolvers<Response>();
  let openings = 0;
  models = () => ++openings === 1 ? older.promise : page([{...MODEL, ref: "model:new"}], null);
  const lists = new DraftLists(`/live/v1/workspaces/${WORKSPACE}`);
  const signal = new AbortController().signal;
  const first = lists.open(signal);
  const latest = await lists.open(signal);
  older.resolve(page([MODEL], CURSOR));
  await expect(first).resolves.toEqual(latest);
  expect(lists.view.models.map(({ref}) => ref)).toEqual(["model:new"]);
  expect(lists.view.moreModels).toBe(false);
});
