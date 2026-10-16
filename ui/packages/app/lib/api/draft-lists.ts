// Model choices and private chats hold one bounded page; the next loads only when asked.

import type { z } from "zod";
import type { DraftCard, WorkspaceModel } from "./draft-types";
import { DraftCardsSchema, ModelsSchema } from "./draft-types";
import { setupRequest } from "./draft-request";
import { DRAFT_LIST, LIST_PAGE_LIMIT, type DraftList, type DraftListsView } from "./draft-list-types";
type Page<T> = {readonly items: readonly T[]; readonly next_cursor: string | null};

// Stop a bounded search once chat has a choice; retain other configured models for setup review.
const MAX_MODEL_PAGES_PER_LOAD = 4;
const RESOURCE = {models: "models", drafts: "drafts"} as const;

export class DraftLists {
  readonly #base: string;
  readonly #loading = new Map<DraftList, Promise<DraftListsView | null>>();
  #models: readonly WorkspaceModel[] = [];
  #cards: readonly DraftCard[] = [];
  #modelCursor: string | null = null;
  #cardCursor: string | null = null;
  #generation = 0;

  constructor(base: string) { this.#base = base; }

  get view(): DraftListsView {
    return {models: this.#models, cards: this.#cards, moreModels: this.#modelCursor !== null, moreChats: this.#cardCursor !== null};
  }

  clear(): void {
    this.#generation += 1; this.#loading.clear();
    this.#models = []; this.#cards = []; this.#modelCursor = null; this.#cardCursor = null;
  }

  async open(signal: AbortSignal): Promise<DraftListsView> {
    this.clear();
    const generation = this.#generation;
    const [models, cards] = await Promise.all([this.#modelPage(null, signal), this.#cardPage(null, signal)]);
    if (generation === this.#generation) {
      this.#models = models.items; this.#modelCursor = models.next_cursor;
      this.#cards = cards.items; this.#cardCursor = cards.next_cursor;
    }
    return this.view;
  }

  /** Resolves null when the list is complete or a reopen replaced the page this load continued. */
  more(list: DraftList, signal: AbortSignal): Promise<DraftListsView | null> {
    const pending = this.#loading.get(list);
    if (pending) return pending;
    const cursor = list === DRAFT_LIST.models ? this.#modelCursor : this.#cardCursor;
    if (cursor === null) return Promise.resolve(null);
    const load = this.#append(list, cursor, this.#generation, signal);
    this.#loading.set(list, load);
    const release = (): void => { if (this.#loading.get(list) === load) this.#loading.delete(list); };
    load.then(release, release);
    return load;
  }

  async #append(list: DraftList, cursor: string, generation: number, signal: AbortSignal): Promise<DraftListsView | null> {
    try {
      if (list === DRAFT_LIST.models) {
        const page = await this.#modelPage(cursor, signal);
        if (generation !== this.#generation) return null;
        this.#models = appendNew(this.#models, page.items, (model) => model.ref); this.#modelCursor = page.next_cursor;
      } else {
        const page = await this.#cardPage(cursor, signal);
        if (generation !== this.#generation) return null;
        this.#cards = appendNew(this.#cards, page.items, (card) => card.id); this.#cardCursor = page.next_cursor;
      }
      return this.view;
    } catch (cause) {
      if (generation !== this.#generation) return null;
      throw cause;
    }
  }

  async #modelPage(cursor: string | null, signal: AbortSignal): Promise<Page<WorkspaceModel>> {
    let page = await this.#page(RESOURCE.models, ModelsSchema, cursor, signal);
    let items = page.items;
    for (let read = 1; !items.some(model => model.eligibility.authoring) && page.next_cursor !== null && read < MAX_MODEL_PAGES_PER_LOAD; read += 1) {
      page = await this.#page(RESOURCE.models, ModelsSchema, page.next_cursor, signal);
      items = appendNew(items, page.items, model => model.ref);
    }
    return {items, next_cursor:page.next_cursor};
  }

  #cardPage(cursor: string | null, signal: AbortSignal): Promise<Page<DraftCard>> {
    return this.#page(RESOURCE.drafts, DraftCardsSchema, cursor, signal);
  }

  #page<T>(resource: string, schema: z.ZodType<Page<T>>, cursor: string | null, signal: AbortSignal): Promise<Page<T>> {
    const query = new URLSearchParams({limit: String(LIST_PAGE_LIMIT)});
    if (cursor !== null) query.set("starting_after", cursor);
    return setupRequest({path: `${this.#base}/${resource}?${query}`}, schema, signal);
  }
}

function appendNew<T>(rows: readonly T[], more: readonly T[], key: (row: T) => string): readonly T[] {
  const seen = new Set(rows.map(key));
  return [...rows, ...more.filter((row) => !seen.has(key(row)))];
}
