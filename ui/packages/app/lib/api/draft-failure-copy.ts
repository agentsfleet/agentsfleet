import type { Message } from "./draft-types";
import { UNKNOWN_USAGE_REASON } from "./draft-recovery";

const REASONS: Readonly<Record<string, {title: string; description: string}>> = {
  insufficient_balance: {title: "More credits needed", description: "There aren’t enough credits to finish this request. Add credits, then try again."},
  model_unavailable: {title: "Model unavailable", description: "This model isn’t available right now. Choose another model, then try again."},
  context_too_large: {title: "Conversation too long", description: "This conversation is too long for the selected model. Choose a model with more room, or start a new setup."},
  policy_changed: {title: "Setup changed", description: "The setup or its permissions changed during this request. Review the setup before trying again."},
};

export function draftFailureCopy(failure: NonNullable<Message["failure"]>) {
  const timedOut = failure.cause === "deadline" || failure.reason === "attempt_timed_out";
  const copy = timedOut
    ? {title: "Model request timed out", description: "The model didn’t finish in time, so there’s no complete response. This request has ended."}
    : failure.cause === "cancelled"
      ? {title: "Request stopped", description: "This request was stopped before the model finished. You can try again when you’re ready."}
      : REASONS[failure.reason] ?? {title: "Couldn’t complete this request", description: "The model didn’t finish its response. You can try again."};
  return {...copy, note: failure.reason === UNKNOWN_USAGE_REASON ? "Usage is still being reconciled." : undefined};
}
