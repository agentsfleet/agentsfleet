"use client";

import { useEffect, useState } from "react";
import { getFleetEventAction } from "@/app/(dashboard)/w/[workspaceId]/fleets/actions";
import type { EventDetail, EventRow } from "@/lib/api/events";

/**
 * Fetch one event's bodies when its row is expanded.
 *
 * The list read carries no request or response body — a page is up to 200 rows
 * and is deliberately kept off oversized-attribute storage — so the two things
 * this dialog exists to show arrive on their own request, for the one event the
 * operator actually opened.
 *
 * Routed through a Server Action, never a browser fetch: the dashboard's
 * credential does not leave the server, and a gate enforces it
 * (`tests/grep-gates/no-api-template-mint.test.ts`). That is also why this hook
 * takes no token and no caller threads one down.
 *
 * Returns `null` while in flight and on failure. Both render as "not recorded"
 * rather than an error banner — every other field in the dialog arrived with
 * the row and is already on screen, so a failed body fetch degrades one panel
 * instead of blanking the view.
 */
export function useEventDetail(row: EventRow | null): EventDetail | null {
  const [detail, setDetail] = useState<EventDetail | null>(null);

  // Keyed on the identity of the open row, not the object: re-rendering the
  // list must not re-fetch a body that is already on screen.
  const workspaceId = row?.workspace_id ?? null;
  const fleetId = row?.fleet_id ?? null;
  const eventId = row?.event_id ?? null;

  useEffect(() => {
    if (workspaceId === null || fleetId === null || eventId === null) {
      setDetail(null);
      return;
    }
    // An AbortController rather than a `let cancelled` flag: the guard has to
    // survive an await, and `signal.aborted` reads as a live value where a
    // captured boolean reads to static analysis as never changing.
    const inflight = new AbortController();
    // Cleared before the request so reopening on a different row never shows
    // the previous row's answer while this one is still in flight.
    setDetail(null);
    void (async () => {
      const result = await getFleetEventAction(workspaceId, fleetId, eventId)
        .catch(() => ({ ok: false as const, error: "unreachable" }));
      if (inflight.signal.aborted) return;
      setDetail(result.ok ? result.data : null);
    })();
    return () => {
      inflight.abort();
    };
  }, [workspaceId, fleetId, eventId]);

  return detail;
}
