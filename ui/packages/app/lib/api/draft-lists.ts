// Model choices and private chats hold one page; the next loads only when asked.

import type { z } from "zod";
import type { Draft, DraftCard, WorkspaceModel } from "./draft-types";
import type { DraftView } from "./draft-state";
import { DraftCardsSchema, ModelsSchema } from "./draft-types";
import { losesAccess, setupRequest, SetupUnavailable } from "./draft-request";
import { DRAFT_LIST, DRAFT_LIST_STATUS, LIST_PAGE_LIMIT, MODEL_LIST_RETRYING, defaultChatModel, type DraftList, type DraftListsView } from "./draft-list-types";
type Page<T> = {readonly items: readonly T[]; readonly next_cursor: string | null};
type ListUpdates = {
  ready: (list: DraftList, view: DraftListsView) => void;
  failed: (list: DraftList, cause: unknown) => void;
};

const RESOURCE = {models: "models", drafts: "drafts"} as const;

export class DraftLists {
  readonly #base: string;
  readonly #removed = new Set<string>();
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
    this.#generation += 1; this.#loading.clear(); this.#removed.clear();
    this.#models = []; this.#cards = []; this.#modelCursor = null; this.#cardCursor = null;
  }

  remove(id: string): DraftListsView {
    this.#removed.add(id);
    this.#cards = this.#cards.filter((card) => card.id !== id);
    return this.view;
  }

  updateDraft(draft: Draft): DraftListsView {
    this.#cards = this.#cards.map((card) => card.id === draft.id
      ? {...card, title: draft.title, state: draft.state, etag: draft.etag, linked_fleet_id: draft.linked_fleet_id ?? null,
        candidate_revision: summaryRevision(draft.candidate?.revision),
        setup_revision: card.linked_fleet_id === draft.linked_fleet_id ? card.setup_revision : null} : card);
    return this.view;
  }

  updateSetup(id: string, fleet: {id: string; setup_revision: string}): DraftListsView {
    this.#cards = this.#cards.map(card => card.id === id
      ? {...card, linked_fleet_id: fleet.id, setup_revision: summaryRevision(fleet.setup_revision)} : card);
    return this.view;
  }

  /** A completed creation refreshes one summary without moving existing rows or its cursor. */
  async refreshDraft(id: string, signal: AbortSignal): Promise<DraftListsView> {
    const generation = this.#generation;
    const page = await this.#cardPage(null, signal);
    if (generation !== this.#generation || this.#removed.has(id)) return this.view;
    const card = page.items.find(item => item.id === id);
    if (card) this.#cards = this.#cards.some(item => item.id === id)
      ? this.#cards.map(item => item.id === id ? card : item) : [card, ...this.#cards];
    return this.view;
  }

  async refreshSummary(id: string, signal: AbortSignal,
    update: (patch: Partial<DraftListsView> & {listStatus?: typeof DRAFT_LIST_STATUS.unavailable}) => void,
    revoked: (cause: unknown) => void): Promise<void> {
    try {
      const lists = await this.refreshDraft(id, signal);
      if (!signal.aborted) update(lists);
    } catch (cause) {
      if (signal.aborted) return;
      if (losesAccess(cause)) revoked(cause); else update({listStatus: DRAFT_LIST_STATUS.unavailable});
    }
  }

  async open(signal: AbortSignal, updates?: ListUpdates): Promise<DraftListsView> {
    this.clear();
    await Promise.all([this.first(DRAFT_LIST.models, signal, updates), this.first(DRAFT_LIST.chats, signal, updates)]);
    return this.view;
  }

  first(list: DraftList, signal: AbortSignal, updates?: ListUpdates): Promise<DraftListsView | null> {
    return this.#append(list, null, this.#generation, signal, updates);
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

  async #append(list: DraftList, cursor: string | null, generation: number, signal: AbortSignal, updates?: ListUpdates): Promise<DraftListsView | null> {
    try {
      if (list === DRAFT_LIST.models) {
        const page = await this.#modelPage(cursor, signal);
        if (generation !== this.#generation) return null;
        this.#models = cursor === null ? page.items : appendNew(this.#models, page.items, (model) => model.ref);
        this.#modelCursor = page.next_cursor;
      } else {
        const page = await this.#cardPage(cursor, signal);
        if (generation !== this.#generation) return null;
        this.#cards = appendNew(this.#cards, page.items.filter((card) => !this.#removed.has(card.id)), (card) => card.id); this.#cardCursor = page.next_cursor;
      }
    } catch (cause) {
      if (generation !== this.#generation) return null;
      updates?.failed(list, cause);
      throw cause;
    }
    const view = this.view;
    updates?.ready(list, view);
    return view;
  }

  async #modelPage(cursor: string | null, signal: AbortSignal): Promise<Page<WorkspaceModel>> {
    let page = await this.#page(DRAFT_LIST.models, ModelsSchema, cursor, signal);
    let items = page.items;
    // Read on until chat has a choice or the server reports no next page; the
    // other configured models stay for setup review.
    const followed = new Set<string>();
    while (!items.some(model => model.eligibility.setup) && page.next_cursor !== null) {
      if (followed.has(page.next_cursor)) throw new SetupUnavailable("The model list repeated a page. Refresh to load it again.", 502, {list: DRAFT_LIST.models});
      followed.add(page.next_cursor);
      page = await this.#page(DRAFT_LIST.models, ModelsSchema, page.next_cursor, signal);
      items = appendNew(items, page.items, model => model.ref);
    }
    return {items, next_cursor:page.next_cursor};
  }

  #cardPage(cursor: string | null, signal: AbortSignal): Promise<Page<DraftCard>> {
    return this.#page(DRAFT_LIST.chats, DraftCardsSchema, cursor, signal);
  }

  #page<T>(list: DraftList, schema: z.ZodType<Page<T>>, cursor: string | null, signal: AbortSignal): Promise<Page<T>> {
    const resource = list === DRAFT_LIST.models ? RESOURCE.models : RESOURCE.drafts;
    const query = new URLSearchParams({limit: String(LIST_PAGE_LIMIT)});
    if (cursor !== null) query.set("starting_after", cursor);
    return setupRequest({path: `${this.#base}/${resource}?${query}`, list}, schema, signal);
  }
}

function summaryRevision(value: string | undefined): number | null {
  const revision = Number(value);
  return Number.isSafeInteger(revision) && revision > 0 ? revision : null;
}

function appendNew<T>(rows: readonly T[], more: readonly T[], key: (row: T) => string): readonly T[] {
  const seen = new Set(rows.map(key));
  return [...rows, ...more.filter((row) => !seen.has(key(row)))];
}

export function draftListActions(lists: DraftLists, owner: AbortSignal, snapshot: () => DraftView,
  update: (patch: Partial<DraftView>) => void, denied: (cause: unknown) => void) {
  const modelFor = (models: readonly WorkspaceModel[]) => snapshot().draft?.setup_model_ref ?? defaultChatModel(models, snapshot().model);
  const changes: ListUpdates = {
    ready: (list, result) => {
      if (list === DRAFT_LIST.models) {
        const {models, moreModels} = result;
        update({models, moreModels, model: modelFor(models), modelListStatus: DRAFT_LIST_STATUS.ready});
      } else {
        const {draft} = snapshot();
        const {cards, moreChats} = draft ? lists.updateDraft(draft) : result;
        update({cards, moreChats, listStatus: DRAFT_LIST_STATUS.ready});
      }
    },
    failed: (list, cause) => {
      if (losesAccess(cause)) denied(cause);
      else if (list === DRAFT_LIST.models) update({modelListStatus: DRAFT_LIST_STATUS.unavailable});
      else update({listStatus: DRAFT_LIST_STATUS.unavailable});
    },
  };
  const loadModels = async (): Promise<void> => { await lists.first(DRAFT_LIST.models, owner, changes); };
  return {
    initialize: async () => {
      update({listStatus: DRAFT_LIST_STATUS.loading, modelListStatus: DRAFT_LIST_STATUS.loading, error: null});
      await lists.open(owner, changes);
    },
    loadModels,
    retryModels: async () => {
      const view = snapshot();
      if (view.inaccessible || view.modelListStatus !== DRAFT_LIST_STATUS.unavailable) return;
      update({modelListStatus: MODEL_LIST_RETRYING});
      await loadModels();
    },
    loadMore: async (list: DraftList) => {
      if (snapshot().inaccessible) return;
      try {
        const page = await lists.more(list, owner);
        if (page) update({...page, model: modelFor(page.models)});
      } catch (cause) {
        if (!owner.aborted && losesAccess(cause)) denied(cause);
        throw cause;
      }
    },
  };
}
