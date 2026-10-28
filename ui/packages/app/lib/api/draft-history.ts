// Usage settlement can still change a terminal message's recovery action.

import type { Draft, Message } from "./draft-types";
import { DraftSchema, isRunning, MESSAGE_ROLE, MessagesSchema, PAGE_LIMIT } from "./draft-types";
import { setupRequest, SetupUnavailable } from "./draft-request";
import { isStopped, UNKNOWN_USAGE_REASON } from "./draft-recovery";

type DraftRead = {draft: Draft; messages: readonly Message[]};

const MAX_WALK_PAGES = 200;
const RETAINED_MESSAGES = 500;
const ROLE_ORDER = {[MESSAGE_ROLE.user]: 0, [MESSAGE_ROLE.assistant]: 1};

export class DraftHistory {
  readonly #rows = new Map<string, Message>();
  #cursor: string | undefined;
  #highest = "0";
  #truncated = false;
  #reading: Promise<DraftRead> | undefined;

  get highest(): string { return this.#highest; }
  get truncated(): boolean { return this.#truncated; }
  clear(): void { this.#rows.clear(); this.#cursor = undefined; this.#highest = "0"; this.#truncated = false; this.#reading = undefined; }

  seed(messages: readonly Message[], cursor: string | undefined, truncated: boolean): void {
    this.clear(); this.#cursor = cursor; this.#truncated = truncated;
    for (const message of messages) this.#remember(message);
  }

  /** Queue one fresh read after an outstanding read; concurrent waiters share that successor. */
  async synchronize(path: string, signal: AbortSignal): Promise<DraftRead> {
    const previous = this.#reading;
    if (previous) {
      await previous.catch(() => undefined);
      signal.throwIfAborted();
      if (this.#reading) return this.#reading;
    }
    const read = (async () => {
      signal.throwIfAborted();
      const current = setupRequest({path}, DraftSchema, signal);
      const [messages, draft] = await Promise.all([
        this.read(path, signal, current), current,
      ]);
      signal.throwIfAborted();
      return {messages, draft};
    })();
    this.#reading = read;
    try { return await read; } finally { if (this.#reading === read) this.#reading = undefined; }
  }

  async read(path: string, signal: AbortSignal, current?: Promise<Draft>): Promise<readonly Message[]> {
    let cursor = this.#cursor;
    let canAdvance = true;
    for (let page = 0; page < MAX_WALK_PAGES; page += 1) {
      const query = new URLSearchParams({limit: String(PAGE_LIMIT)});
      if (cursor) query.set("starting_after", cursor);
      const [result, draft] = await Promise.all([
        setupRequest({path: `${path}/messages?${query}`}, MessagesSchema, signal), current,
      ]);
      for (const message of result.items) this.#remember(message);
      const cutoff = draft?.cancelled_through ?? "0";
      if (result.items.some((message) => !isStopped(message, cutoff)
        && (isRunning(message) || message.failure?.reason === UNKNOWN_USAGE_REASON))) canAdvance = false;
      if (!result.next_cursor) return [...this.#rows.values()].sort(ordered);
      if (canAdvance) this.#cursor = result.next_cursor;
      cursor = result.next_cursor;
    }
    throw new SetupUnavailable("Conversation history is still loading. Refresh to continue from the last confirmed page.", 503);
  }

  #remember(message: Message): void {
    if (BigInt(message.sequence) > BigInt(this.#highest)) this.#highest = message.sequence;
    this.#rows.set(`${message.sequence}:${message.role}`, message);
    if (this.#rows.size > RETAINED_MESSAGES) {
      const excess = [...this.#rows.values()].sort(ordered).slice(0, this.#rows.size - RETAINED_MESSAGES);
      for (const oldest of excess) this.#rows.delete(`${oldest.sequence}:${oldest.role}`);
      this.#truncated = true;
    }
  }
}

function ordered(left: Message, right: Message): number {
  const sequence = BigInt(left.sequence) - BigInt(right.sequence);
  if (sequence !== 0n) return sequence < 0n ? -1 : 1;
  return ROLE_ORDER[left.role] - ROLE_ORDER[right.role];
}
