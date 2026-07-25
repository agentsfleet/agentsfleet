"use client";

import type { ReactNode } from "react";
import type { MessageState } from "@assistant-ui/react";
import { readFailureDetail, readFailureLabel, readOutcome } from "./fleetMessageReaders";
import { failureSentenceFor, guidanceFor } from "@/lib/events/event-summary";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-frames";

const STARTUP_FAILURE_TAG = "startup_posture";
const SKILL_VIEW_HREF = "?view=skill";
const CHAT_STARTUP_FAILURE_LABEL = "This fleet needs instructions before it can respond.";
const CHAT_STARTUP_GUIDANCE = "Tell the fleet what to do in its instructions, then retry.";
const SKILL_LINK_LABEL = "Edit instructions";

// Failure copy for the chat surface. The outcome line states the cause, the
// guidance line states the remedy; startup-posture failures get chat-specific
// wording with a link into the skill editor, where the fix actually lives.
// The generic sentences for every other tag come from event-summary.

export function messageOutcome(message: MessageState): string {
  const failureLabel = readFailureLabel(message);
  const failureDetail = readFailureDetail(message);
  const rawOutcome = readOutcome(message);
  if (!failureLabel) return rawOutcome;
  return formatFailureOutcome(chatFailureSentenceFor(failureLabel), rawOutcome, failureDetail);
}

export function eventOutcome(event: FleetEvent): string {
  if (!event.failureLabel) return event.outcome;
  return formatFailureOutcome(
    chatFailureSentenceFor(event.failureLabel),
    event.outcome,
    event.failureDetail,
  );
}

export function failureGuidanceFor(tag: string | null): ReactNode | undefined {
  const guidance = chatGuidanceFor(tag);
  return guidance ? <FailureGuidance guidance={guidance} /> : undefined;
}

function chatFailureSentenceFor(tag: string): string {
  return tag === STARTUP_FAILURE_TAG ? CHAT_STARTUP_FAILURE_LABEL : failureSentenceFor(tag);
}

function chatGuidanceFor(tag: string | null): string | null {
  return tag === STARTUP_FAILURE_TAG ? CHAT_STARTUP_GUIDANCE : guidanceFor(tag);
}

function formatFailureOutcome(sentence: string, rawOutcome: string, detail: string | null): string {
  const embeddedDetail = rawOutcome.split("—").slice(1).join("—").trim();
  const cause = detail ?? (embeddedDetail.length > 0 ? embeddedDetail : null);
  return cause ? `${sentence} — ${cause}` : sentence;
}

function FailureGuidance({ guidance }: { guidance: string }) {
  return (
    <span className="mt-xs block text-label text-muted-foreground" data-testid="failure-guidance">
      {guidance}
      <a href={SKILL_VIEW_HREF} className="ml-sm underline hover:text-foreground">
        {SKILL_LINK_LABEL}
      </a>
    </span>
  );
}
