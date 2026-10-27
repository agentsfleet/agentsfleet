import type { Draft, Review, Source } from "./draft-types";
import { setupRequest, SetupUnavailable } from "./draft-request";
import type { DrawerEdit, DraftView } from "./draft-state";
import { sourcePreview } from "./draft-source";
import { FleetSchema } from "./draft-types";

/** A save must use the proposal the person reviewed, including its digest. */
export async function readySetup(displayed: Draft, read: () => Promise<Draft>, target: (draft: Draft, review: Review) => Promise<void>) {
  if (!displayed.candidate || !displayed.review) throw new SetupUnavailable("Continue chatting until a complete setup is ready.", 409);
  const draft = await read();
  if (!draft.candidate || !draft.review || draft.candidate.id !== displayed.candidate.id || draft.review.review_digest !== displayed.review.review_digest) {
    throw new SetupUnavailable("The setup changed. Check the updated instructions, then try again.", 409);
  }
  await target(draft, draft.review);
  return {draft: {...draft, candidate: draft.candidate}, review: draft.review};
}

export async function targetFleetSetup(base: string, draft: Draft, review: Review, signal: AbortSignal) {
  if (!draft.linked_fleet_id || review.effective.target_setup_revision === undefined) return;
  const fleet = await setupRequest({path: `${base}/fleets/${draft.linked_fleet_id}`}, FleetSchema, signal);
  if (fleet.setup_revision !== review.effective.target_setup_revision) throw new SetupUnavailable("This fleet changed. Open Edit setup from its current setup and try again.", 409);
  return fleet;
}

/** Refresh a source preview independently of any unfinished authoring conversation. */
export async function editableSetup(base: string, source: Source, displayed: Draft | null, signal: AbortSignal,
  read: () => Promise<Draft>, target: (draft: Draft, review: Review) => Promise<void>): Promise<{preview: DraftView["preview"]; edit: DrawerEdit}> {
  if (!displayed) {
    const preview = await sourcePreview(base, source, signal);
    if (!preview) throw new SetupUnavailable("There is no setup to refresh yet.", 409);
    return {preview, edit: preview};
  }
  const draft = await read();
  if (!draft.review) throw new SetupUnavailable("Continue chatting or edit instructions before saving setup.", 409);
  await target(draft, draft.review);
  return {preview: null, edit: {definition: draft.review.effective.definition, instanceName: draft.review.effective.instance_name}};
}
