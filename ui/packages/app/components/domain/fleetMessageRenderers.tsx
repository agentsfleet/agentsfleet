"use client";

import type { ReactNode } from "react";
import type { MessageState } from "@assistant-ui/react";
import { Badge, cn } from "@agentsfleet/design-system";
import { readTools, ToolCalls } from "./FleetToolCalls";
import {
  FleetMessageRow,
  ROW_TONE,
  useFleetName,
  type RowTone,
} from "./FleetMessageRow";
import { SENDER, roleFor, senderLabelFor } from "@/lib/events/event-summary";

const SENDER_FLEET = SENDER.FLEET_FALLBACK;

const STATUS_OPTIMISTIC = "optimistic";
const STATUS_FAILED = "failed";
const STATUS_AGENT_ERROR = "fleet_error";
// An event the server has accepted but not finished. The cursor reads off our
// own event status rather than the runtime's running flag — that flag means
// "disable the composer" in this library, which is not this surface's
// behaviour, so the thread never sets it.
const STATUS_IN_FLIGHT = "received";

const SENDING_LABEL = "sending";
const FAILED_LABEL = "not sent";
const STREAM_CURSOR = "▍";

const PAYLOAD_SHOW_LABEL = "▸ payload";
const PAYLOAD_HIDE_LABEL = "▾ hide payload";

/**
 * Render function passed to `<ThreadPrimitive.Messages>` in `FleetThread`.
 * Every role renders the same approved row; the role decides the chip tone,
 * the annotation beside the sender, and what rides under the body.
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
  return (
    <>
      {/* The trigger bubble: the operator's message or the integration event.
          An assistant-only row has no trigger and skips straight to the reply. */}
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

// The fleet's answer to a trigger, or — for a completed turn with no answer —
// the honest outcome sentence. Rendered as its own bubble so the reply never
// appears under the operator's identity. Suppressed only for an operator turn
// that is still optimistic or has failed to send (no fleet turn exists yet).
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
        <span className="ml-xs text-pulse animate-pulse" aria-label="streaming">
          {STREAM_CURSOR}
        </span>
      ) : null}
    </FleetMessageRow>
  );
}

// ── Row parts ─────────────────────────────────────────────────────────────

// The delivery state of an operator's own trigger message: sending, or not
// sent. A fleet error is the reply bubble's annotation, not the trigger's.
function Annotation({ optimistic, failed }: { optimistic: boolean; failed: boolean }) {
  if (optimistic) return <Badge variant="evidence">{SENDING_LABEL}</Badge>;
  if (failed) return <Badge variant="destructive">{FAILED_LABEL}</Badge>;
  return null;
}

/**
 * The payload an event arrived with, one click away. Offered for every event
 * that carries one — a platform identity such as `github-app` is as much a
 * webhook as a `webhook:`-prefixed actor, and hiding its payload behind an
 * actor-name prefix was why those rows read as blank.
 */
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

// ── helpers ──────────────────────────────────────────────────────────────

function toneFor(actor: string, status: string): RowTone {
  if (status === STATUS_OPTIMISTIC || status === STATUS_FAILED) return ROW_TONE.OPERATOR;
  // toneFor is only asked for a TRIGGER's tone, and a trigger is never an
  // assistant row (those render straight to the reply bubble). So the actor
  // here is an operator or an integration — no assistant arm.
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

function readRequestJson(message: MessageState): string | null {
  const raw = message.metadata.custom["requestJson"];
  if (typeof raw !== "string") return null;
  const trimmed = raw.trim();
  // An empty object is what a payload-less event stores; offering a disclosure
  // that opens onto `{}` is noise, not evidence.
  return trimmed.length > 0 && trimmed !== "{}" ? trimmed : null;
}
