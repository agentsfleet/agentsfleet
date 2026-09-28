"use client";

import { useEffect, useRef, useState, type ReactNode } from "react";
import { useScrollLock } from "@assistant-ui/react";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
  BrailleSpinner,
} from "@agentsfleet/design-system";

import { formatSeconds } from "@/lib/utils";

// The reasoning group of a reply, as one chip: "⠧ Thinking · 3.2s <latest
// sentence>" while it streams, "Thought · 8.5s" once the answer starts. The
// library decides live or settled (the group's status); the row decides how
// long (its reasoning span, stamped where it survives a remount).

export const THOUGHT_LIVE_LABEL = "Thinking";
export const THOUGHT_LABEL = "Thought";
const THOUGHT_VALUE = "thought";
const DURATION_SEPARATOR = " · ";
// Tenths are what the label shows, so the clock ticks at tenths. The leaf owns
// the tick; nothing above it re-renders.
const CLOCK_TICK_MS = 100;
// The accordion's fold (tw-animate-css `accordion-up`, .2s), so the viewport
// holds still for exactly that long.
const FOLD_ANIMATION_MS = 200;
// Only the tail can hold the latest sentence; scanning more costs every frame.
const SENTENCE_TAIL_CHARS = 400;
const SENTENCES = new Intl.Segmenter(undefined, { granularity: "sentence" });

export type FleetThoughtProps = {
  live: boolean;
  reasoning: string;
  startedAtMs: number | null;
  endedAtMs: number | null;
  children: ReactNode;
};

/**
 * Open while it streams, because watching a fleet think is the only signal
 * during a long turn; closed once the answer lands, unless the operator opened
 * it. Closed content unmounts, so a long thought leaves no hidden DOM behind.
 */
export function FleetThought({ live, reasoning, startedAtMs, endedAtMs, children }: FleetThoughtProps) {
  const [opened, setOpened] = useState<string | null>(null);
  const contentRef = useRef<HTMLDivElement | null>(null);
  const lockScroll = useScrollLock(contentRef, FOLD_ANIMATION_MS);
  const value = opened ?? (live ? THOUGHT_VALUE : "");
  return (
    <Accordion type="single" collapsible value={value} onValueChange={setOpened} className="mb-md">
      <AccordionItem value={THOUGHT_VALUE} className="border-0">
        <AccordionTrigger className="min-w-0 py-xs text-label text-text-dim hover:no-underline">
          {live ? (
            <LiveLabel reasoning={reasoning} startedAtMs={startedAtMs} />
          ) : (
            <span>
              {THOUGHT_LABEL}
              {startedAtMs !== null && endedAtMs !== null
                ? `${DURATION_SEPARATOR}${formatSeconds(endedAtMs - startedAtMs)}`
                : null}
            </span>
          )}
        </AccordionTrigger>
        <AccordionContent ref={contentRef} onAnimationStart={lockScroll}>
          {children}
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}

function LiveLabel({ reasoning, startedAtMs }: { reasoning: string; startedAtMs: number | null }) {
  return (
    <span className="flex min-w-0 items-center gap-sm">
      <BrailleSpinner className="text-pulse" />
      <span className="shrink-0">
        {THOUGHT_LIVE_LABEL}
        {/* The clock and the sentence are for the eye. The chip sits in the
            transcript's live log, so a ticking name would be read out ten
            times a second; the button's name stays "Thinking". */}
        {startedAtMs !== null ? (
          <span aria-hidden="true">
            {DURATION_SEPARATOR}
            <ThoughtClock startedAtMs={startedAtMs} />
          </span>
        ) : null}
      </span>
      <span aria-hidden="true" className="truncate text-text-subtle">{latestSentence(reasoning)}</span>
    </span>
  );
}

/** Mounted only while live; unmounting on fold is what stops the interval. */
function ThoughtClock({ startedAtMs }: { startedAtMs: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), CLOCK_TICK_MS);
    return () => clearInterval(id);
  }, []);
  return <span className="tabular-nums">{formatSeconds(now - startedAtMs)}</span>;
}

/** The last sentence of the reasoning so far, whole or still arriving. */
export function latestSentence(text: string): string {
  let latest = "";
  for (const { segment } of SENTENCES.segment(text.slice(-SENTENCE_TAIL_CHARS))) {
    const trimmed = segment.trim();
    if (trimmed.length > 0) latest = trimmed;
  }
  return latest;
}
