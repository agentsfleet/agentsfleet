import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DraftCache } from "@/lib/api/draft-cache";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { DRAFT, MESSAGE } from "./helpers/draft-fixtures";

const view = {...EMPTY_DRAFT_VIEW, draft: DRAFT, messages: [MESSAGE]};
const CACHE_LIMIT_BYTES = 1_048_576;
beforeEach(() => {vi.useFakeTimers(); vi.setSystemTime(0);});
afterEach(() => vi.useRealTimers());

it("caches only durable fields and expires them after the bounded lifetime", () => {
  const cache = new DraftCache();
  cache.remember({...view, optimistic: {text: "private pending", operation: "op", createdAt: "now"}});
  expect(Object.keys(cache.read(DRAFT.id)!)).toEqual(["draft", "messages", "model", "truncated"]);
  vi.advanceTimersByTime(120_000);
  expect(cache.read(DRAFT.id)).toBeUndefined();
});

it("evicts oldest chats and never carries private snapshots between owners", () => {
  const first = new DraftCache(); const second = new DraftCache();
  for (let index = 0; index < 6; index += 1) first.remember({...view, draft: {...DRAFT, id: `chat-${index}`}});
  expect(first.read("chat-0")).toBeUndefined();
  expect(first.read("chat-1")).toBeDefined();
  expect(second.read("chat-1")).toBeUndefined();
  first.remember({...view, inaccessible: true});
  expect(first.read("chat-1")).toBeUndefined();
});

it("bounds UTF-8 bytes and replaces old snapshots without retaining obsolete messages", () => {
  const cache = new DraftCache(); cache.remember(view);
  cache.remember({...view, messages: [{...MESSAGE, text: "🌿".repeat(300_000)}]});
  expect(cache.read(DRAFT.id)).toBeUndefined();
  cache.remember(view); cache.clear();
  expect(cache.read(DRAFT.id)).toBeUndefined();
  cache.remember({...view, draft: null});
  expect(cache.read(DRAFT.id)).toBeUndefined();
});

it("enforces encoded size even when Unicode text fits the character limit", () => {
  const cache = new DraftCache();
  cache.remember({...view, messages: [{...MESSAGE, text: "🌿".repeat(270_000)}]});
  expect(cache.read(DRAFT.id)).toBeUndefined();
  cache.remember({...view, messages: [{...MESSAGE, text: "x".repeat(CACHE_LIMIT_BYTES + 1)}]});
  expect(cache.read(DRAFT.id)).toBeUndefined();
});

it("evicts by total bytes before reaching the chat-count limit", () => {
  const cache = new DraftCache();
  const messages = [{...MESSAGE, text: "x".repeat(400_000)}];
  for (const id of ["first", "second", "third"]) cache.remember({...view, draft: {...DRAFT, id}, messages});
  expect(cache.read("first")).toBeUndefined();
  expect(cache.read("second")?.messages).toEqual(messages);
  expect(cache.read("third")?.messages).toEqual(messages);
});
