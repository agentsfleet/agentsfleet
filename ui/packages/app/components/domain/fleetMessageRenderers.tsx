"use client";

import { useState, type ReactNode } from "react";
import { MessagePrimitive, type MessageState } from "@assistant-ui/react";
import { Badge } from "@agentsfleet/design-system";
import { GitPullRequestIcon } from "lucide-react";
import { readTools, ToolCalls } from "./FleetToolCalls";
import {
  FleetActivityRow,
  FleetGroupRow,
  FleetMessageRow,
  ROW_TONE,
  useFleetName,
} from "./FleetMessageRow";
import { FleetPayloadDisclosure } from "./FleetPayloadDisclosure";
import { eventOutcome, messageOutcome } from "./fleetFailureCopy";
import {
  readActor,
  readCustomStatus,
  readGroupMembers,
  readReply,
  readRenderKind,
  readRequestJson,
  readText,
} from "./fleetMessageReaders";
import { RENDER_KIND } from "./useFleetThreadEntries";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-frames";
import { groupSpan } from "@/lib/events/event-grouping";
import {
  eventLinkFrom,
  eventReferenceFrom,
  senderLabelFor,
} from "@/lib/events/event-summary";

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

/**
 * Render function passed to the thread message list. Integration activity
 * renders as flat activity traces; operator turns use a right-side surface while
 * fleet replies stay open on the left (FleetMessageRow).
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
  const sender = senderLabelFor(readActor(message), fleetName);
  const status = readCustomStatus(message);
  const optimistic = status === STATUS_OPTIMISTIC;
  const failed = status === STATUS_FAILED;
  const tools = readTools(message);
  const trigger = readText(message);
  const isReplyRow = message.role === "assistant";
  const isSplitTrigger = readRenderKind(message) === RENDER_KIND.TRIGGER;
  // A run of identical deliveries is one row until the operator opens it.
  const group = readGroupMembers(message);
  if (group) return <FleetGroupMessage fleetName={fleetName} members={group} />;
  // Integration deliveries recede to a flat trace so the operator's own
  // conversation dominates the column. Order is untouched.
  if (message.role === "system") {
    return <FleetActivityMessage message={message} fleetName={fleetName} />;
  }
  return (
    <>
      {isReplyRow ? null : (
        <FleetMessageRow
          sender={sender}
          tone={ROW_TONE.OPERATOR}
          messageRole={message.role}
          dimmed={optimistic}
          failed={failed}
          annotation={<Annotation optimistic={optimistic} failed={failed} />}
        >
          <span>{trigger}</span>
        </FleetMessageRow>
      )}
      {isSplitTrigger ? null : (
        <FleetReply
          message={message}
          fleetName={fleetName}
          tools={tools}
          status={status}
        />
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
  const outcome =
    working || reply.length > 0 || isSplitTrigger ? undefined : messageOutcome(message);
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
        messageRole={message.role}
        annotation={
          <ActivityAnnotation
            link={link}
            label={reference ?? SOURCE_LINK_FALLBACK}
            repositoryReference={reference !== null}
          />
        }
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
function FleetGroupMessage({ fleetName, members }: { fleetName: string; members: FleetEvent[] }) {
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
      last={span.last}
      expanded={expanded}
      onToggle={() => setExpanded((open) => !open)}
    >
      {expanded
        ? members.map((member) => (
            <FleetActivityRow
              key={member.id}
              sender={senderLabelFor(member.actor, fleetName)}
              createdAt={member.createdAt}
              headline={member.text}
              outcome={eventOutcome(member)}
              failed={member.status === STATUS_AGENT_ERROR}
              messageRole="system"
            >
              {member.custom?.requestJson ? (
                <FleetPayloadDisclosure json={member.custom.requestJson} inline />
              ) : null}
            </FleetActivityRow>
          ))
        : null}
    </FleetGroupRow>
  );
}

// The provider's source reference is the link label. Unknown payload shapes
// still get a plain source action rather than an internal "open" affordance.
function ActivityAnnotation({
  link,
  label,
  repositoryReference,
}: {
  link: string | null;
  label: string;
  repositoryReference: boolean;
}) {
  if (link === null) return null;
  return (
    <a
      href={link}
      target="_blank"
      rel="noreferrer noopener"
      className="inline-flex min-h-11 shrink-0 items-center gap-xs rounded-sm text-label font-medium text-foreground underline decoration-border-strong underline-offset-2 hover:decoration-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring sm:min-h-6"
    >
      {repositoryReference ? (
        <GitPullRequestIcon size={14} aria-hidden="true" />
      ) : null}
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
  return (
    <FleetMessageRow
      sender={fleetName || "Fleet"}
      tone={ROW_TONE.FLEET}
      messageRole="assistant"
      failed={errored}
    >
      <ToolCalls tools={tools} />
      {awaitingFirstWord ? (
        <WorkingIndicator />
      ) : (
        <>
          <span
            className={errored ? "text-label font-medium leading-label text-foreground" : undefined}
          >
            {body}
          </span>
          {streaming ? (
            <span className="ml-xs animate-pulse text-pulse" aria-label="streaming">
              {STREAM_CURSOR}
            </span>
          ) : null}
        </>
      )}
    </FleetMessageRow>
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
