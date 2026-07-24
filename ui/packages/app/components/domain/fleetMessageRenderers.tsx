"use client";

import { useState, type ReactNode } from "react";
import { MessagePrimitive, type MessageState } from "@assistant-ui/react";
import { Badge, cn } from "@agentsfleet/design-system";
import { readTools, ToolCalls } from "./FleetToolCalls";
import {
  FleetActivityRow,
  FleetGroupRow,
  FleetMessageRow,
  ROW_TONE,
  useFleetName,
} from "./FleetMessageRow";
import { FleetPayloadDisclosure } from "./FleetPayloadDisclosure";
import {
  readActor,
  readCustomStatus,
  readFailureLabel,
  readFailureDetail,
  readGroupMembers,
  readOutcome,
  readReply,
  readRenderKind,
  readRequestJson,
  readText,
} from "./fleetMessageReaders";
import { RENDER_KIND } from "./useFleetThreadEntries";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-frames";
import { groupSpan } from "@/lib/events/event-grouping";
import {
  SENDER,
  eventLinkFrom,
  eventReferenceFrom,
  failureSentenceFor,
  guidanceFor,
  senderLabelFor,
} from "@/lib/events/event-summary";

const SENDER_FLEET = SENDER.FLEET_FALLBACK;
const STATUS_OPTIMISTIC = "optimistic";
const STATUS_FAILED = "failed";
const STATUS_AGENT_ERROR = "fleet_error";
const STATUS_IN_FLIGHT = "received";
const SENDING_LABEL = "sending";
const FAILED_LABEL = "not sent";
const STREAM_CURSOR = "▍";
const WORKING_LABEL = "Working";
// Staggered so the three dots read as one travelling wave rather than three
// lights blinking in unison.
const WORKING_DOT_DELAYS = ["0ms", "160ms", "320ms"] as const;
const SOURCE_LINK_FALLBACK = "View source";
const STARTUP_FAILURE_TAG = "startup_posture";
const SKILL_VIEW_HREF = "?view=skill";
const CHAT_STARTUP_FAILURE_LABEL = "This fleet needs instructions before it can respond.";
const CHAT_STARTUP_GUIDANCE = "Tell the fleet what to do in its instructions, then retry.";
const SKILL_LINK_LABEL = "Edit instructions";

/**
 * Render function passed to the thread message list. Every role uses the same
 * row skeleton; role changes the chip tone, annotation, and body.
 */
export function renderFleetMessage({ message }: { message: MessageState }): ReactNode {
  return (
    <MessagePrimitive.Root className="w-full" data-testid="fleet-message">
      <FleetMessage message={message} />
    </MessagePrimitive.Root>
  );
}

function FleetMessage({ message }: { message: MessageState }) {
  const fleetName = useFleetName();
  const actor = readActor(message);
  const status = readCustomStatus(message);
  const optimistic = status === STATUS_OPTIMISTIC;
  const failed = status === STATUS_FAILED;
  const payload = readRequestJson(message);
  const tools = readTools(message);
  const trigger = readText(message);
  const isReplyRow = message.role === "assistant";
  const isSplitTrigger = readRenderKind(message) === RENDER_KIND.TRIGGER;
  // A run of identical deliveries is one row until the operator opens it.
  const group = readGroupMembers(message);
  if (group) return <FleetGroupMessage fleetName={fleetName} members={group} />;
  // Integration deliveries recede to a one-line tick so the operator's own
  // conversation dominates the column (approved variant B). Order is
  // untouched — activity looks quieter, it never moves.
  if (message.role === "system") {
    return <FleetActivityMessage message={message} fleetName={fleetName} />;
  }
  return (
    <>
      {isReplyRow ? null : (
        <FleetMessageRow
          sender={senderLabelFor(actor, fleetName)}
          createdAt={message.createdAt}
          // The only message that still renders a full trigger row is the
          // operator's own — system activity is a compact tick with its own
          // chip — so this row is always operator-toned.
          tone={ROW_TONE.OPERATOR}
          messageRole={message.role}
          dimmed={optimistic}
          failed={failed}
          annotation={<Annotation optimistic={optimistic} failed={failed} />}
        >
          <span>{trigger}</span>
          {payload ? <FleetPayloadDisclosure json={payload} /> : null}
        </FleetMessageRow>
      )}
      {isSplitTrigger ? null : (
        <FleetReply message={message} fleetName={fleetName} tools={tools} status={status} />
      )}
    </>
  );
}

/**
 * One integration delivery as a compact rail tick. When the fleet actually
 * answered, its reply still gets its own full conversation row underneath —
 * the tick demotes the TRIGGER, never the fleet's words.
 */
function FleetActivityMessage({
  message,
  fleetName,
}: {
  message: MessageState;
  fleetName: string;
}) {
  const status = readCustomStatus(message);
  const reply = readReply(message);
  const payload = readRequestJson(message);
  const working = status === STATUS_IN_FLIGHT;
  const errored = status === STATUS_AGENT_ERROR;
  const isSplitTrigger = readRenderKind(message) === RENDER_KIND.TRIGGER;
  // The tick states the outcome itself — a delivery whose only content is its
  // outcome does not earn a second row. A real reply does.
  const outcome = working || reply.length > 0 || isSplitTrigger ? undefined : messageOutcome(message);
  const link = eventLinkFrom(payload);
  const reference = link ? eventReferenceFrom(payload) : null;
  return (
    <>
      <FleetActivityRow
        sender={senderLabelFor(readActor(message), fleetName)}
        createdAt={message.createdAt}
        headline={activityHeadline(readText(message), reference)}
        outcome={outcome}
        failed={errored}
        guidance={
          isSplitTrigger
            ? undefined
            : failureGuidanceFor(readFailureLabel(message))
        }
        messageRole={message.role}
        annotation={<ActivityAnnotation link={link} label={reference ?? SOURCE_LINK_FALLBACK} />}
      >
        {payload ? <FleetPayloadDisclosure json={payload} inline /> : null}
      </FleetActivityRow>
    </>
  );
}

/**
 * A run of identical deliveries as one "×N" row. Collapsed by default; opening
 * it renders every member as its own tick, so the count is always a summary
 * the operator can check rather than a claim they have to trust.
 */
function FleetGroupMessage({
  fleetName,
  members,
}: {
  fleetName: string;
  members: FleetEvent[];
}) {
  const [expanded, setExpanded] = useState(false);
  // Everything is derived from `members` (guaranteed non-empty by the caller):
  // `reduce` yields the newest as a definite `FleetEvent`, and the span reads
  // the members' timestamps. Nothing is re-read from the message metadata, so
  // there is no "missing metadata" branch to leave uncovered.
  const newest = members.reduce((_, member) => member);
  const span = groupSpan(members);
  const failed = newest.status === STATUS_AGENT_ERROR;
  return (
    <FleetGroupRow
      sender={senderLabelFor(newest.actor, fleetName)}
      headline={newest.text}
      outcome={eventOutcome(newest)}
      failed={failed}
      count={members.length}
      first={span.first}
      last={span.last}
      expanded={expanded}
      onToggle={() => setExpanded((open) => !open)}
    >
      {members.map((member) => (
        <FleetActivityRow
          key={member.id}
          sender={senderLabelFor(member.actor, fleetName)}
          createdAt={member.createdAt}
          headline={member.text}
          outcome={eventOutcome(member)}
          failed={member.status === STATUS_AGENT_ERROR}
          guidance={failureGuidanceFor(member.failureLabel)}
          messageRole="system"
        >
          {member.custom?.requestJson ? (
            <FleetPayloadDisclosure json={member.custom.requestJson} inline />
          ) : null}
        </FleetActivityRow>
      ))}
    </FleetGroupRow>
  );
}

// The provider's source reference is the link label. Unknown payload shapes
// still get a plain source action rather than an internal "open" affordance.
function ActivityAnnotation({ link, label }: { link: string | null; label: string }) {
  if (link === null) return null;
  return (
    <a
      href={link}
      target="_blank"
      rel="noreferrer noopener"
      className="shrink-0 text-label text-muted-foreground underline hover:text-foreground"
    >
      {label}
    </a>
  );
}

function activityHeadline(headline: string, reference: string | null): string {
  if (!reference) return headline;
  const referenceIndex = headline.indexOf(reference);
  if (referenceIndex < 0) return headline;
  const prefix = headline.slice(0, referenceIndex).trimEnd();
  const suffix = headline.slice(referenceIndex + reference.length).trimStart();
  if (suffix.startsWith("—")) {
    if (!prefix) return suffix.slice(1).trimStart();
    return `${prefix.endsWith("·") ? prefix : `${prefix} ·`} ${suffix.slice(1).trimStart()}`;
  }
  if (suffix.length === 0 && prefix.endsWith("·")) return prefix.slice(0, -1).trimEnd();
  return `${prefix}${suffix.length > 0 ? ` ${suffix}` : ""}`.trim();
}

// A trigger and its fleet answer are separate rows so a reply never appears
// beneath the operator or integration identity that woke the fleet.
function FleetReply({
  message,
  fleetName,
  tools,
  status,
}: {
  message: MessageState;
  fleetName: string;
  tools: ReturnType<typeof readTools>;
  status: string;
}) {
  const reply = readReply(message);
  const outcome = messageOutcome(message);
  const errored = status === STATUS_AGENT_ERROR;
  const streaming = status === STATUS_IN_FLIGHT;
  if (status === STATUS_OPTIMISTIC || status === STATUS_FAILED) return null;
  // A turn that has started but said nothing yet gets motion, not a sentence.
  // "Still working." is true and completely inert — it reads the same at one
  // second and at five minutes, so the operator cannot tell the fleet is alive.
  const awaitingFirstWord = streaming && reply.length === 0;
  const body = reply.length > 0 ? reply : outcome;
  // The cause says what broke; the guidance says what to do about it. Only
  // rendered when the failure sentence is what the operator is reading — a
  // recorded reply is the fleet's own words and takes precedence.
  const guidance = reply.length > 0 ? null : failureGuidanceFor(readFailureLabel(message));
  return (
    <FleetMessageRow
      sender={fleetName.length > 0 ? fleetName : SENDER_FLEET}
      createdAt={message.createdAt}
      tone={ROW_TONE.FLEET}
      messageRole="assistant"
    >
      <ToolCalls tools={tools} />
      {awaitingFirstWord ? (
        <WorkingIndicator />
      ) : (
        <>
          <span className={cn(errored && "text-destructive")}>{body}</span>
          {streaming ? (
            <span className="ml-xs animate-pulse text-pulse" aria-label="streaming">
              {STREAM_CURSOR}
            </span>
          ) : null}
        </>
      )}
      {guidance}
    </FleetMessageRow>
  );
}

function messageOutcome(message: MessageState): string {
  const failureLabel = readFailureLabel(message);
  const failureDetail = readFailureDetail(message);
  const rawOutcome = readOutcome(message);
  if (!failureLabel) return rawOutcome;
  return formatFailureOutcome(chatFailureSentenceFor(failureLabel), rawOutcome, failureDetail);
}

function eventOutcome(event: FleetEvent): string {
  if (!event.failureLabel) return event.outcome;
  return formatFailureOutcome(
    chatFailureSentenceFor(event.failureLabel),
    event.outcome,
    event.failureDetail,
  );
}

function chatFailureSentenceFor(tag: string): string {
  return tag === STARTUP_FAILURE_TAG ? CHAT_STARTUP_FAILURE_LABEL : failureSentenceFor(tag);
}

function chatGuidanceFor(tag: string | null): string | null {
  return tag === STARTUP_FAILURE_TAG ? CHAT_STARTUP_GUIDANCE : guidanceFor(tag);
}

function failureGuidanceFor(tag: string | null): ReactNode | undefined {
  const guidance = chatGuidanceFor(tag);
  return guidance ? <FailureGuidance guidance={guidance} /> : undefined;
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

// Three dots, staggered, under one live region so a screen reader is told
// once that the fleet is working rather than on every animation frame.
function WorkingIndicator() {
  return (
    <output
      className="inline-flex items-baseline gap-xs"
      aria-label={WORKING_LABEL}
      data-testid="fleet-working"
    >
      {WORKING_DOT_DELAYS.map((delay) => (
        <span
          key={delay}
          aria-hidden="true"
          className="inline-block size-1 rounded-full bg-pulse motion-safe:animate-pulse"
          style={{ animationDelay: delay }}
        />
      ))}
    </output>
  );
}

function Annotation({ optimistic, failed }: { optimistic: boolean; failed: boolean }) {
  if (optimistic) return <Badge variant="evidence">{SENDING_LABEL}</Badge>;
  if (failed) return <Badge variant="destructive">{FAILED_LABEL}</Badge>;
  return null;
}

// Any integration event with a stored payload can reveal it. Restricting this
// to one actor prefix previously left platform integrations looking blank.
