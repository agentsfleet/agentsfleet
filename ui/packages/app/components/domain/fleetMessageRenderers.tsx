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
const STATUS_IN_FLIGHT = "received";
const SENDING_LABEL = "sending";
const FAILED_LABEL = "not sent";
const STREAM_CURSOR = "▍";
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

function readRequestJson(message: MessageState): string | null {
  const raw = message.metadata.custom["requestJson"];
  if (typeof raw !== "string") return null;
  const trimmed = raw.trim();
  return trimmed.length > 0 && trimmed !== "{}" ? trimmed : null;
}
