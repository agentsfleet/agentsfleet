"use client";

import { createContext, memo, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { CopyButton } from "@agentsfleet/design-system";
import { MessagePrimitive, groupPartByType, type MessageState } from "@assistant-ui/react";

import { truncate } from "@/lib/utils";
import { FLEET_OUTCOME_CLASS } from "./FleetFailedOutcome";
import { FleetReplyFailure } from "./FleetReplyFailure";
import { FleetReplyMarkdown } from "./FleetMarkdown";
import { FleetMessageRow, ROW_TONE } from "./FleetMessageRow";
import { FleetExplored } from "./FleetExplored";
import { FleetReplyFigures, OmittedCalls, Waiting } from "./FleetReplyFigures";
import { FleetThought } from "./FleetThought";
import { ToolCallList, ToolCallRow } from "./FleetToolCalls";
import { messageOutcome } from "./fleetFailureCopy";
import { readOmittedCallCount, readQueued, readReasoningSpan, readReplyFigures, readReplyRecovering, readSubmittedAtMs, readText } from "./fleetMessageReaders";
import { STATUS_AGENT_ERROR } from "./fleetMessageStatus";
import { EXPLORE_TOOLS } from "./tool-call-explore";
import { STATUS_RUNNING } from "./fleetReplyMessage";
import { REPLY_ID_SUFFIX } from "./useFleetThreadEntries";
import { useFirstVisiblePaint } from "./useFirstVisiblePaint";
import { ContextProgressStatus } from "./ContextProgressStatus";
import type { ReplyFigures } from "./fleetMessageReaders";
import { readContextProgress } from "@/lib/context-progress";

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
// The library groups the parts: the reasoning becomes one Thought chip,
// adjacent reads one Explored cell, and other adjacent tool calls one list. A
// `tool-call:<name>` key outranks the plain one. Module scope keeps the
// grouping's memo fingerprint stable across renders.
const GROUP = {
  REASONING: "group-reasoning",
  EXPLORE: "group-explore",
  TOOL: "group-tool",
} as const;
const REPLY_GROUP_BY = groupPartByType({
  reasoning: [GROUP.REASONING],
  ...Object.fromEntries(EXPLORE_TOOLS.map((name) => [`tool-call:${name}`, [GROUP.EXPLORE]])),
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
      <MessagePrimitive.GroupedParts groupBy={REPLY_GROUP_BY}>
        {(info) => renderReplyPart(info, {
          errored, running, queued, eventId, reasoning: reasoningText(message), span,
          content: message.content, startedAtMs: message.createdAt.getTime(),
          contextProgress: context,
        })}
      </MessagePrimitive.GroupedParts>
      {running ? <ContextProgressStatus value={context} /> : null}
      <OmittedCalls count={readOmittedCallCount(message)} />
      {errored && !running && !recovering ? <FleetReplyFailure message={message} /> : null}
      {answer.length === 0 && !running && !errored ? <p className={FLEET_OUTCOME_CLASS}>{messageOutcome(message)}</p> : null}
      {recovering ? <output aria-label={RECOVERING_LABEL} className="text-body-sm text-text-subtle">{RECOVERING_LABEL}</output> : null}
      <ReplyActions answer={answer} settled={!running && !errored && !recovering} figures={readReplyFigures(message)} />
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
  /** The message's parts, which a group reads by index. */
  content: MessageState["content"];
  /** When the turn began, for the waiting clock. */
  startedAtMs: number;
  contextProgress?: unknown;
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
          live={part.status.type === STATUS_RUNNING}
          reasoning={reply.reasoning}
          startedAtMs={reply.span.startedAtMs}
          endedAtMs={reply.span.endedAtMs}
        >
          {children}
        </FleetThought>
      );
    case GROUP.EXPLORE:
      // The group draws its own lines; its leaves stay unmounted.
      return <FleetExplored content={reply.content} indices={part.indices} running={part.counts.running > 0} />;
    case GROUP.TOOL:
      return <ToolCallList>{children}</ToolCallList>;
    case "reasoning":
      return <p className="whitespace-pre-wrap text-body-sm leading-prose text-text-dim">{part.text}</p>;
    case "tool-call":
      return (
        <ToolCallRow
          name={part.toolName}
          args={part.args}
          argsText={part.argsText}
          result={part.result}
          running={part.status.type === STATUS_RUNNING}
          settled={!reply.running}
          eventId={reply.eventId}
        />
      );
    case "text":
      return <ReplyText streaming={reply.running} />;
    case "indicator":
      return reply.contextProgress ? <></> : <Waiting queued={reply.queued} startedAtMs={reply.startedAtMs} />;
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
 * The library owns smoothing and deferred parsing within the current text part.
 * Completing a run updates its status without replacing the displayed answer.
 */
function ReplyText({ streaming }: { streaming: boolean }) {
  return (
    <>
      <FleetReplyMarkdown />
      {streaming ? (
        <span className="sr-only" aria-label="streaming">Receiving reply</span>
      ) : null}
    </>
  );
}

/*
 * The actions under a finished reply.
 *
 * Keep the action row's height while the answer streams. Revealing controls
 * when it settles must not add a footer and push the reader's position.
 *
 * `memo` on top of that: the transcript re-renders when ANY row changes, and a
 * settled reply's text does not change again. Comparing one string prop is
 * cheaper than rebuilding a button per frame of somebody else's turn.
 */
const ReplyActions = memo(function ReplyActions({
  answer,
  settled,
  figures,
}: {
  answer: string;
  settled: boolean;
  figures: ReplyFigures;
}) {
  return (
    <div className="-ml-sm flex h-11 items-center gap-xs">
      {settled ? <>
        {answer.length > 0 ? <CopyButton value={answer} label={COPY_REPLY_LABEL} /> : null}
        <FleetReplyFigures figures={figures} />
      </> : null}
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
