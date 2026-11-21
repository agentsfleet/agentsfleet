// Reconciliation follows durable state through ordinary message reads. A
// history walk ends when the server reports no next page; a follow ends when
// the turn settles or the caller interrupts it. Neither counts pages or reads.

import { Effect } from "effect";
import { ServerError } from "../../errors/index.ts";
import { MESSAGE_ROLE, MESSAGE_STATE, type Message } from "./protocol.ts";
import type { DraftSession } from "./session.ts";

const MIN_POLL_DELAY_MS = 250;
type Wait = (sequence: string, state: string) => Effect.Effect<void, ServerError>;
type Emit = (message: Message) => Effect.Effect<void>;

export function readHistory(session: DraftSession, emit: Emit, target?: string) {
  return Effect.gen(function* () {
    const cursors = new Set<string>();
    let cursor: string | undefined;
    let matching: Message | undefined;
    for (;;) {
      const page = yield* session.page(cursor);
      for (const message of page.items) {
        if (target === undefined || message.sequence === target) {
          yield* emit(message);
          if (message.role === MESSAGE_ROLE.user) matching = message;
        }
      }
      if (!page.next_cursor) return matching;
      // A cursor the walk already followed would replay the same pages forever.
      if (cursors.has(page.next_cursor)) return yield* Effect.fail(new ServerError({
        detail: `The history of draft ${session.id} repeated page cursor ${page.next_cursor}`,
        suggestion: "retry; if it persists, report the server's repeating next_cursor",
        code: "CHAT_HISTORY_CURSOR_REPEATED", status: 502, requestId: null,
      }));
      cursors.add(page.next_cursor);
      cursor = page.next_cursor;
    }
  });
}

export function follow(session: DraftSession, sequence: string, emit: Emit, wait: Wait) {
  return Effect.gen(function* () {
    const seen = new Map<string, string>();
    const changed: Emit = (message) => {
      const identity = `${message.sequence}:${message.role}`;
      const fingerprint = JSON.stringify(message);
      if (seen.get(identity) === fingerprint) return Effect.void;
      seen.set(identity, fingerprint);
      return emit(message);
    };
    for (;;) {
      const message = yield* readHistory(session, changed, sequence);
      if (message && message.state !== MESSAGE_STATE.queued && message.state !== MESSAGE_STATE.running) {
        yield* session.refresh();
        return message;
      }
      yield* wait(sequence, message?.state ?? MESSAGE_STATE.queued);
      yield* Effect.sleep(MIN_POLL_DELAY_MS);
    }
  });
}
