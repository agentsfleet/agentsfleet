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
  type RowTone,
} from "./FleetMessageRow";
import { GROUP_META } from "./useFleetThreadEntries";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-frames";
import {
  SENDER,
  changeProposalActionFrom,
  eventLinkFrom,
  guidanceFor,
  roleFor,
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
  if (group) return <FleetGroupMessage message={message} fleetName={fleetName} members={group} />;
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
          tone={toneFor(actor, status)}
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
  message,
  fleetName,
  members,
}: {
  message: MessageState;
  fleetName: string;
  members: FleetEvent[];
}) {
  const [expanded, setExpanded] = useState(false);
  const newest = members[members.length - 1];
  const range = readGroupRange(message);
  if (newest === undefined || range === null) return null;
  const failed = newest.status === STATUS_AGENT_ERROR;
  return (
    <FleetGroupRow
      sender={senderLabelFor(newest.actor, fleetName)}
      headline={newest.text}
      outcome={newest.reply.length > 0 ? undefined : newest.outcome}
      failed={failed}
      count={members.length}
      first={range.first}
      last={range.last}
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
      <span className={cn(errored && "text-destructive")}>{body}</span>
      {streaming ? (
        <span className="ml-xs animate-pulse text-pulse" aria-label="streaming">
          {STREAM_CURSOR}
        </span>
      ) : null}
      {guidance ? (
        <span className="mt-xs block text-label text-muted-foreground" data-testid="failure-guidance">
          {guidance}
        </span>
      ) : null}
    </FleetMessageRow>
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

function toneFor(actor: string, status: string): RowTone {
  if (status === STATUS_OPTIMISTIC || status === STATUS_FAILED) return ROW_TONE.OPERATOR;
  return roleFor(actor) === "user" ? ROW_TONE.OPERATOR : ROW_TONE.EVENT;
}

function readText(message: MessageState): string {
  for (const part of message.content) {
    if (part.type === "text") return part.text;
  }
  return "";
}

function readActor(message: MessageState): string {
  const raw = message.metadata.custom["actor"];
  return typeof raw === "string" ? raw : "";
}

function readCustomStatus(message: MessageState): string {
  const raw = message.metadata.custom["status"];
  return typeof raw === "string" ? raw : "";
}

function readReply(message: MessageState): string {
  const raw = message.metadata.custom["reply"];
  return typeof raw === "string" ? raw : "";
}

function readOutcome(message: MessageState): string {
  const raw = message.metadata.custom["outcome"];
  return typeof raw === "string" ? raw : "";
}

// Present only on a grouped message; null means this row stands for itself.
function readGroupMembers(message: MessageState): FleetEvent[] | null {
  const raw = message.metadata.custom[GROUP_META.MEMBERS];
  return Array.isArray(raw) && raw.length > 0 ? (raw as FleetEvent[]) : null;
}

function readGroupRange(message: MessageState): { first: Date; last: Date } | null {
  const first = message.metadata.custom[GROUP_META.FIRST_AT];
  const last = message.metadata.custom[GROUP_META.LAST_AT];
  if (!(first instanceof Date) || !(last instanceof Date)) return null;
  return { first, last };
}

function readFailureLabel(message: MessageState): string | null {
  const raw = message.metadata.custom["failureLabel"];
  return typeof raw === "string" && raw.length > 0 ? raw : null;
}

function readRequestJson(message: MessageState): string | null {
  const raw = message.metadata.custom["requestJson"];
  if (typeof raw !== "string") return null;
  const trimmed = raw.trim();
  return trimmed.length > 0 && trimmed !== "{}" ? trimmed : null;
}
