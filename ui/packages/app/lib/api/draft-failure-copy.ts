import type { Message } from "./draft-types";
import { UNKNOWN_USAGE_REASON } from "./draft-recovery";

const REASONS = new Map([
  ["insufficient_balance", "More credits needed"],
  ["model_unavailable", "Model unavailable"],
  ["frame_too_large", "Message too large"],
]);

export function draftFailureLabel(failure: NonNullable<Message["failure"]>): string {
  if (failure.reason === UNKNOWN_USAGE_REASON) return "Usage is still being reconciled.";
  return REASONS.get(failure.reason) ?? "Couldn’t complete this request";
}
