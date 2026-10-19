// One creator-only query owns its continuation and cancellation.
import type { DraftCard } from "./draft-types";
import { DraftCardsSchema } from "./draft-types";
import { LIST_PAGE_LIMIT } from "./draft-list-types";
import { DRAFT_SEARCH_QUERY_MAX, DRAFT_SEARCH_STATUS as STATUS } from "./draft-vocabulary";
import { losesAccess, setupErrorMessage, setupRequest } from "./draft-request";

type Results = {readonly query: string; readonly items: readonly DraftCard[]; readonly next_cursor: string | null};
export type DraftSearchView = Results & ({readonly status: typeof STATUS.loading | typeof STATUS.ready}
  | {readonly status: typeof STATUS.failed | typeof STATUS.inaccessible; readonly message: string});
const SEARCH_FAILED = "We couldn’t search your chats. Try again.";
const QUERY_INVALID = `Use 1–${DRAFT_SEARCH_QUERY_MAX} characters to search your chats.`;

export class DraftSearch {
  readonly #base: string;
  readonly #listeners = new Set<() => void>();
  readonly #removed = new Set<string>();
  #owner = new AbortController();
  #pending: Promise<void> | null = null;
  #view: DraftSearchView = {query: "", items: [], next_cursor: null, status: STATUS.ready};

  constructor(workspace: string) { this.#base = `/live/v1/workspaces/${encodeURIComponent(workspace)}/drafts`; }
  getSnapshot = (): DraftSearchView => this.#view;
  subscribe = (listener: () => void): (() => void) => {this.#listeners.add(listener); return () => {this.#listeners.delete(listener);};};

  search(query: string): Promise<void> {
    this.cancel(); this.#owner = new AbortController();
    const trimmed = query.trim();
    const empty = {query: trimmed, items: [], next_cursor: null};
    if (!trimmed || trimmed.includes("\0") || Array.from(trimmed).length > DRAFT_SEARCH_QUERY_MAX) {
      this.#publish({...empty, status: STATUS.failed, message: QUERY_INVALID});
      return Promise.resolve();
    }
    const retain = trimmed === this.#view.query && (this.#view.status === STATUS.ready || this.#view.items.length > 0);
    this.#publish(retain ? {query: trimmed, items: this.#view.items, next_cursor: this.#view.next_cursor, status: STATUS.ready}
      : {...empty, status: STATUS.loading});
    return this.#load(null, true);
  }

  more = (): Promise<void> => {
    if (this.#pending) return this.#pending;
    const cursor = this.#view.next_cursor;
    if (cursor === null || this.#view.status !== STATUS.ready || this.#owner.signal.aborted) return Promise.resolve();
    return this.#load(cursor, false);
  };

  #load(cursor: string | null, first: boolean): Promise<void> {
    const pending = this.#read(cursor, this.#owner, first);
    this.#pending = pending;
    const release = (): void => {if (this.#pending === pending) this.#pending = null;};
    pending.then(release, release);
    return pending;
  }

  remove = (id: string): void => {this.#removed.add(id); this.#publish({...this.#view, items: this.#view.items.filter(card => card.id !== id)});};
  cancel(): void { this.#owner.abort(); this.#pending = null; }

  async #read(cursor: string | null, owner: AbortController, first: boolean): Promise<void> {
    const query = new URLSearchParams({q: this.#view.query, limit: String(LIST_PAGE_LIMIT)});
    if (cursor !== null) query.set("starting_after", cursor);
    try {
      const page = await setupRequest({path: `${this.#base}?${query}`}, DraftCardsSchema, owner.signal);
      owner.signal.throwIfAborted();
      const seen = new Set(first ? [] : this.#view.items.map(card => card.id));
      const items = [...(first ? [] : this.#view.items), ...page.items.filter(card => {
        if (seen.has(card.id)) return false;
        seen.add(card.id); return true;
      })].filter(card => !this.#removed.has(card.id));
      this.#publish({query: this.#view.query, status: STATUS.ready, items, next_cursor: page.next_cursor});
    } catch (cause) {
      if (owner.signal.aborted) return;
      if (losesAccess(cause)) this.#publish({query: this.#view.query, status: STATUS.inaccessible, items: [], next_cursor: null, message: setupErrorMessage(cause, true)});
      else if (first) this.#publish({...this.#view, status: STATUS.failed, message: SEARCH_FAILED});
      else throw cause;
    }
  }

  #publish(view: DraftSearchView): void {this.#view = view; for (const listener of this.#listeners) listener();}
}
