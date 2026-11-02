"use client";

import type { MessageState } from "@assistant-ui/react";
import { FLEET_OUTCOME_CLASS } from "./FleetFailedOutcome";
import { FleetReplyFailure } from "./FleetReplyFailure";
import { FleetMessageRow, ROW_TONE } from "./FleetMessageRow";
import { OmittedCalls } from "./FleetReplyFigures";
import { FleetReplyParts } from "./FleetReplyParts";
import { FleetReplyActions } from "./FleetReplyActions";
import { useSettleAnnouncement } from "./FleetReplyAnnouncements";
import { messageOutcome } from "./fleetFailureCopy";
import { readOmittedCallCount, readQueued, readReasoningSpan, readReplyFigures, readReplyRecovering, readSubmittedAtMs, readText } from "./fleetMessageReaders";
import { STATUS_AGENT_ERROR } from "./fleetMessageStatus";
import { STATUS_RUNNING } from "./fleetReplyMessage";
import { REPLY_ID_SUFFIX } from "./useFleetThreadEntries";
import { useFirstVisiblePaint } from "./useFirstVisiblePaint";
import { ContextProgressStatus } from "./ContextProgressStatus";
import { readContextProgress } from "@/lib/context-progress";

const RECOVERING_LABEL = "Loading final reply; retrying if needed…";
const DEFAULT_SENDER = "Fleet";

/**
 * The fleet's answer as assistant-ui message parts, in its own row beneath the
 * trigger that woke the fleet. Whether it is still running is the message's
 * status; what it contains is its parts.
 */
export function FleetReply({
  message,
  senderLabel,
  status,
}: {
  message: MessageState;
  senderLabel: string;
  status: string;
}) {
  const errored = status === STATUS_AGENT_ERROR;
  const running = message.status?.type === STATUS_RUNNING;
  const recovering = readReplyRecovering(message);
  const answer = readText(message).trim();
  const span = readReasoningSpan(message);
  const queued = readQueued(message);
  const context = readContextProgress(message.metadata.custom.contextProgress);
  const eventId = message.id.endsWith(REPLY_ID_SUFFIX) ? message.id.slice(0, -REPLY_ID_SUFFIX.length) : message.id;
  useFirstVisiblePaint(eventId, readSubmittedAtMs(message), message.content.length > 0);
  const sender = senderLabel || DEFAULT_SENDER;
  // Spoken the way the row reads: who replied, then what they said.
  useSettleAnnouncement(!running && !recovering, sender, answer.length > 0 ? answer : messageOutcome(message));
  return (
    <FleetMessageRow
      sender={sender}
      tone={ROW_TONE.FLEET}
      messageRole="assistant"
      failed={errored}
    >
      <FleetReplyParts errored={errored} running={running} queued={queued} eventId={eventId}
        reasoning={reasoningText(message)} span={span} content={message.content}
        startedAtMs={message.createdAt.getTime()} contextProgress={context} />
      {running ? <ContextProgressStatus value={context} /> : null}
      <OmittedCalls count={readOmittedCallCount(message)} />
      {errored && !running && !recovering ? <FleetReplyFailure message={message} /> : null}
      {answer.length === 0 && !running && !errored ? <p className={FLEET_OUTCOME_CLASS}>{messageOutcome(message)}</p> : null}
      {recovering ? <output aria-label={RECOVERING_LABEL} className="text-body-sm text-text-subtle">{RECOVERING_LABEL}</output> : null}
      <FleetReplyActions answer={answer} settled={!running && !errored && !recovering} figures={readReplyFigures(message)} />
    </FleetMessageRow>
  );
}

function reasoningText(message: MessageState): string {
  for (const part of message.content) {
    if (part.type === "reasoning") return part.text;
  }
  return "";
}
