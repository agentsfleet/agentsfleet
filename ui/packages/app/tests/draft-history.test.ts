// @vitest-environment node
import { afterEach, expect, it, vi } from "vitest";
import { DraftHistory } from "@/lib/api/draft-history";
import { UNKNOWN_USAGE_REASON } from "@/lib/api/draft-recovery";
import { MESSAGE_ROLE, MESSAGE_STATE } from "@/lib/api/draft-types";
import { DRAFT, MESSAGE, response } from "./helpers/draft-fixtures";

const OWNER = new AbortController();
const SETTLED_REASON = "provider_failed";
afterEach(() => vi.unstubAllGlobals());

it("revisits an in-flight page and orders a late assistant beside its own user", async () => {
  let completed = false;
  const cursors: (string | null)[] = [];
  vi.stubGlobal("fetch", vi.fn(async (url: string) => {
    const cursor = new URL(url, "https://app.example.test").searchParams.get("starting_after"); cursors.push(cursor);
    if (cursor) return response({items: [{...MESSAGE, sequence: "2", text: "next"}], next_cursor: null});
    return response({items: completed ? [MESSAGE, {...MESSAGE, role: MESSAGE_ROLE.assistant, text: "late answer"}]
      : [{...MESSAGE, state: MESSAGE_STATE.running}], next_cursor: "second"});
  }));
  const history = new DraftHistory();
  await history.read("/draft", OWNER.signal);
  completed = true;
  const rows = await history.read("/draft", OWNER.signal);
  expect(cursors).toEqual([null, "second", null, "second"]);
  expect(rows.map((row) => `${row.sequence}:${row.role}`)).toEqual(["1:user", "1:assistant", "2:user"]);
  expect(history.highest).toBe("2");
  await history.read("/draft", OWNER.signal);
  expect(cursors.at(-1)).toBe("second");
});

it("bounds retained private text and keeps a precise highest sequence", async () => {
  vi.stubGlobal("fetch", vi.fn(async (url: string) => {
    const page = Number(new URL(url, "https://app.example.test").searchParams.get("starting_after") ?? 0);
    const items = Array.from({length: 25}, (_, index) => ({...MESSAGE,
      sequence: String(9007199254740993n + BigInt(page * 25 + index))}));
    return response({items, next_cursor: page < 20 ? String(page + 1) : null});
  }));
  const history = new DraftHistory();
  const rows = await history.read("/draft", OWNER.signal);
  expect(rows).toHaveLength(500);
  expect(rows[0]?.sequence).toBe("9007199254741018");
  expect(history.highest).toBe("9007199254741517");
  expect(history.truncated).toBe(true);
  history.clear(); expect(history.highest).toBe("0"); expect(history.truncated).toBe(false);
});

it("bounds a repeating server cursor and never reports an incomplete walk as complete", async () => {
  const fetcher = vi.fn(async () => response({items: [MESSAGE], next_cursor: "same"}));
  vi.stubGlobal("fetch", fetcher);
  await expect(new DraftHistory().read("/draft", OWNER.signal)).rejects.toMatchObject({status: 503});
  expect(fetcher).toHaveBeenCalledTimes(200);
});

it.each([401, 403, 404, 410])("preserves history access refusal %i", async (status) => {
  vi.stubGlobal("fetch", vi.fn(async () => response({}, status)));
  await expect(new DraftHistory().read("/draft", OWNER.signal)).rejects.toMatchObject({status});
});

it("cancels a pending read before it can restore cleared private history", async () => {
  const pending = Promise.withResolvers<Response>();
  vi.stubGlobal("fetch", vi.fn(() => pending.promise));
  const history = new DraftHistory(); const owner = new AbortController();
  const read = history.read("/draft", owner.signal);
  owner.abort(); history.clear(); pending.resolve(response({items: [MESSAGE], next_cursor: null}));
  await expect(read).rejects.toThrow(); expect(history.highest).toBe("0");
});


it("orders a reversed pair by speaker and replaces the same durable message", async () => {
  const assistant = {...MESSAGE, role: MESSAGE_ROLE.assistant, text: "answer"};
  vi.stubGlobal("fetch", vi.fn(async () => response({items: [assistant, MESSAGE, {...MESSAGE, text: "latest"}], next_cursor: null})));
  const rows = await new DraftHistory().read("/draft", OWNER.signal);
  expect(rows.map(({role, text}) => ({role, text}))).toEqual([
    {role: MESSAGE_ROLE.user, text: "latest"}, {role: MESSAGE_ROLE.assistant, text: "answer"},
  ]);
});


it.each([false, true])("coalesces concurrent refreshes after an outstanding read (refused: %s)", async (refused) => {
  const first = Promise.withResolvers<Response>();
  let messageReads = 0;
  const fetcher = vi.fn(async (input: string) => {
    if (input.includes("/messages?")) {
      messageReads += 1;
      return messageReads === 1 ? first.promise : response({items: [{...MESSAGE, text: "latest"}], next_cursor: null});
    }
    return response(DRAFT);
  });
  vi.stubGlobal("fetch", fetcher);
  const history = new DraftHistory();
  const original = history.synchronize("/draft", OWNER.signal).catch((cause: unknown) => cause);
  const second = history.synchronize("/draft", OWNER.signal);
  const third = history.synchronize("/draft", OWNER.signal);
  first.resolve(response({items: [MESSAGE], next_cursor: null}, refused ? 503 : 200));
  await original;
  const results = await Promise.all([second, third]);
  expect(results[0]).toEqual(results[1]);
  expect(results[0]?.messages[0]?.text).toBe("latest");
  expect(messageReads).toBe(2);
});

it("does not let a cancelled generation release or restore its replacement read", async () => {
  const old = Promise.withResolvers<Response>();
  const latest = Promise.withResolvers<Response>();
  vi.stubGlobal("fetch", vi.fn((input: string) => input.startsWith("/old") ? old.promise : latest.promise.then((value) => value.clone())));
  const history = new DraftHistory(); const owner = new AbortController();
  const oldRead = history.synchronize("/old", owner.signal).catch((cause: unknown) => cause);
  const waiting = history.synchronize("/old", owner.signal).catch((cause: unknown) => cause);
  owner.abort(); history.clear();
  const next = history.synchronize("/new", OWNER.signal);
  old.reject(new Error("cancelled transport"));
  await Promise.all([oldRead, waiting]);
  latest.resolve(response({...DRAFT, items: [{...MESSAGE, text: "new owner"}], next_cursor: null}));
  expect((await next).messages.map(({text}) => text)).toEqual(["new owner"]);
});

it.each([false, true])("should release a cached unknown-usage page after reconciliation (stopped=%s)", async (stopped) => {
  let reconciled = false;
  const cursors: (string | null)[] = [];
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    if (!input.includes("/messages?")) return response({...DRAFT, cancelled_through: stopped && reconciled ? "1" : "0"});
    const cursor = new URL(input, "https://app.example.test").searchParams.get("starting_after");
    cursors.push(cursor);
    const message = {...MESSAGE, state: MESSAGE_STATE.failed,
      failure: {reason: reconciled && !stopped ? SETTLED_REASON : UNKNOWN_USAGE_REASON,
        error_code: "UZ-AUTHOR-015", recovery_action: "await_accounting"}};
    return response({items: [{...message, role: cursor ? MESSAGE_ROLE.assistant : MESSAGE_ROLE.user}],
      next_cursor: cursor ? null : "second"});
  }));
  const history = new DraftHistory();
  await history.synchronize("/draft", OWNER.signal);
  reconciled = true;
  const result = await history.synchronize("/draft", OWNER.signal);
  expect(cursors).toEqual([null, "second", null, "second"]);
  expect(result.messages[0]?.failure?.reason).toBe(stopped ? UNKNOWN_USAGE_REASON : SETTLED_REASON);
  expect(result.draft.cancelled_through).toBe(stopped ? "1" : "0");
  await history.synchronize("/draft", OWNER.signal);
  expect(cursors.at(-1)).toBe("second");
  expect(cursors).toHaveLength(5);
});
