"use client";

import { createContext, memo, useCallback, useContext, useDeferredValue, useEffect, useRef, useState, type ReactNode } from "react";
import { BrailleSpinner, CopyButton } from "@agentsfleet/design-system";
import { MessagePrimitive, groupPartByType, type MessageState } from "@assistant-ui/react";

import { loadingPhrase, loadingVerbFor } from "@/components/layout/loading-verbs";
import { truncate } from "@/lib/utils";
import { FleetFailedOutcome } from "./FleetFailedOutcome";
import { FleetMarkdown, FleetStreamingMarkdown } from "./FleetMarkdown";
import { FleetMessageRow, ROW_TONE } from "./FleetMessageRow";
import { FleetThought } from "./FleetThought";
import { ToolCallList, ToolCallRow } from "./FleetToolCalls";
import { messageOutcome } from "./fleetFailureCopy";
import { readQueued, readReasoningSpan, readReplyRecovering, readSubmittedAtMs, readText } from "./fleetMessageReaders";
import { STATUS_AGENT_ERROR } from "./fleetMessageStatus";
import { REPLY_ID_SUFFIX } from "./useFleetThreadEntries";
import { useFirstVisiblePaint } from "./useFirstVisiblePaint";

const STREAM_CURSOR = "▍";
const WORKING_LABEL = "Working";
const QUEUED_LABEL = "Queued";
const COPY_REPLY_LABEL = "Copy reply";
const RECOVERING_LABEL = "Loading final reply; retrying if needed…";
const DEFAULT_SENDER = "Fleet";
export const SETTLED_REPLY_STATUS = "settled-reply-status";
// Long enough for a screen reader to queue what the status says; any longer and
// the copy only repeats the reply to someone browsing the page.
export const ANNOUNCEMENT_HOLD_MS = 7_000;
// A settled reply is announced as a summary: the row holds the whole answer,
// and a screen reader reading an essay from a status cannot be skimmed.
export const SPOKEN_REPLY_MAX_CHARS = 200;
// The markdown a summary drops, cheaply: fences, link and image targets, block
// markers, and emphasis marks. An underscore inside a word is kept, so
// `fleet_id` reads as written.
const MARKDOWN_TO_SPOKEN: ReadonlyArray<readonly [RegExp, string]> = [
  [/^[ \t]*(?:```|~~~).*$/gm, ""],
  [/^[ \t]*(?:[-*_][ \t]*){3,}$/gm, ""],
  [/!?\[([^\]]*)\]\([^)]*\)/g, "$1"],
  [/^[ \t]{0,3}(?:#{1,6}[ \t]+|>[ \t]?|[-*+][ \t]+|\d+[.)][ \t]+)/gm, ""],
  [/[*`~]+|(?<!\w)_+|_+(?!\w)/g, ""],
  [/\|/g, " "],
  [/\s+/g, " "],
];
// The library groups the parts: the reasoning becomes one Thought chip, and
// adjacent tool calls one list. Module scope keeps the grouping's memo
// fingerprint stable across renders.
const GROUP = {
  REASONING: "group-reasoning",
  TOOL: "group-tool",
} as const;
const REPLY_GROUP_BY = groupPartByType({
  reasoning: [GROUP.REASONING],
  "tool-call": [GROUP.TOOL],
});

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
  const running = message.status?.type === "running";
  const recovering = readReplyRecovering(message);
  const answer = readText(message).trim();
  const span = readReasoningSpan(message);
  const queued = readQueued(message);
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
      <MessagePrimitive.GroupedParts groupBy={REPLY_GROUP_BY}>
        {(info) => renderReplyPart(info, { errored, running, queued, eventId, reasoning: reasoningText(message), span })}
      </MessagePrimitive.GroupedParts>
      {answer.length === 0 && !running ? (
        errored ? <FleetFailedOutcome>{messageOutcome(message)}</FleetFailedOutcome> : <span>{messageOutcome(message)}</span>
      ) : null}
      {recovering ? <output aria-label={RECOVERING_LABEL} className="text-body-sm text-text-subtle">{RECOVERING_LABEL}</output> : null}
      <ReplyActions answer={answer} settled={!running && !errored && !recovering} />
    </FleetMessageRow>
  );
}

export type ReplyContext = {
  errored: boolean;
  running: boolean;
  queued: boolean;
  eventId: string;
  reasoning: string;
  span: ReturnType<typeof readReasoningSpan>;
};

/** One switch over every node the library hands back: groups, leaves, the indicator. */
export function renderReplyPart(
  { part, children }: MessagePrimitive.GroupedParts.RenderInfo<(typeof GROUP)[keyof typeof GROUP]>,
  reply: ReplyContext,
): ReactNode {
  switch (part.type) {
    case GROUP.REASONING:
      return (
        <FleetThought
          live={part.status.type === "running"}
          reasoning={reply.reasoning}
          startedAtMs={reply.span.startedAtMs}
          endedAtMs={reply.span.endedAtMs}
        >
          {children}
        </FleetThought>
      );
    case GROUP.TOOL:
      return <ToolCallList>{children}</ToolCallList>;
    case "reasoning":
      return <p className="whitespace-pre-wrap text-body-sm leading-prose text-text-dim">{part.text}</p>;
    case "tool-call":
      return <ToolCallRow name={part.toolName} done={part.result !== undefined} />;
    case "text":
      return <ReplyText text={part.text} errored={reply.errored} streaming={reply.running} />;
    case "indicator":
      return <Waiting queued={reply.queued} eventId={reply.eventId} />;
    default:
      // A leaf that returns null gets the library's fallback UI; an empty
      // fragment keeps parts this reply never carries invisible.
      return <></>;
  }
}

function reasoningText(message: MessageState): string {
  for (const part of message.content) {
    if (part.type === "reasoning") return part.text;
  }
  return "";
}

/**
 * What the fleet said. Parsing a growing answer is the reply's heaviest
 * render, so it is deferred: React 19 `useDeferredValue` lets typing and
 * scrolling interrupt it, and the markdown catches up on the next idle frame.
 * While it streams, only its open block is parsed per flush; once settled it
 * is parsed whole, once. `streaming` is deferred with the text, so the settled
 * parse reads the final answer rather than the flush before it.
 */
function ReplyText({ text, errored, streaming }: { text: string; errored: boolean; streaming: boolean }) {
  const deferred = useDeferredValue(text);
  const deferredStreaming = useDeferredValue(streaming);
  const markdown = deferredStreaming ? (
    <FleetStreamingMarkdown text={deferred} />
  ) : (
    <FleetMarkdown>{deferred}</FleetMarkdown>
  );
  return (
    <>
      {errored ? <FleetFailedOutcome>{deferred}</FleetFailedOutcome> : markdown}
      {streaming ? (
        <span className="ml-xs animate-pulse text-pulse" aria-label="streaming">
          {STREAM_CURSOR}
        </span>
      ) : null}
    </>
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

// A reply reports its own settling: only it knows it was seen running. The
// default does nothing, for a reply drawn outside a transcript.
const AnnounceSettled = createContext<(text: string) => void>(() => {});

// Each announcement carries its own number, so the same words twice in a row
// still replace the status's text node and are read again.
type Announcement = { text: string; seq: number };
const SILENT: Announcement = { text: "", seq: 0 };

/**
 * The transcript's one polite status. The log itself stays silent, since a
 * streamed reply rewrites it on every flush; a reply that settles while on
 * screen is read out here once, then cleared. `children` pass through
 * untouched, so an announcement re-renders this status and nothing in the log.
 */
export function SettledReplyStatus({ children }: { children: ReactNode }) {
  const [announcement, setAnnouncement] = useState(SILENT);
  const announce = useCallback((text: string) => setAnnouncement((prev) => ({ text, seq: prev.seq + 1 })), []);
  useEffect(() => {
    if (announcement.text.length === 0) return;
    const cleared = setTimeout(() => setAnnouncement((prev) => ({ text: "", seq: prev.seq })), ANNOUNCEMENT_HOLD_MS);
    return () => clearTimeout(cleared);
  }, [announcement]);
  return (
    <AnnounceSettled value={announce}>
      {children}
      <output className="sr-only" data-testid={SETTLED_REPLY_STATUS}>
        <span key={announcement.seq}>{announcement.text}</span>
      </output>
    </AnnounceSettled>
  );
}

// A reply that mounts settled is history and stays unannounced; one seen open
// is announced when it settles, and never again.
// The summary is made only when it is spoken, never per streamed flush.
function useSettleAnnouncement(settled: boolean, sender: string, text: string) {
  const announce = useContext(AnnounceSettled);
  const seenOpen = useRef(false);
  useEffect(() => {
    if (!settled) {
      seenOpen.current = true;
      return;
    }
    if (!seenOpen.current) return;
    seenOpen.current = false;
    announce(`${sender}: ${spokenSummary(text)}`);
  }, [settled, sender, text, announce]);
}

/** A reply as a screen reader should hear it: plain words, bounded. */
export function spokenSummary(text: string): string {
  let spoken = text;
  for (const [syntax, replacement] of MARKDOWN_TO_SPOKEN) spoken = spoken.replace(syntax, replacement);
  return truncate(spoken.trim(), SPOKEN_REPLY_MAX_CHARS);
}

/** The library's `indicator` part: the reply is running and has nothing to show yet.
 * The status is named "Working" or "Queued"; the visible verb is whimsy, and a
 * live region reads its content, not its name, so the verb is hidden from
 * assistive tech and the name is its spoken text. */
function Waiting({ queued, eventId }: { queued: boolean; eventId: string }) {
  const label = queued ? QUEUED_LABEL : WORKING_LABEL;
  return (
    <output
      className="inline-flex items-center gap-sm text-body-sm text-text-subtle"
      aria-label={label}
      data-testid="fleet-working"
    >
      <BrailleSpinner className="text-pulse" />
      <span className="sr-only">{label}</span>
      <span aria-hidden="true">{loadingPhrase(queued ? QUEUED_LABEL : loadingVerbFor(eventId))}</span>
    </output>
  );
}
