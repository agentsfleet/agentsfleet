import type { z } from "zod";
import { losesAccess, setupRequest } from "./draft-request";
import { LIST_PAGE_LIMIT } from "./draft-list-types";

type Page<T> = {items: T[]; next_cursor: string | null};
type Snapshot<T> = {items: readonly T[]; more: boolean; loading: boolean; error: string | null};

/** One mounted sidebar owns its requests, pagination and private rows. */
export class RecentCollection<T extends {id: string}> {
  readonly #owner = new AbortController();
  readonly #listeners = new Set<() => void>();
  readonly #removed = new Set<string>();
  #snapshot: Snapshot<T> = {items: [], more: false, loading: true, error: null};
  #cursor: string | null = null;
  #loading: Promise<void> | null = null;

  constructor(readonly path: string, readonly schema: z.ZodType<Page<T>>) {}
  readonly getSnapshot = (): Snapshot<T> => this.#snapshot;
  readonly subscribe = (listener: () => void): (() => void) => {this.#listeners.add(listener); return () => this.#listeners.delete(listener);};
  dispose(): void {this.#owner.abort(); this.#listeners.clear(); this.#removed.clear(); this.#snapshot = {items: [], more: false, loading: false, error: null};}
  remove(id: string): void {this.#removed.add(id); this.#update({items: this.#snapshot.items.filter(item => item.id !== id)});}
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

  async #read(cursor: string | null, append: boolean): Promise<void> {
    this.#update({loading: true, error: null});
    const query = new URLSearchParams({limit: String(LIST_PAGE_LIMIT)});
    if (cursor) query.set("starting_after", cursor);
    try {
      const page = await setupRequest({path: `${this.path}?${query}`}, this.schema, this.#owner.signal);
      this.#owner.signal.throwIfAborted();
      const rows = new Map((append ? this.#snapshot.items : []).map(item => [item.id, item]));
      for (const item of page.items) if (!this.#removed.has(item.id)) rows.set(item.id, item);
      this.#cursor = page.next_cursor === cursor ? null : page.next_cursor;
      this.#update({items: [...rows.values()], more: this.#cursor !== null});
    } catch (cause) {
      if (this.#owner.signal.aborted) return;
      if (losesAccess(cause)) {this.#cursor = null; this.#update({items: [], more: false});}
      this.#update({error: "Fleets could not be loaded."});
    } finally {this.#update({loading: false});}
  }

  #update(patch: Partial<Snapshot<T>>): void {
    if (this.#owner.signal.aborted) return;
    this.#snapshot = {...this.#snapshot, ...patch};
    for (const listener of this.#listeners) listener();
  }
}
