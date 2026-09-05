"use client";

import { useEffect, useRef } from "react";
import type { FleetEvent, FleetEventStatus } from "@/lib/streaming/fleet-stream-frames";
import { AGENTSFLEET_EVENT_STATUS } from "@/lib/streaming/fleet-stream-frames";
import type { EventRow } from "@/lib/api/events";

const TERMINAL_EVENT_STATUSES: ReadonlySet<FleetEventStatus> = new Set([
  AGENTSFLEET_EVENT_STATUS.PROCESSED,
  AGENTSFLEET_EVENT_STATUS.AGENT_ERROR,
  AGENTSFLEET_EVENT_STATUS.GATE_BLOCKED,
]);

/**
 * A burst of completions coalesces into ONE summary refresh. Trailing-edge —
 * the last completion in a burst is always reflected.
 */
export const REFRESH_DEBOUNCE_MS = 2_000;

/**
 * Calls `onRunCompleted` when a streamed run reaches a terminal status —
 * debounced, trailing-edge, and cancelled on unmount so a dead route never
 * refreshes its successor. The caller decides what a completion refreshes; this
 * hook only decides WHEN. It never touches the router: re-running the whole
 * detail-page fetch graph per completion is the cost this shape retired.
 */
export function useRefreshSummariesOnCompletion(
  initial: EventRow[],
  events: FleetEvent[],
  onRunCompleted: () => void,
) {
  const refreshTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // The latest callback, read when the timer fires — a re-render with a new
  // closure must neither reschedule the debounce nor call a stale one.
  const onRunCompletedRef = useRef(onRunCompleted);
  onRunCompletedRef.current = onRunCompleted;
  const terminalEventIds = useRef(
    new Set([
      ...events
        .filter((event) => TERMINAL_EVENT_STATUSES.has(event.status))
        .map((event) => event.id),
      ...initial
        .filter((event) => event.status !== AGENTSFLEET_EVENT_STATUS.RECEIVED)
        .map((event) => event.event_id),
    ]),
  );
  useEffect(() => {
    let completed = false;
    for (const event of events) {
      if (
        TERMINAL_EVENT_STATUSES.has(event.status) &&
        terminalEventIds.current.has(event.id) === false
      ) {
        terminalEventIds.current.add(event.id);
        completed = true;
      }
    }
    if (completed) {
      if (refreshTimer.current !== null) clearTimeout(refreshTimer.current);
      refreshTimer.current = setTimeout(() => {
        refreshTimer.current = null;
        onRunCompletedRef.current();
      }, REFRESH_DEBOUNCE_MS);
    }
  }, [events]);
  // A pending refresh dies with the surface that scheduled it — an unmounted
  // route must not refresh whichever page replaced it.
  useEffect(() => {
    return () => {
      if (refreshTimer.current !== null) clearTimeout(refreshTimer.current);
    };
  }, []);
}
