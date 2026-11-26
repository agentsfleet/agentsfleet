import type { z } from "zod";
import { liveRequest, losesAccess } from "./live-request";

type Page<T> = {items: T[]; next_cursor: string | null};
// Rows the sidebar asks for per read.
const LIST_PAGE_LIMIT = 50;
type Snapshot<T> = {items: readonly T[]; more: boolean; loading: boolean; error: string | null};

/** One mounted sidebar owns its requests, pagination and private rows. */
export class RecentCollection<T extends {id: string}> {
  readonly #owner = new AbortController();
  readonly #listeners = new Set<() => void>();
  readonly #removed = new Set<string>();
  #snapshot: Snapshot<T> = {items: [], more: false, loading: true, error: null};
  #cursor: string | null = null;
  #loading: Promise<void> | null = null;
  #readAt = -Infinity;
  #pageCount = 0;

  constructor(readonly path: string, readonly schema: z.ZodType<Page<T>>) {}
  readonly getSnapshot = (): Snapshot<T> => this.#snapshot;
  readonly subscribe = (listener: () => void): (() => void) => {this.#listeners.add(listener); return () => this.#listeners.delete(listener);};
  dispose(): void {this.#owner.abort(); this.#listeners.clear(); this.#removed.clear(); this.#snapshot = {items: [], more: false, loading: false, error: null};}
  remove(id: string): void {this.#removed.add(id); this.#update({items: this.#snapshot.items.filter(item => item.id !== id)});}
  refresh(maxAgeMs: number): Promise<void> {
    if (!this.#snapshot.error && performance.now() - this.#readAt < maxAgeMs) return Promise.resolve();
    return this.load();
  }

  load(append = false): Promise<void> {
    if (this.#loading) return this.#loading;
    const cursor = append ? this.#cursor : null;
    if (append && !cursor) return Promise.resolve();
    const pending = this.#read(cursor, append);
    this.#loading = pending;
    const release = (): void => {this.#loading = null;};
    pending.then(release, release);
    return pending;
  }

  async #read(after: string | null, append: boolean): Promise<void> {
    this.#update({loading: true, error: null});
    try {
      const rows = new Map((append ? this.#snapshot.items : []).map(item => [item.id, item]));
      // A refresh replaces the loaded pages together, using the new cursor
      // chain. Never fetch a page the person hasn't already opened.
      const limit = append ? 1 : Math.max(1, this.#pageCount);
      let cursor = after;
      let count = 0;
      do {
        const query = new URLSearchParams({limit: String(LIST_PAGE_LIMIT)});
        if (cursor) query.set("starting_after", cursor);
        const page = await liveRequest({path: `${this.path}?${query}`}, this.schema, this.#owner.signal);
        this.#owner.signal.throwIfAborted();
        for (const item of page.items) rows.set(item.id, item);
        cursor = page.next_cursor === cursor ? null : page.next_cursor;
        count += 1;
      } while (cursor && count < limit);
      this.#cursor = cursor;
      this.#pageCount = append ? this.#pageCount + count : count;
      if (!append) this.#readAt = performance.now();
      this.#update({items: [...rows.values()].filter(item => !this.#removed.has(item.id)), more: cursor !== null});
    } catch (cause) {
      if (this.#owner.signal.aborted) return;
      if (losesAccess(cause)) {this.#cursor = null; this.#pageCount = 0; this.#update({items: [], more: false});}
      this.#update({error: "Fleets could not be loaded."});
    } finally {this.#update({loading: false});}
  }

  #update(patch: Partial<Snapshot<T>>): void {
    if (this.#owner.signal.aborted) return;
    this.#snapshot = {...this.#snapshot, ...patch};
    for (const listener of this.#listeners) listener();
  }
}
