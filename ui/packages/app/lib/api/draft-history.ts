// Advance only past terminal pages: an in-flight user's state remains mutable.

import type { Draft, Message } from "./draft-types";
import { DraftSchema, isRunning, MESSAGE_ROLE, MessagesSchema, PAGE_LIMIT } from "./draft-types";
import { setupRequest, SetupUnavailable } from "./draft-request";

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
      const [messages, draft] = await Promise.all([
        this.read(path, signal), setupRequest({path}, DraftSchema, signal),
      ]);
      signal.throwIfAborted();
      return {messages, draft};
    })();
    this.#reading = read;
    try { return await read; } finally { if (this.#reading === read) this.#reading = undefined; }
  }

  async read(path: string, signal: AbortSignal): Promise<readonly Message[]> {
    let cursor = this.#cursor;
    let canAdvance = true;
    for (let page = 0; page < MAX_WALK_PAGES; page += 1) {
      const query = new URLSearchParams({limit: String(PAGE_LIMIT)});
      if (cursor) query.set("starting_after", cursor);
      const result = await setupRequest({path: `${path}/messages?${query}`}, MessagesSchema, signal);
      for (const message of result.items) this.#remember(message);
      if (result.items.some(isRunning)) canAdvance = false;
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
