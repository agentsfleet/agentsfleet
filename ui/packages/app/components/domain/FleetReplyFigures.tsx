"use client";

import { useEffect, useState } from "react";
import {
  Button, DescriptionDetails, DescriptionList, DescriptionTerm, Popover, PopoverContent,
  PopoverTrigger, SignalSpinner,
} from "@agentsfleet/design-system";

import { formatDollars } from "@/lib/money";
import { formatCompactCount, formatElapsed, formatRunDuration, runFigure } from "@/lib/events/run-figures-format";
import type { ReplyFigures } from "./fleetMessageReaders";

export const WORKING_LABEL = "Working";
export const QUEUED_LABEL = "Queued";
export const RUN_DETAILS_LABEL = "Run details";
export const REPLY_FIGURES_TEST_ID = "fleet-reply-figures";
// The waiting clock shows whole seconds, so it ticks at seconds.
const ELAPSED_TICK_MS = 1_000;

/**
 * Optional run information stays outside the transcript layout. Missing figures
 * remain absent; a run reporting nothing gets no details control.
 */
export function FleetReplyFigures({ figures }: { figures: ReplyFigures }) {
  const rows = [
    { label: "Duration", value: runFigure(figures.wallMs, formatRunDuration) },
    { label: "Tokens", value: runFigure(figures.tokens, formatCompactCount) },
    { label: "Reported cost", value: runFigure(figures.costNanos, formatDollars) },
  ].filter(row => row.value !== null);
  if (rows.length === 0) return null;
  return <Popover>
    <PopoverTrigger asChild>
      <Button variant="ghost" size="sm" data-testid={REPLY_FIGURES_TEST_ID}>{RUN_DETAILS_LABEL}</Button>
    </PopoverTrigger>
    <PopoverContent side="top" align="start" aria-label={RUN_DETAILS_LABEL}>
      <DescriptionList className="tabular-nums">
        {rows.map(row => <div key={row.label}><DescriptionTerm>{row.label}</DescriptionTerm><DescriptionDetails>{row.value}</DescriptionDetails></div>)}
      </DescriptionList>
    </PopoverContent>
  </Popover>;
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
 * Keep the label stable; only the isolated clock ticks. */
export function Waiting({ queued, startedAtMs }: { queued: boolean; startedAtMs: number }) {
  const label = queued ? QUEUED_LABEL : WORKING_LABEL;
  return (
    <output
      className="inline-flex items-center gap-sm text-body-sm text-text-subtle"
      aria-label={label}
      data-testid="fleet-working"
    >
      <SignalSpinner className="text-pulse" />
      <span className="sr-only">{label}</span>
      <span aria-hidden="true" data-waiting-verb="">{label}…</span>
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
