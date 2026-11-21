import type { Message } from "./draft-types";
import { UNKNOWN_USAGE_REASON } from "./draft-recovery";

const REASONS = new Map([
  ["insufficient_balance", "More credits needed"],
  ["model_unavailable", "Model unavailable"],
  ["context_too_large", "Conversation too long"],
  ["policy_changed", "Setup changed"],
]);

export function draftFailureLabel(failure: NonNullable<Message["failure"]>): string {
  if (failure.reason === UNKNOWN_USAGE_REASON) return "Usage is still being reconciled.";
  if (failure.cause === "deadline" || failure.reason === "attempt_timed_out") return "Model request timed out";
  if (failure.cause === "cancelled") return "Request stopped";
  return REASONS.get(failure.reason) ?? "Couldn’t complete this request";
}
