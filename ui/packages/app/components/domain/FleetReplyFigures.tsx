"use client";

import { useEffect, useState } from "react";
import { BrailleSpinner } from "@agentsfleet/design-system";

import { formatDollars } from "@/app/(dashboard)/settings/billing/lib/charges";
import { loadingPhrase, loadingVerbFor } from "@/components/layout/loading-verbs";
import { formatCompactCount, formatElapsed, runFigure } from "@/lib/events/run-figures-format";
import type { ReplyFigures } from "./fleetMessageReaders";

// How long a reply has run, and once it settles, what it took: the line Codex
// ends a turn with, "Worked for 41s", plus what the turn spent.

export const WORKING_LABEL = "Working";
export const QUEUED_LABEL = "Queued";
export const WORKED_FOR = "Worked for";
export const REPLY_FIGURES_TEST_ID = "fleet-reply-figures";
const TOKENS_WORD = "tokens";
const FIGURE_SEPARATOR = " · ";
// The waiting clock shows whole seconds, so it ticks at seconds.
const ELAPSED_TICK_MS = 1_000;

/**
 * "Worked for 41s · 12.4K tokens · $0.03". A figure the run did not report is
 * left out rather than drawn as zero; with none reported, there is no line.
 */
export function FleetReplyFigures({ figures }: { figures: ReplyFigures }) {
  const shown = [
    runFigure(figures.wallMs, (ms) => `${WORKED_FOR} ${formatElapsed(ms)}`),
    runFigure(figures.tokens, (count) => `${formatCompactCount(count)} ${TOKENS_WORD}`),
    runFigure(figures.costNanos, formatDollars),
  ].filter((figure) => figure !== null);
  if (shown.length === 0) return null;
  return (
    <p data-testid={REPLY_FIGURES_TEST_ID} className="pt-xs text-body-sm text-text-subtle tabular-nums">
      {shown.join(FIGURE_SEPARATOR)}
    </p>
  );
}

/** The calls the saved trace dropped to stay inside its bounds, said in the
 * thread so a short list never passes for the whole run. */
export function OmittedCalls({ count }: { count: number }) {
  if (count === 0) return null;
  return (
    <p className="mb-xs text-body-sm text-text-subtle">
      {count} more {count === 1 ? "call" : "calls"} not recorded
    </p>
  );
}

/** The library's `indicator` part: the reply is running and has nothing to show yet.
 * The status is named "Working" or "Queued"; the visible verb is whimsy and the
 * clock ticks, and a live region reads its content, not its name, so both are
 * hidden from assistive tech and the name is its spoken text. */
export function Waiting({ queued, eventId, startedAtMs }: { queued: boolean; eventId: string; startedAtMs: number }) {
  const label = queued ? QUEUED_LABEL : WORKING_LABEL;
  return (
    <output
      className="inline-flex items-center gap-sm text-body-sm text-text-subtle"
      aria-label={label}
      data-testid="fleet-working"
    >
      <BrailleSpinner className="text-pulse" />
      <span className="sr-only">{label}</span>
      <span aria-hidden="true" data-waiting-verb="">{loadingPhrase(queued ? QUEUED_LABEL : loadingVerbFor(eventId))}</span>
      <ElapsedClock startedAtMs={startedAtMs} />
    </output>
  );
}

/** A leaf of its own, so the tick re-renders only the figure. */
function ElapsedClock({ startedAtMs }: { startedAtMs: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), ELAPSED_TICK_MS);
    return () => clearInterval(id);
  }, []);
  return <span aria-hidden="true" data-waiting-elapsed="" className="tabular-nums">({formatElapsed(now - startedAtMs)})</span>;
}
