import type { Draft, Review } from "./draft-types";
import { SetupUnavailable } from "./draft-request";

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
