import type { z } from "zod";
import type { CachedDraft } from "./draft-cache";
import { DraftPublication, type PlatformPublisher } from "./draft-publication";
export type { PlatformPublisher } from "./draft-publication";
import type { Draft, Fleet, Review, Source, Start } from "./draft-types";
import { DraftLive } from "./draft-live";
import { DraftOperation } from "./draft-operation";
import { latestSaveAnswer, saveRequested } from "./draft-chat-save";
import { admitDraftMessage } from "./draft-message";
import { earlierConversations } from "./draft-editor";
import { DraftHistory } from "./draft-history";
import { DraftWatch } from "./draft-watch";
import { DraftLists } from "./draft-lists";
import { DRAFT_LIST_STATUS, defaultChatModel, type DraftList } from "./draft-list-types";
import { EMPTY_DRAFT_VIEW as EMPTY, messagePatch, projectDraftView, type DraftIdentity, type DraftView, type DrawerEdit } from "./draft-state";
import { createdDrawerPatch, drawerPatch } from "./draft-edits";
import { applySavedSetup, continueSavedDraft, readFleetSetup, resumeSavedSetup, retrySavedSetup } from "./draft-continuation";
import { editableSetup, readySetup, targetFleetSetup } from "./draft-ready-setup";
import { updateSavedSetup } from "./draft-activity";
import { chatRecovery } from "./draft-recovery";
import { REQUEST_METHOD, deleteDraft, definiteRefusal, losesAccess, setupErrorMessage, setupRequest, SetupUnavailable, type SetupRequest } from "./draft-request";
import { CancellationSchema, DRAFT_STATE, DraftSchema, FleetSchema, MessageInputSchema, SETUP_HEADERS, ScheduleSchema, appliedNotice } from "./draft-types";
import { SOURCE_KIND, START_KIND } from "./draft-vocabulary";
import { scheduleSynced, scheduleSyncPath, type Schedule } from "./draft-schedules";

export class DraftClient {
  readonly #owner = new AbortController();
  readonly #operation = new DraftOperation();
  readonly #history = new DraftHistory();
  readonly #lists: DraftLists;
  readonly #listeners = new Set<() => void>();
  readonly #base: string;
  readonly #publication: DraftPublication;
  readonly #replies = new DraftLive(patch => this.#update(patch));
  #view = new AbortController();
  #snapshot: DraftView = EMPTY;
  #source: Source = {kind: SOURCE_KIND.blank};
  readonly #watching = new DraftWatch();
  #continueSave: (() => Promise<void>) | undefined;
  #setupTarget: Fleet | undefined;
  constructor(identity: DraftIdentity, platformPublisher?: PlatformPublisher) {
    this.#base = `/live/v1/workspaces/${encodeURIComponent(identity.workspace)}`;
    this.#lists = new DraftLists(this.#base);
    this.#publication = new DraftPublication(this.#base, identity.workspace, (request, parser) => this.#request(request, parser), platformPublisher);
  }
  get canStagePlatform(): boolean { return this.#publication.canStagePlatform; }
  readonly getSnapshot = (): DraftView => this.#snapshot;
  readonly subscribe = (listener: () => void): (() => void) => { this.#listeners.add(listener); return () => this.#listeners.delete(listener); };
  dispose(): void {
    this.#view.abort(); this.#owner.abort(); this.#operation.clear(); this.#history.clear(); this.#replies.clear(); this.#lists.clear();
    this.#continueSave = undefined; this.#setupTarget = undefined; this.#source = {kind: SOURCE_KIND.blank};
    this.#snapshot = EMPTY; this.#listeners.clear();
  }
  async initialize(): Promise<void> {
    this.#update({listStatus: DRAFT_LIST_STATUS.loading, error: null});
    try {
      const lists = await this.#lists.open(this.#owner.signal);
      this.#update({...lists, ...(this.#snapshot.draft ? this.#lists.updateDraft(this.#snapshot.draft) : {}),
        listStatus: DRAFT_LIST_STATUS.ready, model: this.#snapshot.draft?.authoring_model_ref ?? defaultChatModel(lists.models, this.#snapshot.model)});
    } catch (cause) {
      if (!this.#owner.signal.aborted) { this.#update({listStatus: DRAFT_LIST_STATUS.unavailable}); this.#failed(cause, true); }
      throw cause;
    }
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
      await this.#continueAuthoring();
      if (this.#snapshot.draft) await this.#patch({authoring_model_ref: model});
      this.#update({model});
    });
  }
  async resume(id: string, cached?: CachedDraft): Promise<void> {
    if (this.#snapshot.busy) throw new SetupUnavailable("A setup request is still in progress.", 409);
    this.#idle(); this.#reset();
    if (cached?.draft?.id === id) this.#update(cached);
    await this.#act(async () => {
      const draft = await this.#request({path: `${this.#base}/drafts/${encodeURIComponent(id)}`}, DraftSchema);
      this.#update({...this.#lists.updateDraft(draft), draft, model: draft.authoring_model_ref});
      if (draft.state !== DRAFT_STATE.editing && draft.linked_fleet_id) {
        this.#continueSave = await resumeSavedSetup(this.#base, draft.linked_fleet_id, AbortSignal.any([this.#owner.signal, this.#view.signal]),
          fleet => this.#savedSetup(draft.id, fleet), () => this.#read(draft.id));
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
      await this.#continueAuthoring();
      const id = this.#snapshot.draft?.id;
      const source = this.#source;
      const model = this.#snapshot.model;
      const signal = AbortSignal.any([this.#owner.signal, this.#view.signal]);
      try {
        await this.#operation.run({id, source, model, message}, async (operation_id) => {
          this.#update({optimistic: {operation: operation_id, text: message, save: saveRequested(message), createdAt: new Date().toISOString()}});
          const draft = await admitDraftMessage(this.#base, {id, source, model, message}, operation_id, signal);
          if (draft) {
            this.#update({...this.#lists.updateDraft(draft), draft});
            void this.#refreshSummary(draft.id);
          }
        });
      } catch (cause) {
        if (definiteRefusal(cause)) {this.#update({optimistic: null});}
        throw cause;
      }
      signal.throwIfAborted();
      this.#startWatching();
      await this.#read(this.#draft().id);
      this.#startWatching();
    });
  }
  refreshSetup(source?: Source): Promise<DrawerEdit> {
    return this.#act(async () => {
      this.#idle();
      await this.#continueAuthoring();
      if (source && !this.#snapshot.draft) this.#source = source;
      const refreshed = await editableSetup(this.#base, this.#source, this.#snapshot.draft,
        AbortSignal.any([this.#owner.signal, this.#view.signal]),
        async () => {await this.#read(this.#draft().id); return this.#draft();}, (draft, review) => this.#targetSetup(draft, review));
      if (refreshed.preview) this.#update({preview: refreshed.preview});
      return refreshed.edit;
    }, false, "reading");
  }
  refreshFleetSetup(readinessOnly = false, currentRevision = false): Promise<void> {
    if (readinessOnly && !this.#snapshot.fleet) return this.refreshSetup().then(() => undefined);
    return this.#act(async () => {
      this.#idle(); const signal = AbortSignal.any([this.#owner.signal, this.#view.signal]);
      const patch = await readFleetSetup(this.#base, this.#source, this.#snapshot, signal, readinessOnly, currentRevision);
      if (currentRevision && patch.fleet) this.#source = {kind: SOURCE_KIND.fleet, fleet_id: patch.fleet.id, setup_revision: patch.fleet.setup_revision};
      this.#update(patch);
      if (readinessOnly && this.#snapshot.draft) await this.#read(this.#snapshot.draft.id);
      if (!readinessOnly && patch.draft) this.#history.seed(this.#snapshot.messages, patch.messagesStartingAfter ?? undefined, this.#snapshot.truncated);
      this.#startWatching();
    }, false, "reading");
  }
  loadEarlierConversations(): Promise<void> {
    return this.#act(async () => {
      const signal = AbortSignal.any([this.#owner.signal, this.#view.signal]);
      this.#update(await earlierConversations(this.#base, this.getSnapshot, signal));
    });
  }
  edit(input: DrawerEdit): Promise<void> {
    return this.#act(async () => {
      this.#idle();
      await this.#continueAuthoring();
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
          const patch = createdDrawerPatch(created, input);
          if (patch) await this.#patch(patch, selectionTag);
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
        review_digest: review.review_digest, start, origin: "dashboard"};
      await this.#operation.run({save: body}, async (key) => {
        const request = {path: `${this.#base}/fleets`, method: REQUEST_METHOD.post, body, headers: {[SETUP_HEADERS.key]: key}};
        this.#continueSave = async () => {
          const fleet = await this.#request(request, FleetSchema);
          await this.#savedSetup(draft.id, fleet);
          await this.#read(draft.id);
        };
        await this.#continueSave();
      });
    }, false, "saving");
  }
  apply(): Promise<void> {
    return this.#act(async () => {
      this.#idle();
      const {draft, review} = await this.#readySetup();
      await applySavedSetup(this.#base, draft, review, this.#setupTarget, this.#operation,
        AbortSignal.any([this.#owner.signal, this.#view.signal]), fleet => this.#savedSetup(draft.id, fleet));
      this.#update({notice: appliedNotice(this.#snapshot.schedules)});
    }, false, "saving");
  }
  publish(label: string, append: boolean): Promise<void> {
    return this.#act(async () => {
      this.#idle();
      const {draft} = await this.#readySetup();
      const publication = await this.#publication.prepare(draft, label, append);
      await this.#operation.run({publish: publication}, async key => {
        this.#update({notice: await this.#publication.publish(publication, key)});
        await this.#read(draft.id); await this.#refreshSummary(draft.id);
      });
    }, false, "publishing");
  }
  stagePlatform(label: string): Promise<void> {
    return this.#act(async () => {
      this.#idle();
      const {draft} = await this.#readySetup();
      await this.#operation.run({platform: draft.candidate.id, label}, async key => {
        const notice = await this.#publication.stage(draft, label, key);
        this.#owner.signal.throwIfAborted(); this.#update({notice});
        await this.#read(draft.id);
      });
    });
  }
  retry(): Promise<void> {
    return this.#act(async () => {
      if (this.#operation.pending) await this.#operation.retry();
      else if (this.#continueSave) await this.#continueSave();
      else await retrySavedSetup(this.#base, this.#snapshot, AbortSignal.any([this.#owner.signal, this.#view.signal]), (id, fleet) => this.#savedSetup(id, fleet), id => this.#read(id));
      this.#startWatching();
    }, false, this.#snapshot.setupAction);
  }
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
      this.#watching.stop(); this.#startWatching();
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
  async #continueAuthoring(): Promise<void> {
    const next = await continueSavedDraft(this.#base, this.getSnapshot, AbortSignal.any([this.#owner.signal, this.#view.signal]));
    if (!next) return;
    this.#history.clear(); this.#replies.clear(); this.#watching.stop(); this.#continueSave = undefined; this.#setupTarget = undefined;
    this.#source = next.source; this.#update(next.patch);
  }
  #savedSetup(id: string | undefined, fleet: Fleet): Promise<void> { return updateSavedSetup(this.#base, fleet, AbortSignal.any([this.#owner.signal, this.#view.signal]), patch => this.#update(patch), id ? this.#lists.updateSetup(id, fleet) : {}); }
  #draft() {
    const draft = this.#snapshot.draft;
    if (!draft) throw new SetupUnavailable("Begin a setup conversation first.", 409);
    return draft;
  }
  async #readySetup() {
    const displayed = this.#draft();
    return readySetup(displayed, async () => {await this.#read(displayed.id); this.#idle(); return this.#draft();},
      (draft, review) => this.#targetSetup(draft, review));
  }
  async #targetSetup(draft: Draft, review: Review): Promise<void> {
    this.#setupTarget = undefined;
    const fleet = await targetFleetSetup(this.#base, draft, review, AbortSignal.any([this.#owner.signal, this.#view.signal]));
    if (fleet) this.#update({...this.#lists.updateSetup(draft.id, fleet), fleet});
    this.#setupTarget = fleet;
  }
  #idle(): void {
    if (this.#operation.pending || chatRecovery(this.#snapshot).pending) throw new SetupUnavailable("Stop or finish the current operation before editing setup.", 409);
  }
  async #patch(body: unknown, expected?: string): Promise<void> {
    const current = this.#draft();
    const draft = await this.#request({path: `${this.#base}/drafts/${current.id}`, method: REQUEST_METHOD.patch,
      body, headers: {[SETUP_HEADERS.match]: expected ?? current.etag}}, DraftSchema);
    this.#update({...this.#lists.updateDraft(draft), draft});
  }
  async #read(id: string): Promise<void> {
    const signal = AbortSignal.any([this.#owner.signal, this.#view.signal]);
    const {messages, draft} = await this.#history.synchronize(`${this.#base}/drafts/${id}`, signal).catch(cause => {signal.throwIfAborted(); throw cause;});
    signal.throwIfAborted();
    this.#replies.reconcile(messages, draft.cancelled_through);
    this.#update({...this.#lists.updateDraft(draft), ...messagePatch(this.#snapshot, draft, messages, this.#history.truncated)});
    const answer = latestSaveAnswer(messages);
    if (draft.state !== DRAFT_STATE.editing && draft.linked_fleet_id && answer?.save_requested) {
      const fleet = await this.#request({path: `${this.#base}/fleets/${draft.linked_fleet_id}`}, FleetSchema);
      if (fleet.setup_revision !== this.#snapshot.savedSetupRevision) await this.#savedSetup(draft.id, fleet);
    }
  }
  #startWatching(): void {
    const id = this.#snapshot.draft?.id;
    if (!id || (!chatRecovery(this.#snapshot).canStop && !this.#snapshot.optimistic)) return;
    this.#watching.follow({id, path: `${this.#base}/drafts/${id}`, owner: this.#owner.signal, view: this.#view.signal,
      reconcile: async () => { await this.#read(id); return chatRecovery(this.#snapshot).canStop || Boolean(this.#snapshot.optimistic); },
      needsPolling: () => chatRecovery(this.#snapshot).blocked,
      progress: {gap: () => this.#replies.gap(), delta: data => this.#snapshot.draft?.id === id
        && this.#replies.accept(data, id, this.#snapshot.draft.cancelled_through, this.#snapshot.messages),
        context: data => this.#snapshot.draft?.id === id && this.#replies.context(data, id, this.#snapshot.draft.cancelled_through, this.#snapshot.messages)},
      finished: () => { if (chatRecovery(this.#snapshot).pending) this.#update({notice: "Still waiting for a result."}); },
      failed: cause => this.#failed(cause),
    });
  }
  async #act<T>(action: () => Promise<T>, opening = false, setupAction: DraftView["setupAction"] = null): Promise<T> {
    if (this.#snapshot.inaccessible) throw new SetupUnavailable("Reload this page to check access before continuing.", 403);
    if (this.#snapshot.busy) throw new SetupUnavailable("A setup request is still in progress.", 409);
    this.#update({busy: true, error: null, notice: null, setupAction: this.#operation.pending ? this.#snapshot.setupAction : setupAction});
    try { return await action(); } catch (cause) { if (!this.#owner.signal.aborted && !this.getSnapshot().inaccessible) this.#failed(cause, opening); throw cause; }
    finally { this.#update({busy: false, pending: this.#operation.pending, setupAction: this.#operation.pending ? this.#snapshot.setupAction : null}); }
  }
  #failed(cause: unknown, opening = false): void {
    const error = setupErrorMessage(cause, opening);
    if (losesAccess(cause)) {
      this.#reset(); this.#lists.clear(); this.#update({...EMPTY, listStatus: DRAFT_LIST_STATUS.unavailable, inaccessible: true, error});
    } else this.#update({error});
  }
  #reset(): void {
    this.#view.abort(); this.#view = new AbortController(); this.#history.clear(); this.#replies.clear(); this.#operation.clear();
    this.#continueSave = undefined; this.#setupTarget = undefined; this.#watching.stop();
    this.#update({draft: null, messages: [], previousDrafts: [], conversationCursor: null, activityCursor: null, messagesStartingAfter: null, activities: [], optimistic: null, liveReply: null, contextProgress: null, preview: null, fleet: null, savedSetupRevision: null, setupAction: null, schedules: [], error: null, notice: null, truncated: false, focusRevision: this.#snapshot.focusRevision + 1});
  }
  #refreshSummary(id: string): Promise<void> {
    return this.#lists.refreshSummary(id, AbortSignal.any([this.#owner.signal, this.#view.signal]), patch => this.#update(patch), cause => this.#failed(cause));
  }
  #update(patch: Partial<DraftView>): void {
    if (this.#owner.signal.aborted) return;
    this.#snapshot = projectDraftView(this.#snapshot, patch); for (const listener of this.#listeners) listener();
  }
}
