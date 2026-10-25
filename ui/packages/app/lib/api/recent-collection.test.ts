import { afterEach, expect, it, vi } from "vitest";
import { z } from "zod";
import { RecentCollection } from "./recent-collection";

const schema = z.object({items: z.array(z.object({id: z.string(), name: z.string()})), next_cursor: z.string().nullable()});
const path = "/live/v1/workspaces/workspace/fleets";
const collections: RecentCollection<{id: string; name: string}>[] = [];
function collection() {
  const current = new RecentCollection(path, schema);
  collections.push(current);
  return current;
}
const row = (id: string) => ({id, name: `Private ${id}`});
afterEach(() => {for (const current of collections) current.dispose(); collections.length = 0; vi.unstubAllGlobals();});

it("coalesces simultaneous reads, validates pages, and deduplicates a continued walk", async () => {
  const pending = Promise.withResolvers<Response>();
  const fetcher = vi.fn().mockReturnValueOnce(pending.promise)
    .mockResolvedValueOnce(Response.json({items: [row("first"), row("next")], next_cursor: "opaque"}));
  vi.stubGlobal("fetch", fetcher);
  const current = collection();
  const first = current.load();
  expect(current.load()).toBe(first);
  expect(current.getSnapshot()).toMatchObject({loading: true, items: []});
  pending.resolve(Response.json({items: [row("first")], next_cursor: "opaque"}));
  await first;
  expect(current.getSnapshot()).toMatchObject({loading: false, more: true, items: [row("first")]});
  await current.load(true);
  expect(current.getSnapshot()).toMatchObject({loading: false, more: false, items: [row("first"), row("next")]});
  expect(fetcher.mock.calls.map(call => call[0])).toEqual([`${path}?limit=50`, `${path}?limit=50&starting_after=opaque`]);
  await current.load(true);
  expect(fetcher).toHaveBeenCalledTimes(2);
});

it("does not resurrect a deleted row when an older page arrives later", async () => {
  const pending = Promise.withResolvers<Response>();
  vi.stubGlobal("fetch", vi.fn().mockReturnValue(pending.promise));
  const current = collection();
  const reading = current.load(); current.remove("deleted");
  pending.resolve(Response.json({items: [row("deleted"), row("kept")], next_cursor: null}));
  await reading;
  expect(current.getSnapshot().items).toEqual([row("kept")]);
});

it.each([401, 403, 404, 410])("clears every private row and cursor when access ends (%s)", async status => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(Response.json({items: [row("old")], next_cursor: "next"}))
    .mockResolvedValueOnce(Response.json({user_message: "private refusal"}, {status})));
  const current = collection(); await current.load(); await current.load(true);
  expect(current.getSnapshot()).toEqual({items: [], more: false, loading: false, error: "Fleets could not be loaded."});
});

it.each([Response.json({}, {status: 503}), Response.json({items: [{id: 4}], next_cursor: null})])("retains loaded rows after an unavailable or invalid refresh (%#)", async failure => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(Response.json({items: [row("kept")], next_cursor: null}))
    .mockResolvedValueOnce(failure));
  const current = collection(); await current.load(); await current.load();
  expect(current.getSnapshot()).toMatchObject({items: [row("kept")], loading: false, error: "Fleets could not be loaded."});
});

it("aborts on identity disposal and ignores an upstream that completes despite abort", async () => {
  const pending = Promise.withResolvers<Response>();
  const fetcher = vi.fn().mockReturnValue(pending.promise); vi.stubGlobal("fetch", fetcher);
  const current = collection(); const listener = vi.fn(); const release = current.subscribe(listener);
  const reading = current.load(); const signal = fetcher.mock.calls[0]![1].signal as AbortSignal;
  current.dispose(); listener.mockClear();
  expect(signal.aborted).toBe(true);
  pending.resolve(Response.json({items: [row("old owner")], next_cursor: "next"}));
  await reading; release();
  expect(listener).not.toHaveBeenCalled();
  expect(current.getSnapshot()).toEqual({items: [], more: false, loading: false, error: null});
});

it("keeps separate mounted identities independent and releases snapshot listeners", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => Response.json({items: [row("owner")], next_cursor: null})));
  const first = collection(); const second = collection(); const listener = vi.fn();
  const release = first.subscribe(listener); await first.load();
  expect(second.getSnapshot().items).toEqual([]);
  expect(listener).toHaveBeenCalled(); release(); listener.mockClear(); first.remove("owner");
  expect(first.getSnapshot().items).toEqual([]); expect(listener).not.toHaveBeenCalled();
});
