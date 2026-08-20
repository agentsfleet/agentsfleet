"use client";

import { useEffect, useRef } from "react";
import { useRouter } from "next/navigation";
import type { FleetEvent, FleetEventStatus } from "@/lib/streaming/fleet-stream-frames";
import { AGENTSFLEET_EVENT_STATUS } from "@/lib/streaming/fleet-stream-frames";
import type { EventRow } from "@/lib/api/events";

const TERMINAL_EVENT_STATUSES: ReadonlySet<FleetEventStatus> = new Set([
  AGENTSFLEET_EVENT_STATUS.PROCESSED,
  AGENTSFLEET_EVENT_STATUS.AGENT_ERROR,
  AGENTSFLEET_EVENT_STATUS.GATE_BLOCKED,
]);

/**
 * A burst of completions coalesces into ONE server re-render: `router.refresh`
 * re-runs the whole detail-page fetch graph, so firing it per terminal frame
 * multiplies that cost by the burst size. Trailing-edge — the last completion
 * in a burst is always reflected.
 */
export const REFRESH_DEBOUNCE_MS = 2_000;

/**
 * Refresh the surrounding Server Components (run counters, header badges) when
 * a streamed run reaches a terminal status — debounced, trailing-edge, and
 * cancelled on unmount so a dead route never refreshes its successor.
 */
export function useRefreshSummariesOnCompletion(
  initial: EventRow[],
  events: FleetEvent[],
) {
  const router = useRouter();
  const refreshTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
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
        router.refresh();
      }, REFRESH_DEBOUNCE_MS);
    }
  }, [events, router]);
  // A pending refresh dies with the surface that scheduled it — an unmounted
  // route must not refresh whichever page replaced it.
  useEffect(() => {
    return () => {
      if (refreshTimer.current !== null) clearTimeout(refreshTimer.current);
    };
  }, []);
}
