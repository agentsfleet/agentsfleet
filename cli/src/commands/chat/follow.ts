// Bounded reconciliation follows durable state through ordinary message reads.

import { Effect } from "effect";
import { ServerError, ValidationError } from "../../errors/index.ts";
import { MESSAGE_ROLE, MESSAGE_STATE, type Message } from "./protocol.ts";
import type { DraftSession } from "./session.ts";

const MAX_HISTORY_PAGES = 200;
const MAX_FOLLOW_READS = 160;
const MIN_POLL_DELAY_MS = 250;
const FOLLOW_PENDING = "CHAT_MESSAGE_PENDING";
type Wait = (sequence: string, state: string) => Effect.Effect<void, ServerError>;
type Emit = (message: Message) => Effect.Effect<void>;

export function readHistory(session: DraftSession, emit: Emit, target?: string) {
  return Effect.gen(function* () {
    let cursor: string | undefined;
    let matching: Message | undefined;
    for (let pageIndex = 0; pageIndex < MAX_HISTORY_PAGES; pageIndex++) {
      const page = yield* session.page(cursor);
      for (const message of page.items) {
        if (target === undefined || message.sequence === target) {
          yield* emit(message);
          if (message.role === MESSAGE_ROLE.user) matching = message;
        }
      }
      if (!page.next_cursor) return matching;
      cursor = page.next_cursor;
    }
    return yield* Effect.fail(new ValidationError({
      detail: "This conversation exceeds the terminal history window",
      suggestion: "open it in the workspace to inspect the paged transcript",
    }));
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
    for (let read = 0; read < MAX_FOLLOW_READS; read++) {
      const message = yield* readHistory(session, changed, sequence);
      if (message && message.state !== MESSAGE_STATE.queued && message.state !== MESSAGE_STATE.running) {
        yield* session.refresh();
        return message;
      }
      yield* wait(sequence, message?.state ?? MESSAGE_STATE.queued);
      yield* Effect.sleep(MIN_POLL_DELAY_MS);
    }
    return yield* Effect.fail(new ServerError({
      detail: `Message ${sequence} remains pending in draft ${session.id}`,
      suggestion: "resume that draft to reconcile its durable outcome; do not resend with a fresh operation key",
      code: FOLLOW_PENDING, status: 408, requestId: null,
    }));
  });
}
