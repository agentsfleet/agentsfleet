"use client";

import { useEffect, useState, useSyncExternalStore } from "react";
import { DraftClient } from "@/lib/api/drafts";
import type { DraftIdentity } from "@/lib/api/draft-state";
import type { WorkspaceModel } from "@/lib/api/draft-types";
import { DRAFT_LIST, DRAFT_LIST_STATUS } from "@/lib/api/draft-list-types";
import { definiteRefusal, setupErrorMessage, SetupUnavailable } from "@/lib/api/draft-request";
import { SOURCE_KIND } from "@/lib/api/draft-vocabulary";

export type FleetModelOwner = DraftIdentity & {fleet: string; revision: string; model: string};
type Snapshot = {
  models: readonly WorkspaceModel[]; more: boolean; ready: boolean; loading: boolean; saving: boolean;
  busy: boolean; pending: boolean; inaccessible: boolean; retryable: boolean; reloadRequired: boolean;
  confirmed: WorkspaceModel | null; error: string | null;
};
const EMPTY: Snapshot = {models: [], more: false, ready: false, loading: false, saving: false,
  busy: false, pending: false, inaccessible: false, retryable: false, reloadRequired: false, confirmed: null, error: null};
const LOAD_FAILED = "Models couldn’t be loaded. Try again.";
const MODEL_UNAVAILABLE = "That model is no longer available. Reload the choices and try again.";
const REVIEW_MISMATCH = "The selected model could not be confirmed. Reload this fleet and try again.";
const ignore = (): void => undefined;
const emptySnapshot = (): Snapshot => EMPTY;
const emptySubscribe = (): (() => void) => ignore;

/** A model save owns its own draft, independently of unsaved setup-chat edits. */
class FleetModelSave {
  readonly #client: DraftClient;
  readonly #listeners = new Set<() => void>();
  readonly #owner: FleetModelOwner;
  readonly #unsubscribe: () => void;
  #snapshot = EMPTY;
  #choice: WorkspaceModel | null = null;
  #step: "edit" | "apply" = "edit";
  #disposed = false;
  constructor(owner: FleetModelOwner) {
    this.#owner = owner; this.#client = new DraftClient(owner);
    this.#unsubscribe = this.#client.subscribe(() => this.#update({}));
  }
  readonly getSnapshot = (): Snapshot => this.#snapshot;
  readonly subscribe = (listener: () => void): (() => void) => {this.#listeners.add(listener); return () => this.#listeners.delete(listener);};
  dispose(): void {this.#disposed = true; this.#unsubscribe(); this.#client.dispose(); this.#listeners.clear();}
  async load(more: boolean): Promise<void> {
    if (this.#snapshot.loading || this.#snapshot.saving || this.#snapshot.pending || this.#snapshot.inaccessible || this.#snapshot.reloadRequired) return;
    this.#update({loading: true, error: null});
    try { if (more) await this.#client.loadMore(DRAFT_LIST.models); else await this.#client.initialize(); }
    catch (cause) { this.#update({error: this.#client.getSnapshot().inaccessible ? setupErrorMessage(cause) : LOAD_FAILED}); }
    finally { this.#update({loading: false}); }
  }
  async select(ref: string): Promise<boolean> {
    const current = this.#client.getSnapshot();
    if (this.#snapshot.saving || this.#snapshot.reloadRequired || current.busy || current.pending || current.inaccessible || ref === (this.#snapshot.confirmed?.ref ?? this.#owner.model)) return false;
    const model = current.models.find(model => model.ref === ref && model.eligibility.runtime);
    if (!model) {this.#update({error: MODEL_UNAVAILABLE}); return false;}
    this.#choice = model; this.#step = "edit";
    return this.#run(false);
  }
  retry(): Promise<boolean> { return this.#run(true); }
  resetRefusal(): void {
    if (this.#snapshot.reloadRequired && !this.#snapshot.saving) this.#update({reloadRequired: false, error: null});
  }
  async #run(retry: boolean): Promise<boolean> {
    const choice = this.#choice;
    if (this.#snapshot.saving || !choice) return false;
    this.#update({saving: true, error: null});
    try {
      if (retry && this.#client.getSnapshot().pending) {
        await this.#client.retry();
        if (this.#step === "edit") this.#step = "apply";
      } else if (this.#step === "edit") await this.#edit(choice);
      if (!this.#client.getSnapshot().savedSetupRevision) await this.#apply(choice);
      this.#finish(choice);
      return true;
    } catch (cause) {
      if (definiteRefusal(cause)) {
        this.#choice = null;
        const view = this.#client.getSnapshot();
        this.#update({reloadRequired: cause instanceof SetupUnavailable && [409, 412].includes(cause.status) && !view.pending});
        if (view.draft && !view.pending && !view.inaccessible) await this.#client.remove().catch(ignore);
      }
      this.#update({error: setupErrorMessage(cause)});
      return false;
    } finally { this.#update({saving: false}); }
  }
  async #edit(choice: WorkspaceModel): Promise<void> {
    if (this.#client.getSnapshot().draft) await this.#client.remove();
    this.#client.fresh({kind: SOURCE_KIND.fleet, fleet_id: this.#owner.fleet, setup_revision: this.#owner.revision});
    const preview = await this.#client.refreshSetup();
    await this.#client.edit({...preview, runtimeModel: choice.ref});
  }
  async #apply(choice: WorkspaceModel): Promise<void> {
    if (this.#client.getSnapshot().draft?.review?.effective.runtime_model_ref !== choice.ref) throw new SetupUnavailable(REVIEW_MISMATCH, 409);
    this.#step = "apply";
    await this.#client.apply();
  }
  #finish(choice: WorkspaceModel): void {
    this.#choice = null; this.#update({confirmed: choice});
    // Cleanup cannot turn a confirmed setup save into a failed save.
    void this.#client.remove().catch(ignore);
  }
  #update(patch: Partial<Snapshot>): void {
    if (this.#disposed) return;
    const view = this.#client.getSnapshot();
    this.#snapshot = {...this.#snapshot, ...patch, models: view.models.filter(model => model.eligibility.runtime),
      more: view.moreModels, ready: view.listStatus === DRAFT_LIST_STATUS.ready, busy: view.busy, pending: view.pending,
      inaccessible: view.inaccessible, retryable: this.#choice !== null};
    for (const listener of this.#listeners) listener();
  }
}

export function useFleetModel(owner: FleetModelOwner | null) {
  const [session, setSession] = useState<FleetModelSave | null>(null);
  useEffect(() => {
    if (!owner) {setSession(null); return;}
    const current = new FleetModelSave(owner);
    setSession(current);
    return () => current.dispose();
  }, [owner]);
  const view = useSyncExternalStore(session?.subscribe ?? emptySubscribe, session?.getSnapshot ?? emptySnapshot, emptySnapshot);
  return {...view, locked: !session || view.saving || view.busy || view.pending || view.inaccessible || view.reloadRequired,
    load: (more = false) => session?.load(more), select: (ref: string) => session?.select(ref), retry: () => session?.retry(),
    resetRefusal: () => session?.resetRefusal()};
}
