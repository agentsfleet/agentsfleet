"use client";

import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { useScrollLock } from "@assistant-ui/react";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@agentsfleet/design-system";

import { formatSeconds } from "@/lib/utils";
import { ChatProgressLabel } from "../../shared/chat/ChatProgressLabel";

// The reasoning group of a reply, as one chip: "Thinking · 3s <latest
// sentence>" while it streams, "Thought · 8.5s" once the answer starts. The
// library decides live or settled (the group's status); the row decides how
// long (its reasoning span, stamped where it survives a remount).

export const THOUGHT_LIVE_LABEL = "Thinking";
export const THOUGHT_LABEL = "Thought";
const THOUGHT_VALUE = "thought";
const DURATION_SEPARATOR = " · ";
// The accordion's fold (tw-animate-css `accordion-up`, .2s), so the viewport
// holds still for exactly that long.
const FOLD_ANIMATION_MS = 200;
// Only the tail can hold the latest sentence; scanning more costs every frame.
const SENTENCE_TAIL_CHARS = 400;
const SENTENCES = new Intl.Segmenter("en", { granularity: "sentence" });

export type FleetThoughtProps = {
  live: boolean;
  reasoning: string;
  startedAtMs: number | null;
  endedAtMs: number | null;
  children: ReactNode;
};

/**
 * Keep reasoning collapsed until requested. Stream updates never change the
 * reader's choice or move the answer by opening and folding a growing panel.
 * Closed content unmounts, so a long thought leaves no hidden DOM behind.
 */
export function FleetThought({ live, reasoning, startedAtMs, endedAtMs, children }: FleetThoughtProps) {
  const [value, setValue] = useState("");
  // The item stays mounted through a fold; the content does not, so the lock
  // finds the scroller from here.
  const itemRef = useRef<HTMLDivElement | null>(null);
  const lockScroll = useScrollLock(itemRef, FOLD_ANIMATION_MS);
  // Taken as the fold commits and before it paints, for a click and for the
  // fold the answer triggers alike: the library locks before the height moves.
  const previous = useRef(value);
  useLayoutEffect(() => {
    if (previous.current !== value) lockScroll();
    previous.current = value;
  }, [value, lockScroll]);
  return (
    <Accordion type="single" collapsible value={value} onValueChange={setValue} className="mb-md">
      <AccordionItem ref={itemRef} value={THOUGHT_VALUE} className="border-0">
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
        <AccordionContent>
          {children}
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}

function LiveLabel({ reasoning, startedAtMs }: { reasoning: string; startedAtMs: number | null }) {
  return (
    <span className="flex min-w-0 items-center gap-sm">
      <span className="shrink-0 text-body-sm"><ChatProgressLabel label={THOUGHT_LIVE_LABEL} startedAtMs={startedAtMs ?? undefined} /></span>
      <span aria-hidden="true" className="truncate text-text-subtle">{latestSentence(reasoning)}</span>
    </span>
  );
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
