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

// Failure copy for the chat surface. Startup-posture failures get concise
// chat-specific wording; every other tag reuses the event-summary sentence.

export function messageOutcome(message: MessageState): string {
  const failureLabel = readFailureLabel(message);
  const failureDetail = readFailureDetail(message);
  const rawOutcome = readOutcome(message);
  if (!failureLabel) return rawOutcome;
  return formatFailureOutcome(
    chatFailureSentenceFor(failureLabel),
    rawOutcome,
    failureDetail,
  );
}

export function eventOutcome(event: FleetEvent): string {
  if (!event.failureLabel) return event.outcome;
  return formatFailureOutcome(
    chatFailureSentenceFor(event.failureLabel),
    event.outcome,
    event.failureDetail,
  );
}

function chatFailureSentenceFor(tag: string): string {
  return tag === STARTUP_FAILURE_TAG
    ? CHAT_STARTUP_FAILURE_LABEL
    : failureSentenceFor(tag);
}

function formatFailureOutcome(
  sentence: string,
  rawOutcome: string,
  detail: string | null,
): string {
  const embeddedDetail = rawOutcome.split("—").slice(1).join("—").trim();
  const cause = detail ?? (embeddedDetail.length > 0 ? embeddedDetail : null);
  return cause ? `${sentence} — ${cause}` : sentence;
}
