"use client";

import { useState } from "react";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@agentsfleet/design-system";
import type { MessageState } from "@assistant-ui/react";

import { FleetMarkdown } from "./FleetMarkdown";
import { FleetMessageRow, ROW_TONE } from "./FleetMessageRow";
import { ToolCalls, type readTools } from "./FleetToolCalls";
import { messageOutcome } from "./fleetFailureCopy";
import { readReply } from "./fleetMessageReaders";
import {
  STATUS_AGENT_ERROR,
  STATUS_FAILED,
  STATUS_IN_FLIGHT,
  STATUS_OPTIMISTIC,
} from "./fleetMessageStatus";
import { splitReasoning } from "@/lib/events/reasoning";

const STREAM_CURSOR = "▍";
const WORKING_LABEL = "Working";
// Staggered so the three dots read as one travelling wave rather than three
// lights blinking in unison.
const WORKING_DOT_DELAYS = ["0ms", "160ms", "320ms"] as const;
const REASONING_VALUE = "reasoning";
const REASONING_LABEL = "Reasoning";
const REASONING_LIVE_LABEL = "Thinking…";

/**
 * A trigger and its fleet answer are separate rows, so a reply never appears
 * beneath the operator or integration identity that woke the fleet.
 */
export function FleetReply({
  message,
  senderLabel,
  tools,
  status,
}: {
  message: MessageState;
  senderLabel: string;
  tools: ReturnType<typeof readTools>;
  status: string;
}) {
  const reply = readReply(message);
  const errored = status === STATUS_AGENT_ERROR;
  const streaming = status === STATUS_IN_FLIGHT;
  if (status === STATUS_OPTIMISTIC || status === STATUS_FAILED) return null;
  // Models that reason out loud wrap it in `<think>`. The durable row keeps
  // only the answer, so leaving the raw text in place made a turn read one way
  // live and another way after a navigation.
  const { reasoning, answer, thinking } = splitReasoning(reply);
  // A turn that has started but said nothing yet gets motion, not a sentence.
  // "Still working." is true and completely inert — it reads the same at one
  // second and at five minutes, so the operator cannot tell the fleet is alive.
  const awaitingFirstWord = streaming && reply.length === 0;
  return (
    <FleetMessageRow
      sender={senderLabel || "Fleet"}
      tone={ROW_TONE.FLEET}
      messageRole="assistant"
      failed={errored}
    >
      <ToolCalls tools={tools} />
      {reasoning.length > 0 ? <Reasoning text={reasoning} live={thinking} /> : null}
      {awaitingFirstWord ? (
        <WorkingIndicator />
      ) : (
        <Spoken
          answer={answer}
          outcome={messageOutcome(message)}
          errored={errored}
          streaming={streaming}
        />
      )}
    </FleetMessageRow>
  );
}

/**
 * What the fleet actually said.
 *
 * An empty answer while the turn is still open means the model has reasoned but
 * not spoken; the disclosure above is already carrying that, and printing an
 * outcome there would announce an ending that has not happened.
 */
function Spoken({
  answer,
  outcome,
  errored,
  streaming,
}: {
  answer: string;
  outcome: string;
  errored: boolean;
  streaming: boolean;
}) {
  if (answer.length === 0 && streaming) return null;
  const body = answer.length > 0 ? answer : outcome;
  return (
    <>
      {errored || answer.length === 0 ? (
        // An outcome and an error are the dashboard's own sentences, not the
        // model's markdown, so they render as written.
        <span
          className={errored ? "text-label font-medium leading-label text-foreground" : undefined}
        >
          {body}
        </span>
      ) : (
        <FleetMarkdown>{body}</FleetMarkdown>
      )}
      {streaming ? (
        <span className="ml-xs animate-pulse text-pulse" aria-label="streaming">
          {STREAM_CURSOR}
        </span>
      ) : null}
    </>
  );
}

/**
 * The model's reasoning, folded away.
 *
 * Open while it is still arriving, because watching a fleet think is the only
 * signal there is during a long turn; closed once the answer lands, because by
 * then it is working-out the operator did not ask for.
 */
function Reasoning({ text, live }: { text: string; live: boolean }) {
  const [opened, setOpened] = useState<string | null>(null);
  const value = opened ?? (live ? REASONING_VALUE : "");
  return (
    <Accordion
      type="single"
      collapsible
      value={value}
      onValueChange={setOpened}
      className="mb-md"
    >
      <AccordionItem value={REASONING_VALUE} className="border-0">
        <AccordionTrigger className="py-xs text-label text-muted-foreground hover:no-underline">
          {live ? REASONING_LIVE_LABEL : REASONING_LABEL}
        </AccordionTrigger>
        <AccordionContent>
          <p className="whitespace-pre-wrap text-body-sm leading-prose text-muted-foreground">
            {text}
          </p>
        </AccordionContent>
      </AccordionItem>
    </Accordion>
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
