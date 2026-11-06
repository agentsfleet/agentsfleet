"use client";

import { useEffect, useState } from "react";
import { getFleetEventAction } from "@/app/(dashboard)/w/[workspaceId]/fleets/actions";
import type { EventDetail, EventLocator } from "@/lib/api/events";

export type EventDetailState = {kind: "idle" | "loading" | "failed"} | {kind: "ready"; detail: EventDetail};
type Loaded = {key: string; result: EventDetailState};

/**
 * Fetch one event's bodies when its row is expanded.
 *
 * The list read carries no request or response body — a page is up to 200 rows
 * and is deliberately kept off oversized-attribute storage — so the two things
 * this dialog exists to show arrive on their own request, for the one event the
 * operator actually opened.
 *
 * Routed through a Server Action, never a browser fetch: the dashboard's
 * credential does not leave the server, and a lint enforces it
 * (the ast-grep rule app-client-credential). That is also why this hook
 * takes no token and no caller threads one down.
 *
 * A chat row may hold only the address. Its loading/failure state must stay
 * distinct from a confirmed record that happened to carry no result.
 */
export function useEventDetail(row: EventLocator | null, refresh?: string): EventDetailState {
  const [loaded, setLoaded] = useState<Loaded | null>(null);

  // Object churn is inert; a recorded status/update change refreshes the
  // selected run once, without adding a polling loop.
  const workspaceId = row?.workspace_id ?? null;
  const fleetId = row?.fleet_id ?? null;
  const eventId = row?.event_id ?? null;
  const key = JSON.stringify([workspaceId, fleetId, eventId, refresh]);

  useEffect(() => {
    if (workspaceId === null || fleetId === null || eventId === null) {
      setLoaded(null);
      return;
    }
    // An AbortController rather than a `let cancelled` flag: the guard has to
    // survive an await, and `signal.aborted` reads as a live value where a
    // captured boolean reads to static analysis as never changing.
    const inflight = new AbortController();
    setLoaded({key, result: {kind: "loading"}});
    void (async () => {
      const result = await getFleetEventAction(workspaceId, fleetId, eventId)
        .catch(() => ({ ok: false as const, error: "unreachable" }));
      if (inflight.signal.aborted) return;
      setLoaded({key, result: result.ok ? {kind: "ready", detail: result.data} : {kind: "failed"}});
    })();
    return () => {
      inflight.abort();
    };
  }, [workspaceId, fleetId, eventId, key]);

  if (row === null) return {kind: "idle"};
  return loaded?.key === key ? loaded.result : {kind: "loading"};
}
