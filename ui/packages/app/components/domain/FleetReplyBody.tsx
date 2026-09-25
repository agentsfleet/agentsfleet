"use client";

import { memo, useEffect, useRef, useState } from "react";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
  CopyButton,
} from "@agentsfleet/design-system";
import type { MessageState } from "@assistant-ui/react";
import { PawPrintIcon } from "lucide-react";

import { FleetMarkdown } from "./FleetMarkdown";
import { RecentPaints } from "./RecentPaints";
import { FleetMessageRow, ROW_TONE } from "./FleetMessageRow";
import { ToolCalls, type readTools } from "./FleetToolCalls";
import { messageOutcome } from "./fleetFailureCopy";
import { readQueued, readReasoning, readReply, readReplyRecovering, readSubmittedAtMs, readThinking } from "./fleetMessageReaders";
import {
  STATUS_AGENT_ERROR,
  STATUS_FAILED,
  STATUS_IN_FLIGHT,
  STATUS_OPTIMISTIC,
} from "./fleetMessageStatus";

const STREAM_CURSOR = "▍";
const WORKING_LABEL = "Working";
const QUEUED_LABEL = "Queued";
const COPY_REPLY_LABEL = "Copy reply";
const REASONING_VALUE = "reasoning";
const REASONING_LABEL = "Reasoning";
const REASONING_LIVE_LABEL = "Thinking…";
const RECOVERING_LABEL = "Loading final reply; retrying if needed…";
const FIRST_VISIBLE_MEASURE = "agentsfleet.chat.submit_to_first_visible";
// A user turn moves from an unsplit message to `:reply` when answer text
// arrives. The component remounts, but that is still one visible response.
const measuredPaints = new RecentPaints(400);

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
  const streaming = status === STATUS_IN_FLIGHT || status === STATUS_OPTIMISTIC;
  const reasoning = readReasoning(message);
  const thinking = readThinking(message);
  const recovering = readReplyRecovering(message);
  const answer = reply.trim();
  const eventId = message.id.endsWith(":reply") ? message.id.slice(0, -":reply".length) : message.id;
  useFirstVisiblePaint(eventId, readSubmittedAtMs(message), status !== STATUS_FAILED && (answer.length > 0 || reasoning.length > 0 || tools.length > 0));
  if (status === STATUS_FAILED) return null;
  // Keep the same reply-side cue while delivery is pending and until the
  // first response arrives, so acknowledgement does not flash a second label.
  const awaitingFirstWord = streaming && answer.length === 0 && reasoning.length === 0 && tools.length === 0;
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
        <WorkingIndicator queued={readQueued(message)} />
      ) : (
        <Spoken
          answer={answer}
          outcome={messageOutcome(message)}
          errored={errored}
          streaming={streaming}
        />
      )}
      {recovering ? <output aria-label={RECOVERING_LABEL} className="text-body-sm text-text-subtle">{RECOVERING_LABEL}</output> : null}
      <ReplyActions answer={answer} settled={!streaming && !errored && !recovering} />
    </FleetMessageRow>
  );
}

/** Record browser submit-to-visible time after the first reply, reasoning, or tool paint. */
function useFirstVisiblePaint(eventId: string, submittedAtMs: number | null, visible: boolean): void {
  const measuredEvent = useRef<string | null>(null);
  useEffect(() => {
    if (!visible || submittedAtMs === null || measuredEvent.current === eventId) return;
    const key = `${eventId}:${submittedAtMs}`;
    if (measuredPaints.has(key)) return;
    let secondFrame = 0;
    const firstFrame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => {
        if (measuredPaints.has(key)) return;
        measuredEvent.current = eventId;
        measuredPaints.add(key);
        performance.measure(FIRST_VISIBLE_MEASURE, { start: submittedAtMs, end: performance.now() });
      });
    });
    return () => {
      cancelAnimationFrame(firstFrame);
      cancelAnimationFrame(secondFrame);
    };
  }, [eventId, submittedAtMs, visible]);
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
        <AccordionTrigger className="py-xs text-label text-text-dim hover:no-underline">
          {live ? REASONING_LIVE_LABEL : REASONING_LABEL}
        </AccordionTrigger>
        <AccordionContent>
          <p className="whitespace-pre-wrap text-body-sm leading-prose text-text-dim">{text}</p>
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}

/*
 * The actions under a finished reply.
 *
 * Rendered ONLY on a settled turn, which is both the interaction we want and
 * the cheap one. A streaming reply re-renders on every chunk; mounting a
 * clipboard affordance inside that loop would rebuild it dozens of times for a
 * control nobody can usefully press yet, since the text it would copy is still
 * arriving. Both Claude and ChatGPT reveal these the same way — after the
 * answer lands.
 *
 * `memo` on top of that: the transcript re-renders when ANY row changes, and a
 * settled reply's text does not change again. Comparing one string prop is
 * cheaper than rebuilding a button per frame of somebody else's turn.
 */
const ReplyActions = memo(function ReplyActions({
  answer,
  settled,
}: {
  answer: string;
  settled: boolean;
}) {
  if (!settled || answer.length === 0) return null;
  return (
    <div className="-ml-sm flex items-center gap-xs pt-xs">
      <CopyButton value={answer} label={COPY_REPLY_LABEL} />
    </div>
  );
});

function WorkingIndicator({ queued }: { queued: boolean }) {
  const label = queued ? QUEUED_LABEL : WORKING_LABEL;
  return (
    <output
      className="inline-flex items-center gap-sm text-body-sm text-text-subtle"
      aria-label={label}
      data-testid="fleet-working"
    >
      <PawPrintIcon aria-hidden="true" className="size-4 motion-safe:animate-pulse" />
      <span>{label}…</span>
    </output>
  );
}
