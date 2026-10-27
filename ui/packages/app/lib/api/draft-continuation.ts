import type { DraftView } from "./draft-state";
import type { Draft, Fleet, Review, Source } from "./draft-types";
import type { DraftOperation } from "./draft-operation";
import { DraftHistory } from "./draft-history";
import { REQUEST_METHOD, setupRequest, SetupUnavailable } from "./draft-request";
import { AppliedSchema, DRAFT_STATE, DraftCardsSchema, FleetSchema, SETUP_HEADERS, isRunning } from "./draft-types";
import { SOURCE_KIND } from "./draft-vocabulary";
import { AGENTSFLEET_STATUS } from "./fleets-types";
import { readSetupActivity } from "./draft-activity";

const RELATED_CHAT_LIMIT = 25;
const HISTORY_CONCURRENCY = 3;
const SETUP_CHANGED = "This fleet changed. Open Edit setup from its current setup.";

export async function resumeSavedSetup(base: string, id: string, signal: AbortSignal, saved: (fleet: Fleet) => Promise<void>, read: () => Promise<void>): Promise<() => Promise<void>> {
  const path = `${base}/fleets/${id}`;
  await saved(await setupRequest({path}, FleetSchema, signal));
  return async () => {
    await saved(await activateSavedFleet(base, id, signal));
    await read();
  };
}

/** An accepted Apply is retried with the same key until its current readiness can be read. */
export async function applySavedSetup(base: string, draft: Draft, review: Review, target: Fleet | undefined,
  operation: DraftOperation, signal: AbortSignal, saved: (fleet: Fleet) => Promise<void>): Promise<void> {
  if (!draft.linked_fleet_id || !target || !draft.candidate) throw new SetupUnavailable("The current fleet setup could not be validated. Try saving again.", 409);
  const body = {draft_candidate_id: draft.candidate.id, review_digest: review.review_digest};
  const headers = {[SETUP_HEADERS.match]: target.setup_etag};
  await operation.run({apply: body, headers}, async key => {
    const applied = await setupRequest({path: `${base}/fleets/${target.id}/revisions`, method: REQUEST_METHOD.post,
      body, headers: {...headers, [SETUP_HEADERS.key]: key}}, AppliedSchema, signal);
    const fleet = await setupRequest({path: `${base}/fleets/${target.id}`}, FleetSchema, signal);
    if (BigInt(fleet.setup_revision) < BigInt(applied.setup_revision)) throw new SetupUnavailable("Setup was saved, but its latest readiness could not be loaded. Retry to check it.", 502);
    await saved(fleet);
  });
}

export async function activateSavedFleet(base: string, id: string, signal: AbortSignal): Promise<Fleet> {
  const path = `${base}/fleets/${id}`;
  const current = await setupRequest({path}, FleetSchema, signal);
  if (current.status !== AGENTSFLEET_STATUS.INSTALLING && current.status !== AGENTSFLEET_STATUS.ACTIVE) return current;
  return setupRequest({path, method: REQUEST_METHOD.patch, body: {status: "active"},
    headers: {[SETUP_HEADERS.match]: current.etag}}, FleetSchema, signal);
}

/** Finish a committed setup while retaining its creator's conversation. */
export async function retrySavedSetup(base: string, view: DraftView, signal: AbortSignal,
  saved: (id: string | undefined, fleet: Fleet) => Promise<void>, read: (id: string) => Promise<void>): Promise<void> {
  const id = view.draft?.id;
  if (view.fleet && (!view.draft || savedDraft(view))) {
    await saved(id, await activateSavedFleet(base, view.fleet.id, signal));
    if (id) await read(id);
  } else if (id) await read(id);
  else throw new SetupUnavailable("There is no operation to retry.", 409);
}

export function savedDraft(view: DraftView): view is DraftView & {draft: Draft} {
  return Boolean(view.draft && (view.savedSetupRevision || view.fleet && view.draft.state !== DRAFT_STATE.editing));
}

/** Only the creator-scoped list may supply conversation history for a shared fleet. */
export async function openFleetAuthoring(base: string, source: Extract<Source, {kind: typeof SOURCE_KIND.fleet}>, signal: AbortSignal): Promise<Partial<DraftView>> {
  const fleet = await setupRequest({path: `${base}/fleets/${source.fleet_id}`}, FleetSchema, signal);
  if (fleet.setup_revision !== source.setup_revision) throw new SetupUnavailable(SETUP_CHANGED, 409);
  if (!fleet.definition) throw new SetupUnavailable("The saved instructions could not be loaded.", 502);
  const preview = {definition: fleet.definition, instanceName: fleet.name, revision: fleet.setup_revision, setupRevision: fleet.setup_revision};
  const activities = await readSetupActivity(base, fleet.id, signal);
  const query = new URLSearchParams({linked_fleet_id: fleet.id, limit: String(RELATED_CHAT_LIMIT)});
  const page = await setupRequest({path: `${base}/drafts?${query}`}, DraftCardsSchema, signal);
  const latest = page.items.find(card => card.linked_fleet_id === fleet.id);
  if (!latest) return {fleet, preview, activities};
  const history = new DraftHistory();
  const {draft, messages} = await history.synchronize(`${base}/drafts/${latest.id}`, signal);
  if (draft.linked_fleet_id !== fleet.id) throw new SetupUnavailable("The setup chat does not belong to this fleet.", 409);
  if (draft.state === DRAFT_STATE.editing && draft.review?.effective.target_setup_revision === fleet.setup_revision) {
    return {fleet, preview, activities, draft, messages, model: draft.authoring_model_ref, truncated: history.truncated};
  }
  if (messages.some(isRunning)) throw new SetupUnavailable("Finish or stop your earlier setup request before opening a new edit.", 409);
  return {fleet, preview, activities, previousDrafts: [{id: draft.id, cancelledThrough: draft.cancelled_through, messages}], truncated: history.truncated};
}

/** A saved draft is immutable; the next turn starts from the fleet's current revision. */
export async function continueSavedDraft(base: string, read: () => DraftView, signal: AbortSignal): Promise<{source: Source; patch: Partial<DraftView>} | undefined> {
  const view = read();
  if (!savedDraft(view)) return;
  const id = view.draft.linked_fleet_id ?? view.fleet?.id;
  if (!id) throw new SetupUnavailable("The saved fleet could not be identified.", 409);
  const fleet = await setupRequest({path: `${base}/fleets/${id}`}, FleetSchema, signal);
  if (!fleet.definition) throw new SetupUnavailable("The saved instructions could not be loaded.", 502);
  const previousDrafts = [...(read().previousDrafts ?? [])];
  if (!previousDrafts.some(previous => previous.id === view.draft.id)) {
    previousDrafts.push({id: view.draft.id, cancelledThrough: view.draft.cancelled_through, messages: view.messages});
  }
  return {source: {kind: SOURCE_KIND.fleet, fleet_id: fleet.id, setup_revision: fleet.setup_revision}, patch: {
    fleet, draft: null, messages: [], previousDrafts, savedSetupRevision: null, optimistic: null, liveReply: null, contextProgress: null,
    preview: {definition: fleet.definition, instanceName: fleet.name, revision: fleet.setup_revision, setupRevision: fleet.setup_revision},
  }};
}

/** Read access receipts without replacing a proposal, or resume the full authoring history. */
export async function readFleetSetup(base: string, source: Source, view: DraftView, signal: AbortSignal, readinessOnly: boolean): Promise<Partial<DraftView>> {
  if (readinessOnly && view.fleet) {
    const fleet = await setupRequest({path: `${base}/fleets/${view.fleet.id}`}, FleetSchema, signal);
    return {fleet, activities: await readSetupActivity(base, fleet.id, signal)};
  }
  if (source.kind !== SOURCE_KIND.fleet) throw new SetupUnavailable("Choose a fleet before opening its setup.", 409);
  return openFleetAuthoring(base, source, signal);
}

/** Restore saved conversations without holding the latest setup or its composer busy. */
export async function restoreFleetHistory(base: string, fleet: Fleet | null, signal: AbortSignal,
  read: () => DraftView, update: (patch: Partial<DraftView>) => void): Promise<void> {
  if (!fleet) return;
  const current = (): DraftView | undefined => !signal.aborted && read().fleet?.id === fleet.id ? read() : undefined;
  const incomplete = (): void => {if (current()) update({truncated: true});};
  try {
    const query = new URLSearchParams({linked_fleet_id: fleet.id, limit: String(RELATED_CHAT_LIMIT)});
    const page = await setupRequest({path: `${base}/drafts?${query}`}, DraftCardsSchema, signal);
    if (!current()) return;
    if (page.next_cursor) incomplete();
    const cards = page.items.filter(card => card.linked_fleet_id === fleet.id && card.state === DRAFT_STATE.closed);
    await Promise.all(Array.from({length: Math.min(HISTORY_CONCURRENCY, cards.length)}, async () => {
      while (cards.length && current()) {
        const card = cards.shift();
        const before = current();
        if (!card || !before || before.draft?.id === card.id || before.previousDrafts?.some(draft => draft.id === card.id)) continue;
        try {
          const history = new DraftHistory();
          const {draft, messages} = await history.synchronize(`${base}/drafts/${card.id}`, signal);
          if (draft.linked_fleet_id !== fleet.id || messages.some(isRunning)) {incomplete(); continue;}
          const view = current();
          if (!view || view.draft?.id === draft.id || view.previousDrafts?.some(previous => previous.id === draft.id)) continue;
          const previousDrafts = [...(view.previousDrafts ?? []), {id: draft.id, cancelledThrough: draft.cancelled_through, messages}]
            .sort((left, right) => left.id.localeCompare(right.id));
          update({previousDrafts, truncated: view.truncated || history.truncated});
        } catch {incomplete();}
      }
    }));
  } catch {incomplete();}
}
