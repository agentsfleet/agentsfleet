"use client";

import { useState, type ReactNode } from "react";
import type { MessageState } from "@assistant-ui/react";
import { Badge, cn } from "@agentsfleet/design-system";
import { readTools, ToolCalls } from "./FleetToolCalls";
import {
  FleetActivityRow,
  FleetGroupRow,
  FleetMessageRow,
  ROW_TONE,
  useFleetName,
} from "./FleetMessageRow";
import {
  readActor,
  readCustomStatus,
  readFailureLabel,
  readGroupMembers,
  readOutcome,
  readReply,
  readRequestJson,
  readText,
} from "./fleetMessageReaders";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-frames";
import { groupSpan } from "@/lib/events/event-grouping";
import {
  SENDER,
  changeProposalActionFrom,
  eventLinkFrom,
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
const OPEN_LINK_LABEL = "open ↗";
const PAYLOAD_SHOW_LABEL = "▸ payload";
const PAYLOAD_HIDE_LABEL = "▾ hide payload";

/**
 * Render function passed to the thread message list. Every role uses the same
 * row skeleton; role changes the chip tone, annotation, and body.
 */
export function renderFleetMessage({ message }: { message: MessageState }): ReactNode {
  return <FleetMessage message={message} />;
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
  // A run of identical deliveries is one row until the operator opens it.
  const group = readGroupMembers(message);
  if (group) return <FleetGroupMessage fleetName={fleetName} members={group} />;
  // Integration deliveries recede to a one-line tick so the operator's own
  // conversation dominates the column (approved variant B). Order is
  // untouched — activity looks quieter, it never moves.
  if (message.role === "system") {
    return <FleetActivityMessage message={message} fleetName={fleetName} tools={tools} />;
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
          {payload ? <PayloadDisclosure json={payload} /> : null}
        </FleetMessageRow>
      )}
      <FleetReply message={message} fleetName={fleetName} tools={tools} status={status} />
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
  tools,
}: {
  message: MessageState;
  fleetName: string;
  tools: ReturnType<typeof readTools>;
}) {
  const status = readCustomStatus(message);
  const reply = readReply(message);
  const payload = readRequestJson(message);
  const working = status === STATUS_IN_FLIGHT;
  const errored = status === STATUS_AGENT_ERROR;
  // The tick states the outcome itself — a delivery whose only content is its
  // outcome does not earn a second row. A real reply does.
  const outcome = working || reply.length > 0 ? undefined : readOutcome(message);
  const guidance = reply.length > 0 ? null : guidanceFor(readFailureLabel(message));
  const action = changeProposalActionFrom(payload);
  const link = eventLinkFrom(payload);
  return (
    <>
      <FleetActivityRow
        sender={senderLabelFor(readActor(message), fleetName)}
        createdAt={message.createdAt}
        headline={readText(message)}
        outcome={outcome}
        failed={errored}
        messageRole={message.role}
        annotation={<ActivityAnnotation action={action} link={link} />}
      >
        {guidance ? (
          <span className="mt-xs block text-label text-muted-foreground" data-testid="failure-guidance">
            {guidance}
          </span>
        ) : null}
        {payload ? <PayloadDisclosure json={payload} /> : null}
      </FleetActivityRow>
      {reply.length > 0 ? (
        <FleetReply message={message} fleetName={fleetName} tools={tools} status={status} />
      ) : null}
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
      outcome={newest.reply.length > 0 ? undefined : newest.outcome}
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
          outcome={member.reply.length > 0 ? member.reply : member.outcome}
          failed={member.status === STATUS_AGENT_ERROR}
          messageRole="system"
        >
          {member.custom?.requestJson ? <PayloadDisclosure json={member.custom.requestJson} /> : null}
        </FleetActivityRow>
      ))}
    </FleetGroupRow>
  );
}

// The action verb as a `Badge`, and the provider's own link when the payload
// carries one. A payload with neither renders nothing rather than a dead
// affordance.
function ActivityAnnotation({ action, link }: { action: string; link: string | null }) {
  if (action.length === 0 && link === null) return null;
  return (
    <>
      {action.length > 0 ? <Badge variant="evidence">{action}</Badge> : null}
      {link ? (
        <a
          href={link}
          target="_blank"
          rel="noreferrer noopener"
          className="shrink-0 text-label text-muted-foreground underline hover:text-foreground"
        >
          {OPEN_LINK_LABEL}
        </a>
      ) : null}
    </>
  );
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
  const outcome = readOutcome(message);
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
  const guidance = reply.length > 0 ? null : guidanceFor(readFailureLabel(message));
  return (
    <FleetMessageRow
      sender={fleetName.length > 0 ? fleetName : SENDER_FLEET}
      createdAt={message.createdAt}
      tone={ROW_TONE.FLEET}
      messageRole="assistant"
      annotation={errored ? <Badge variant="destructive">{STATUS_AGENT_ERROR}</Badge> : null}
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
      {guidance ? (
        <span className="mt-xs block text-label text-muted-foreground" data-testid="failure-guidance">
          {guidance}
        </span>
      ) : null}
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
function PayloadDisclosure({ json }: { json: string }) {
  return (
    <details className="group mt-md">
      <summary
        className={cn(
          "cursor-pointer list-none font-mono text-label text-muted-foreground",
          "hover:text-foreground",
          "[&::-webkit-details-marker]:hidden",
        )}
      >
        <span className="group-open:hidden">{PAYLOAD_SHOW_LABEL}</span>
        <span className="hidden group-open:inline">{PAYLOAD_HIDE_LABEL}</span>
      </summary>
      <pre
        className={cn(
          "mt-xs max-h-64 overflow-auto rounded-sm border border-border",
          "bg-surface-deep p-lg",
          "font-mono text-mono leading-mono text-foreground",
        )}
      >
        {json}
      </pre>
    </details>
  );
}
