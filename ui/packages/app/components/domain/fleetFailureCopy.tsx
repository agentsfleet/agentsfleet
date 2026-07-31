"use client";

import type { MessageState } from "@assistant-ui/react";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-frames";
import {
  readFailureDetail,
  readFailureLabel,
  readOutcome,
} from "./fleetMessageReaders";
import { failureSentenceFor } from "@/lib/events/event-summary";

const STARTUP_FAILURE_TAG = "startup_posture";
const CHAT_STARTUP_FAILURE_LABEL =
  "This fleet needs instructions before it can respond.";
const RUNNER_REFUSAL_SENTENCE =
  "The runner refused this run before the fleet started.";

// Cause lines the runner emits when IT refuses a startup_posture lease before
// the fleet ever runs — mirrored verbatim from the runner's own cause lines,
// the single source for telling a runner-side refusal apart from a fleet with
// no instructions. Matching is exact: any other detail keeps the
// needs-instructions sentence.
export const RUNNER_REFUSAL_DETAILS = [
  "sandbox could not be established on this runner",
  "strict egress policy is not implemented on this runner",
  "the child could not be enrolled in the resource-control domain",
  "sandbox setup aborted before the fleet started",
  "failed to serialize the lease for the child",
] as const;

// Failure copy for the chat surface. Startup-posture failures get concise
// chat-specific wording; every other tag reuses the event-summary sentence.

export function messageOutcome(message: MessageState): string {
  const failureLabel = readFailureLabel(message);
  const failureDetail = readFailureDetail(message);
  const rawOutcome = readOutcome(message);
  if (!failureLabel) return rawOutcome;
  return formatFailureOutcome(failureLabel, rawOutcome, failureDetail);
}

export function eventOutcome(event: FleetEvent): string {
  if (!event.failureLabel) return event.outcome;
  return formatFailureOutcome(
    event.failureLabel,
    event.outcome,
    event.failureDetail,
  );
}

// The sentence depends on the cause, not only the tag: a startup_posture
// failure whose detail is one of the runner's refusal lines was refused by the
// runner, not starved of instructions.
function chatFailureSentenceFor(tag: string, cause: string | null): string {
  if (tag !== STARTUP_FAILURE_TAG) return failureSentenceFor(tag);
  return cause !== null && isRunnerRefusal(cause)
    ? RUNNER_REFUSAL_SENTENCE
    : CHAT_STARTUP_FAILURE_LABEL;
}

function isRunnerRefusal(cause: string): boolean {
  return (RUNNER_REFUSAL_DETAILS as readonly string[]).includes(cause);
}

function formatFailureOutcome(
  tag: string,
  rawOutcome: string,
  detail: string | null,
): string {
  const embeddedDetail = rawOutcome.split("—").slice(1).join("—").trim();
  const cause = detail ?? (embeddedDetail.length > 0 ? embeddedDetail : null);
  const sentence = chatFailureSentenceFor(tag, cause);
  return cause ? `${sentence} — ${cause}` : sentence;
}
