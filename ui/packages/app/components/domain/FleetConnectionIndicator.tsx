"use client";

import { useEffect, useRef, useState } from "react";
import { WakePulse, cn } from "@agentsfleet/design-system";

import { CONNECTION_STATUS, type ConnectionStatus } from "./useFleetEventStream";

// The connection, said in one place: a dot, a word, and motion that matches
// what is actually happening.
//
// Motion is the whole point. The steady wake pulse means live; a working
// shimmer means we are trying; nothing moving means nothing is happening. The
// old indicator pulsed ONLY when already live, so the one moment the operator
// wanted a sign of life — while connecting — was the one moment it sat still.

const STATUS_LABEL: Record<ConnectionStatus, string> = {
  [CONNECTION_STATUS.CONNECTING]: "Connecting…",
  [CONNECTION_STATUS.LIVE]: "Live",
  [CONNECTION_STATUS.RECONNECTING]: "Reconnecting…",
  [CONNECTION_STATUS.OFFLINE]: "Not live",
};

const STATUS_CLASS: Record<ConnectionStatus, string> = {
  [CONNECTION_STATUS.CONNECTING]: "text-info",
  [CONNECTION_STATUS.LIVE]: "text-pulse",
  [CONNECTION_STATUS.RECONNECTING]: "text-warning",
  [CONNECTION_STATUS.OFFLINE]: "text-destructive",
};

/** How long the arrival cue plays before the steady pulse takes over. */
const ARRIVAL_CUE_MS = 700;

const WORKING_STATUSES: ReadonlySet<ConnectionStatus> = new Set([
  CONNECTION_STATUS.CONNECTING,
  CONNECTION_STATUS.RECONNECTING,
]);

/**
 * True for a moment after the connection comes up, and only when it was
 * genuinely trying before — so a surface that mounts already-live does not
 * announce an arrival the operator never waited for.
 */
function useArrivalCue(status: ConnectionStatus): boolean {
  const wasWorking = useRef(false);
  const [cue, setCue] = useState(false);

  useEffect(() => {
    if (WORKING_STATUSES.has(status)) {
      wasWorking.current = true;
      setCue(false);
      return;
    }
    if (status !== CONNECTION_STATUS.LIVE || !wasWorking.current) return;
    wasWorking.current = false;
    setCue(true);
    const timer = setTimeout(() => setCue(false), ARRIVAL_CUE_MS);
    return () => clearTimeout(timer);
  }, [status]);

  return cue;
}

export function FleetConnectionIndicator({ status }: { status: ConnectionStatus }) {
  const live = status === CONNECTION_STATUS.LIVE;
  const working = WORKING_STATUSES.has(status);
  const arrived = useArrivalCue(status);
  return (
    <span
      aria-label={`Connection status: ${STATUS_LABEL[status]}`}
      className={cn(
        "inline-flex items-center gap-sm font-mono text-label",
        STATUS_CLASS[status],
        // One short cue on arrival, then it settles. `motion-safe` so a
        // reduced-motion reader gets the colour and word change alone.
        arrived && "motion-safe:animate-in motion-safe:zoom-in-50 motion-safe:duration-300",
      )}
      data-connection={status}
      data-arrived={arrived || undefined}
    >
      <WakePulse
        live={live}
        className={cn(
          "inline-block h-2 w-2 rounded-full bg-current",
          // Trying is not the same state as connected, so it does not borrow
          // the live pulse — it gets its own, plainer motion.
          working && "motion-safe:animate-pulse",
        )}
        aria-hidden="true"
      />
      {STATUS_LABEL[status]}
    </span>
  );
}
