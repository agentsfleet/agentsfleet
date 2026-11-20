import type { Message } from "./draft-types";
import { UNKNOWN_USAGE_REASON } from "./draft-recovery";

const REASONS: Readonly<Record<string, {title: string; description: string}>> = {
  insufficient_balance: {title: "More credits needed", description: "There aren’t enough credits to finish this request. Add credits, then try again."},
  model_unavailable: {title: "Model unavailable", description: "This model isn’t available right now. Choose another model, then try again."},
  frame_too_large: {title: "Message too large", description: "This message is too large for a run to carry. Shorten it or start a new setup, then try again."},
};

export function draftFailureCopy(failure: NonNullable<Message["failure"]>) {
  const copy = REASONS[failure.reason] ?? {title: "Couldn’t complete this request", description: "The model didn’t finish its response. You can try again."};
  return {...copy, note: failure.reason === UNKNOWN_USAGE_REASON ? "Usage is still being reconciled." : undefined};
}
