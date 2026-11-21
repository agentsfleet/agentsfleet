import { expect, test } from "bun:test";
import { Cause, Clock, Duration, Effect, Exit, Fiber } from "effect";
import { TestClock } from "effect/testing";
import { follow, readHistory } from "../../src/commands/chat/follow.ts";
import { MESSAGE_ROLE, MESSAGE_STATE, type Message } from "../../src/commands/chat/protocol.ts";
import { DraftSession, DRAFT_SOURCE } from "../../src/commands/chat/session.ts";
import { CLI_ERROR_TAG } from "../../src/errors/index.ts";
import { DRAFT, DRAFT_ID, MESSAGE, MODEL, withApi } from "./fixture.ts";

// The history-page and follow-read bounds the terminal once carried; a longer
// conversation is read whole, and a follow lasts until the turn settles.
const RETIRED_HISTORY_PAGES = 200;
const RETIRED_FOLLOW_READS = 160;
// pin test: literal is the floor — two reads of one turn are never closer than this.
const POLL_DELAY_MS = 250;

type Page = { readonly items: readonly Message[]; readonly next_cursor: string | null };
type Script = (read: number) => Page;
type Seen = { readonly cursors: (string | null)[]; drafts: number; mutations: number };

function turn(sequence: string, state: Message["state"], role: Message["role"] = MESSAGE_ROLE.user): Message {
  return {...MESSAGE, sequence, state, role};
}

async function withHistory<T>(script: Script, run: (session: DraftSession, seen: Seen) => Promise<T>): Promise<T> {
  const seen: Seen = {cursors: [], drafts: 0, mutations: 0};
  return withApi((request) => {
    const url = new URL(request.url);
    if (request.method !== "GET") { seen.mutations++; return Response.json({}, {status: 500}); }
    if (!url.pathname.endsWith("/messages")) { seen.drafts++; return Response.json(DRAFT); }
    seen.cursors.push(url.searchParams.get("starting_after"));
    return Response.json(script(seen.cursors.length));
  }, (transport) => run(new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}, DRAFT_ID), seen));
}

// Drives the poll delay on a virtual clock while the reads stay real HTTP.
function onVirtualClock<A, E>(effect: Effect.Effect<A, E>): Promise<Exit.Exit<A, E>> {
  return Effect.runPromise(Effect.gen(function* () {
    const fiber = yield* Effect.forkChild(effect);
    while (fiber.pollUnsafe() === undefined) {
      yield* TestClock.adjust(Duration.millis(POLL_DELAY_MS));
      yield* Effect.promise(() => Bun.sleep(1));
    }
    return yield* Fiber.await(fiber);
  }).pipe(Effect.provide(TestClock.layer())));
}

test("should walk every history page by its cursor and return the latest user turn", async () => {
  const emitted: string[] = [];
  await withHistory((read) => read === 1
    ? {items: [turn("1", MESSAGE_STATE.completed), turn("1", MESSAGE_STATE.completed, MESSAGE_ROLE.assistant)], next_cursor: "page-2"}
    : {items: [turn("2", MESSAGE_STATE.completed)], next_cursor: null},
  async (session, seen) => {
    const latest = await Effect.runPromise(readHistory(session, (message) => Effect.sync(() => {
      emitted.push(`${message.sequence}:${message.role}`);
    })));
    expect(latest?.sequence).toBe("2");
    expect(seen.cursors).toEqual([null, "page-2"]);
  });
  expect(emitted).toEqual(["1:user", "1:assistant", "2:user"]);
});

test("should read a conversation past the retired history window until the server reports the end", async () => {
  const pages = RETIRED_HISTORY_PAGES + 5;
  await withHistory((read) => ({items: [turn(String(read), MESSAGE_STATE.completed)], next_cursor: read < pages ? `page-${read + 1}` : null}),
  async (session, seen) => {
    const latest = await Effect.runPromise(readHistory(session, () => Effect.void));
    expect(latest?.sequence).toBe(String(pages));
    expect(seen.cursors).toHaveLength(pages);
    expect(seen.drafts).toBe(0);
  });
});

test("should refuse a page cursor the walk already followed instead of reading forever", async () => {
  await withHistory(() => ({items: [turn("1", MESSAGE_STATE.completed)], next_cursor: "same"}),
  async (session, seen) => {
    const error = await Effect.runPromise(Effect.flip(readHistory(session, () => Effect.void)));
    expect(error._tag).toBe(CLI_ERROR_TAG.server);
    expect(error.detail).toBe(`The history of draft ${DRAFT_ID} repeated page cursor same`);
    expect(error._tag === CLI_ERROR_TAG.server && error.code).toBe("CHAT_HISTORY_CURSOR_REPEATED");
    expect(seen.cursors).toEqual([null, "same"]);
  });
});

test("should wait between reads until the turn settles, render each change once and refresh the draft", async () => {
  const emitted: string[] = [];
  const waits: string[] = [];
  const waitedAt: number[] = [];
  await withHistory((read) => {
    if (read === 1) return {items: [], next_cursor: null};
    if (read < 4) return {items: [turn("1", MESSAGE_STATE.queued)], next_cursor: null};
    return {items: [turn("1", MESSAGE_STATE.completed),
      {...turn("1", MESSAGE_STATE.completed, MESSAGE_ROLE.assistant), text: "the answer"}], next_cursor: null};
  }, async (session, seen) => {
    const exit = await onVirtualClock(follow(session, "1",
      (message) => Effect.sync(() => { emitted.push(`${message.role}:${message.state}`); }),
      (sequence, state) => Clock.currentTimeMillis.pipe(Effect.map((now) => {
        waits.push(`${sequence}:${state}`);
        waitedAt.push(now);
      }))));
    expect(Exit.isSuccess(exit) && exit.value.state).toBe(MESSAGE_STATE.completed);
    expect(seen.cursors).toHaveLength(4);
    expect(seen.drafts).toBe(1);
    expect(seen.mutations).toBe(0);
  });
  // An invisible turn is waited on as queued; an unchanged repeat renders nothing.
  expect(waits).toEqual(["1:queued", "1:queued", "1:queued"]);
  expect(emitted).toEqual(["user:queued", "user:completed", "assistant:completed"]);
  for (let index = 1; index < waitedAt.length; index++) {
    expect((waitedAt[index] ?? 0) - (waitedAt[index - 1] ?? 0)).toBeGreaterThanOrEqual(POLL_DELAY_MS);
  }
});

test("should keep following a running turn past the retired read budget until it settles, never resending it", async () => {
  const emitted: string[] = [];
  let waits = 0;
  const settlesAt = RETIRED_FOLLOW_READS + 5;
  await withHistory((read) => ({items: [turn("1", read < settlesAt ? MESSAGE_STATE.running : MESSAGE_STATE.completed)], next_cursor: null}),
  async (session, seen) => {
    const exit = await onVirtualClock(follow(session, "1",
      (message) => Effect.sync(() => { emitted.push(message.state); }),
      (_sequence, state) => Effect.sync(() => { expect(state).toBe(MESSAGE_STATE.running); waits++; })));
    expect(Exit.isSuccess(exit) && exit.value.state).toBe(MESSAGE_STATE.completed);
    expect(seen.cursors).toHaveLength(settlesAt);
    expect(seen.mutations).toBe(0);
    expect(seen.drafts).toBe(1);
  });
  expect(waits).toBe(settlesAt - 1);
  expect(emitted).toEqual([MESSAGE_STATE.running, MESSAGE_STATE.completed]);
});

test("should stop following a turn that never settles when the caller interrupts", async () => {
  await withHistory(() => ({items: [turn("1", MESSAGE_STATE.running)], next_cursor: null}), async (session, seen) => {
    const exit = await Effect.runPromise(Effect.gen(function* () {
      const fiber = yield* Effect.forkChild(follow(session, "1", () => Effect.void, () => Effect.void));
      while (seen.cursors.length < 3) {
        yield* TestClock.adjust(Duration.millis(POLL_DELAY_MS));
        yield* Effect.promise(() => Bun.sleep(1));
      }
      yield* Fiber.interrupt(fiber);
      return yield* Fiber.await(fiber);
    }).pipe(Effect.provide(TestClock.layer())));
    expect(Exit.isFailure(exit) && Cause.hasInterruptsOnly(exit.cause)).toBe(true);
    expect(seen.mutations).toBe(0);
  });
});
