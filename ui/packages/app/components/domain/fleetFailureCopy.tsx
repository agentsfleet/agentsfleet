"use client";

import type { MessageState } from "@assistant-ui/react";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import {
  readFailureDetail,
  readFailureLabel,
  readOutcome,
} from "./fleetMessageReaders";
import {
  failureSentenceFor,
  outcomeFor,
  RUNNER_REFUSAL_SENTENCE,
} from "@/lib/events/event-summary";

// The runner is the only writer of startup_posture: it ends a lease under that
// class when it refuses the run before the fleet starts, and its cause line
// says why. So every startup failure reads as the runner's refusal.
const STARTUP_FAILURE_TAG = "startup_posture";

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

function chatFailureSentenceFor(tag: string): string {
  return tag === STARTUP_FAILURE_TAG
    ? RUNNER_REFUSAL_SENTENCE
    : failureSentenceFor(tag);
}

// Cause lines written for operators are prose and always contain whitespace
// ("this host could not build a sandbox for the run"). A single bare token —
// `FleetInitFailed` — is an internal error name that leaked through a layer
// which should have mapped it, and it is never shown to the user.
function isInternalIdentifier(cause: string): boolean {
  return !/\s/.test(cause);
}

function formatFailureOutcome(
  tag: string,
  rawOutcome: string,
  detail: string | null,
): string {
  const embeddedDetail = rawOutcome.split("—").slice(1).join("—").trim();
  const cause = detail ?? (embeddedDetail.length > 0 ? embeddedDetail : null);
  if (tag === "runner_crash") {
    return outcomeFor({ status: "fleet_error", failure_label: tag, failure_detail: cause });
  }
  const sentence = chatFailureSentenceFor(tag);
  // Only prose is shown after the sentence: an internal identifier appended
  // after an em-dash reads as diagnostic detail while telling the user nothing.
  const shown = cause !== null && !isInternalIdentifier(cause) ? cause : null;
  return shown ? `${sentence} — ${shown}` : sentence;
}
