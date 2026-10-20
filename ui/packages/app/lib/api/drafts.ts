// One account, workspace and creator own every private byte and retry identity.

import type { z } from "zod";
import type { StagePlatform } from "./platform-library-types";
import type { Draft, Fleet, Review, Source, Start } from "./draft-types";
import { z as schema } from "zod";
import { DraftOperation } from "./draft-operation";
import { DraftHistory } from "./draft-history";
import { DraftLists } from "./draft-lists";
import { DRAFT_LIST_STATUS, defaultChatModel, type DraftList } from "./draft-list-types";
import { EMPTY_DRAFT_VIEW as EMPTY, type DraftIdentity, type DraftView, type DrawerEdit } from "./draft-state";
import { drawerPatch } from "./draft-edits";
import { sourcePreview } from "./draft-source";
import { followDraft } from "./draft-events";
import { REQUEST_METHOD, deleteDraft, losesAccess, setupErrorMessage, setupRequest, SetupUnavailable, type SetupRequest } from "./draft-request";
import {
  AcceptedSchema, AppliedSchema, CancellationSchema, CreatedMessageSchema, DRAFT_STATE, DraftSchema, FleetSchema,
  MessageInputSchema, PublishedSchema, SETUP_HEADERS, SOURCE_KIND, START_KIND, ScheduleSchema, appliedNotice, isRunning,
} from "./draft-types";
import { scheduleSynced, scheduleSyncPath, type Schedule } from "./draft-schedules";

export type PlatformPublisher = (body: StagePlatform, operation: string) => Promise<unknown>;

const FOLLOW_LIMIT_MS = 330_000;

export class DraftClient {
  readonly #owner = new AbortController();
  readonly #operation = new DraftOperation();
  readonly #history = new DraftHistory();
  readonly #lists: DraftLists;
  readonly #listeners = new Set<() => void>();
  readonly #base: string;
  readonly #workspace: string;
  readonly #platformPublisher: PlatformPublisher | undefined;
  #view = new AbortController();
  #snapshot: DraftView = EMPTY;
  #source: Source = {kind: SOURCE_KIND.blank};
  #watching: string | undefined;
  #continueSave: (() => Promise<void>) | undefined;
  #setupTarget: Fleet | undefined;
  constructor(identity: DraftIdentity, platformPublisher?: PlatformPublisher) {
    this.#base = `/live/v1/workspaces/${encodeURIComponent(identity.workspace)}`;
    this.#workspace = identity.workspace; this.#platformPublisher = platformPublisher; this.#lists = new DraftLists(this.#base);
  }
  get canStagePlatform(): boolean { return this.#platformPublisher !== undefined; }
  readonly getSnapshot = (): DraftView => this.#snapshot;
  readonly subscribe = (listener: () => void): (() => void) => { this.#listeners.add(listener); return () => this.#listeners.delete(listener); };
  dispose(): void {
    this.#view.abort(); this.#owner.abort(); this.#operation.clear(); this.#history.clear(); this.#lists.clear();
    this.#continueSave = undefined; this.#setupTarget = undefined; this.#source = {kind: SOURCE_KIND.blank};
    this.#snapshot = EMPTY; this.#listeners.clear();
  }
  async initialize(): Promise<void> {
    await this.#act(async () => {
      this.#update({listStatus: DRAFT_LIST_STATUS.loading});
      try {
        const lists = await this.#lists.open(this.#owner.signal);
        this.#update({...lists, listStatus: DRAFT_LIST_STATUS.ready, model: this.#snapshot.draft?.authoring_model_ref ?? defaultChatModel(lists.models, this.#snapshot.model)});
      } catch (cause) {
        this.#update({listStatus: DRAFT_LIST_STATUS.unavailable});
        throw cause;
      }
    }, true);
  }
  /** The next page of models or chats. A failed page is the caller's to show inline; lost access clears every list. */
  async loadMore(list: DraftList): Promise<void> {
    if (this.#snapshot.inaccessible) return;
    try { const lists = await this.#lists.more(list, this.#owner.signal); if (lists) this.#update({...lists, model: this.#snapshot.draft?.authoring_model_ref ?? defaultChatModel(lists.models, this.#snapshot.model)}); } catch (cause) {
      if (!this.#owner.signal.aborted && losesAccess(cause)) this.#failed(cause, true);
      throw cause;
    }
  }
  selectModel(model: string): Promise<void> {
    return this.#act(async () => {
      this.#idle();
      if (this.#snapshot.draft) await this.#patch({authoring_model_ref: model});
      this.#update({model});
    });
  }
  async resume(id: string): Promise<void> {
    if (this.#snapshot.busy) throw new SetupUnavailable("A setup request is still in progress.", 409);
    this.#idle(); this.#reset();
    await this.#act(async () => {
      const draft = await this.#request({path: `${this.#base}/drafts/${encodeURIComponent(id)}`}, DraftSchema);
      this.#update({...this.#lists.updateDraft(draft), draft, model: draft.authoring_model_ref});
      if (draft.state !== DRAFT_STATE.editing && draft.linked_fleet_id) {
        const path = `${this.#base}/fleets/${draft.linked_fleet_id}`;
        this.#savedSetup(draft.id, await this.#request({path}, FleetSchema));
        this.#continueSave = async () => {
          const current = await this.#request({path}, FleetSchema);
          const fleet = await this.#request({path, method: REQUEST_METHOD.patch, body: {status: "active"},
            headers: {[SETUP_HEADERS.match]: current.etag}}, FleetSchema);
          this.#savedSetup(draft.id, fleet); await this.#read(draft.id);
        };
      }
      await this.#read(draft.id); this.#startWatching();
    });
  }
  fresh(source: Source = {kind: SOURCE_KIND.blank}): void {
    if (this.#snapshot.busy) throw new SetupUnavailable("A setup request is still in progress.", 409);
    this.#idle(); this.#reset(); this.#source = source;
  }
  send(message: string): Promise<void> {
    return this.#act(async () => {
      if (!MessageInputSchema.safeParse(message).success) throw new SetupUnavailable("Messages must contain 1 to 32,768 bytes of text.", 400);
      if (!this.#snapshot.model) throw new SetupUnavailable("Choose an authoring model before sending.", 400);
      const id = this.#snapshot.draft?.id;
      const source = this.#source;
      const model = this.#snapshot.model;
      await this.#operation.run({id, source, model, message}, async (operation_id) => {
        if (id) await this.#request({path: `${this.#base}/drafts/${id}/messages`, method: REQUEST_METHOD.post,
          body: {message, operation_id}}, AcceptedSchema);
        else {
          const draft = await this.#request({path: `${this.#base}/drafts`, method: REQUEST_METHOD.post,
            body: {source, authoring_model_ref: model, message, operation_id}}, CreatedMessageSchema);
          this.#update({...this.#lists.updateDraft(draft), draft});
          this.#update(await this.#lists.refreshDraft(draft.id, AbortSignal.any([this.#owner.signal, this.#view.signal])));
        }
        await this.#read(this.#draft().id);
      });
      this.#startWatching();
    });
  }
  refreshSetup(): Promise<void> {
    return this.#act(async () => {
      this.#idle();
      if (!this.#snapshot.draft) {
        this.#update({preview: await sourcePreview(this.#base, this.#source, AbortSignal.any([this.#owner.signal, this.#view.signal]))});
        return;
      }
      await this.#read(this.#draft().id);
      const draft = this.#draft();
      if (!draft.review) throw new SetupUnavailable("Continue chatting or edit instructions before saving setup.", 409);
      await this.#targetSetup(draft, draft.review);
    });
  }
  edit(input: DrawerEdit): Promise<void> {
    return this.#act(async () => {
      this.#idle();
      const draft = this.#snapshot.draft;
      if (!draft) {
        if (!this.#snapshot.model) throw new SetupUnavailable("Choose an authoring model first.", 400);
        const source = this.#source.kind === SOURCE_KIND.blank ? {kind: SOURCE_KIND.definition, definition: input.definition} : this.#source;
        const model = this.#snapshot.model;
        let selectionTag: string | undefined;
        await this.#operation.run({input, source, model}, async (operation_id) => {
          const created = await this.#request({path: `${this.#base}/drafts`, method: REQUEST_METHOD.post,
            body: {source, authoring_model_ref: model, operation_id}}, DraftSchema);
          this.#update({draft: created});
          this.#update(await this.#lists.refreshDraft(created.id, AbortSignal.any([this.#owner.signal, this.#view.signal])));
          selectionTag ??= created.etag;
          if (!created.candidate) throw new SetupUnavailable("The selected source has no editable setup.", 409);
          if (JSON.stringify(created.candidate.definition) !== JSON.stringify(input.definition)) {
            await this.#patch(drawerPatch(created.candidate.definition, input), selectionTag);
          } else if (created.review?.effective.instance_name !== input.instanceName) await this.#patch({instance_name: input.instanceName}, selectionTag);
        });
      } else {
        if (!draft.candidate) throw new SetupUnavailable("Continue chatting until the first setup is proposed before editing it.", 409);
        await this.#patch(drawerPatch(draft.candidate.definition, input));
      }
    });
  }
  save(start: Start = {kind: START_KIND.none}): Promise<void> {
    return this.#act(async () => {
      this.#idle();
      const {draft, review} = await this.#readySetup();
      const body = {source: {kind: "draft", draft_candidate_id: draft.candidate.id}, name: review.effective.instance_name,
        review_digest: review.review_digest, start};
      await this.#operation.run({save: body}, async (key) => {
        const request = {path: `${this.#base}/fleets`, method: REQUEST_METHOD.post, body, headers: {[SETUP_HEADERS.key]: key}};
        this.#continueSave = async () => {
          const fleet = await this.#request(request, FleetSchema);
          this.#savedSetup(draft.id, fleet);
          await this.#read(draft.id);
        };
        await this.#continueSave();
      });
    });
  }
  apply(): Promise<void> {
    return this.#act(async () => {
      this.#idle();
      const {draft, review} = await this.#readySetup();
      const fleet = this.#setupTarget;
      if (!draft.linked_fleet_id || !fleet) throw new SetupUnavailable("The current fleet setup could not be validated. Try saving again.", 409);
      const body = {draft_candidate_id: draft.candidate.id, review_digest: review.review_digest};
      const headers = {[SETUP_HEADERS.match]: fleet.setup_etag};
      await this.#operation.run({apply: body, headers}, async (key) => {
        const applied = await this.#request({path: `${this.#base}/fleets/${draft.linked_fleet_id}/revisions`, method: REQUEST_METHOD.post,
          body, headers: {...headers, [SETUP_HEADERS.key]: key}}, AppliedSchema);
        this.#update({...this.#lists.updateSetup(draft.id, {id: applied.fleet_id, setup_revision: applied.setup_revision}),
          schedules: applied.schedules, notice: appliedNotice(applied.schedules)});
      });
    });
  }
  publish(label: string, append: boolean): Promise<void> {
    return this.#act(async () => {
      this.#idle();
      const {draft} = await this.#readySetup();
      const target = append ? draft.publication?.library_id : undefined;
      if (append && !target) throw new SetupUnavailable("Choose a previously published Library entry before adding a revision.", 409);
      const path = target ? `${this.#base}/fleet-libraries/${encodeURIComponent(target)}/revisions` : `${this.#base}/fleet-libraries`;
      const body = target ? {draft_candidate_id: draft.candidate.id} : {draft_candidate_id: draft.candidate.id, label};
      const current = target ? await this.#request({path: `${this.#base}/fleet-libraries/${encodeURIComponent(target)}`}, schema.object({etag: schema.string()})) : null;
      await this.#operation.run({publish: path, body}, async (key) => {
        const receipt = await this.#request({path, method: REQUEST_METHOD.post, body,
          headers: {[SETUP_HEADERS.key]: key, ...(current ? {[SETUP_HEADERS.match]: current.etag} : {})}}, PublishedSchema);
        this.#update({notice: `Published Library revision ${receipt.revision_id}. Running fleets keep their pinned setup.`});
        await this.#read(draft.id);
        this.#update(await this.#lists.refreshDraft(draft.id, AbortSignal.any([this.#owner.signal, this.#view.signal])));
      });
    });
  }
  stagePlatform(label: string): Promise<void> {
    return this.#act(async () => {
      this.#idle();
      const {draft} = await this.#readySetup();
      const publish = this.#platformPublisher;
      if (!publish) throw new SetupUnavailable("Platform Library operator scope is required.", 403);
      const body = {workspace_id: this.#workspace, draft_candidate_id: draft.candidate.id, label};
      await this.#operation.run({platform: body}, async (key) => {
        const receipt = PublishedSchema.parse(await publish(body, key));
        this.#owner.signal.throwIfAborted();
        this.#update({notice: `Platform entry ${receipt.library_id} is staged. Publish it explicitly in the platform Library.`});
        await this.#read(draft.id);
      });
    });
  }
  retry(): Promise<void> {
    return this.#act(async () => {
      if (this.#operation.pending) await this.#operation.retry();
      else if (this.#continueSave) await this.#continueSave();
      else if (this.#snapshot.draft) await this.#read(this.#draft().id);
      else throw new SetupUnavailable("There is no operation to retry.", 409);
      this.#startWatching();
    });
  }
  /** Retries one schedule's registration through its `:sync` action, the one the CLI names. */
  syncSchedule(schedule: Schedule): Promise<void> {
    const path = scheduleSyncPath(this.#base, schedule);
    return this.#act(async () => this.#update(scheduleSynced(this.#snapshot, await this.#request({method: REQUEST_METHOD.post, path}, ScheduleSchema))));
  }
  stop(): Promise<void> {
    return this.#act(async () => {
      const id = this.#draft().id;
      await this.#read(id);
      await this.#request({path: `${this.#base}/drafts/${id}/cancellations`, method: REQUEST_METHOD.post,
        body: {through_sequence: this.#history.highest}}, CancellationSchema);
      await this.#read(id);
    });
  }
  rename(title: string): Promise<void> {
    return this.#act(async () => { this.#idle(); await this.#patch({title}); });
  }
  remove(id?: string, signal?: AbortSignal): Promise<void> {
    return this.#act(async () => {
      this.#idle();
      const target = id ?? this.#draft().id;
      await deleteDraft(this.#base, target, AbortSignal.any([this.#owner.signal, this.#view.signal, ...(signal ? [signal] : [])]));
      this.#update(this.#lists.remove(target));
      if (this.#snapshot.draft?.id === target) this.#reset();
    });
  }
  #request<T>(request: SetupRequest, parser: z.ZodType<T>): Promise<T> {
    return setupRequest(request, parser, AbortSignal.any([this.#owner.signal, this.#view.signal]));
  }
  #savedSetup(id: string, fleet: Fleet): void {
    this.#update({...this.#lists.updateSetup(id, fleet), fleet});
  }
  #draft() {
    const draft = this.#snapshot.draft;
    if (!draft) throw new SetupUnavailable("Begin a private conversation first.", 409);
    return draft;
  }
  async #readySetup() {
    const displayed = this.#draft();
    if (!displayed.candidate || !displayed.review) throw new SetupUnavailable("Continue chatting until a complete setup is ready.", 409);
    await this.#read(displayed.id);
    this.#idle();
    const draft = this.#draft();
    if (!draft.candidate || !draft.review || draft.candidate.id !== displayed.candidate.id || draft.review.review_digest !== displayed.review.review_digest) {
      throw new SetupUnavailable("The setup changed. Check the updated instructions, then try again.", 409);
    }
    await this.#targetSetup(draft, draft.review);
    return {draft: {...draft, candidate: draft.candidate}, review: draft.review};
  }
  async #targetSetup(draft: Draft, review: Review): Promise<void> {
    this.#setupTarget = undefined;
    if (!draft.linked_fleet_id || review.effective.target_setup_revision === undefined) return;
    const fleet = await this.#request({path: `${this.#base}/fleets/${draft.linked_fleet_id}`}, FleetSchema);
    if (fleet.setup_revision !== review.effective.target_setup_revision) throw new SetupUnavailable("This fleet changed. Open Edit from its current setup and try again.", 409);
    this.#update(this.#lists.updateSetup(draft.id, fleet));
    this.#setupTarget = fleet;
  }
  #idle(): void {
    if (this.#operation.pending || this.#snapshot.messages.some(isRunning)) throw new SetupUnavailable("Stop or finish the current operation before editing setup.", 409);
  }
  async #patch(body: unknown, expected?: string): Promise<void> {
    const current = this.#draft();
    const draft = await this.#request({path: `${this.#base}/drafts/${current.id}`, method: REQUEST_METHOD.patch,
      body, headers: {[SETUP_HEADERS.match]: expected ?? current.etag}}, DraftSchema);
    this.#update({...this.#lists.updateDraft(draft), draft});
  }
  async #read(id: string): Promise<void> {
    const signal = AbortSignal.any([this.#owner.signal, this.#view.signal]);
    const {messages, draft} = await this.#history.synchronize(`${this.#base}/drafts/${id}`, signal);
    signal.throwIfAborted();
    this.#update({...this.#lists.updateDraft(draft), draft, model: draft.authoring_model_ref, messages, truncated: this.#history.truncated});
  }
  #startWatching(): void {
    const id = this.#snapshot.draft?.id;
    if (!id || this.#watching === id || !this.#snapshot.messages.some(isRunning)) return;
    this.#watching = id;
    const view = this.#view;
    void this.#watch(id, view.signal).catch((cause: unknown) => {
      if (!view.signal.aborted) this.#failed(cause);
    }).finally(() => { if (this.#view === view) this.#watching = undefined; });
  }
  async #watch(id: string, view: AbortSignal): Promise<void> {
    const owner = AbortSignal.any([this.#owner.signal, view, AbortSignal.timeout(FOLLOW_LIMIT_MS)]);
    await followDraft(`${this.#base}/drafts/${id}`, id, owner, async () => {
      await this.#read(id);
      return this.#snapshot.messages.some(isRunning);
    });
    if (!view.aborted && this.#snapshot.messages.some(isRunning)) this.#update({notice: "Work is still pending. Refresh its durable status or Stop."});
  }
  async #act(action: () => Promise<void>, opening = false): Promise<void> {
    if (this.#snapshot.inaccessible) throw new SetupUnavailable("Reload this page to check access before continuing.", 403);
    if (this.#snapshot.busy) throw new SetupUnavailable("A setup request is still in progress.", 409);
    this.#update({busy: true, error: null, notice: null});
    try { await action(); } catch (cause) { if (!this.#owner.signal.aborted) this.#failed(cause, opening); throw cause; }
    finally { this.#update({busy: false, pending: this.#operation.pending}); }
  }
  #failed(cause: unknown, opening = false): void {
    const error = setupErrorMessage(cause, opening);
    if (losesAccess(cause)) {
      this.#reset(); this.#lists.clear(); this.#update({...EMPTY, listStatus: DRAFT_LIST_STATUS.unavailable, inaccessible: true, error});
    } else this.#update({error});
  }
  #reset(): void {
    this.#view.abort(); this.#view = new AbortController(); this.#history.clear(); this.#operation.clear();
    this.#continueSave = undefined; this.#setupTarget = undefined; this.#watching = undefined;
    this.#update({draft: null, messages: [], preview: null, fleet: null, schedules: [], error: null, notice: null, truncated: false});
  }
  #update(patch: Partial<DraftView>): void {
    if (this.#owner.signal.aborted) return;
    this.#snapshot = {...this.#snapshot, ...patch}; for (const listener of this.#listeners) listener();
  }
}
