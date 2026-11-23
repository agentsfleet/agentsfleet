"use client";

import { useEffect, useState } from "react";
import { getFleetTurnAction } from "@/app/(dashboard)/w/[workspaceId]/fleets/actions";
import type { TurnDetail, TurnLocator } from "@/lib/api/turns";

export type TurnDetailState = {kind: "idle" | "loading" | "failed"} | {kind: "ready"; detail: TurnDetail};
type Loaded = {key: string; result: TurnDetailState};

/**
 * Fetch one turn's bodies when its row is expanded.
 *
 * The list read carries no request or response body — a page is up to 200 rows
 * and is deliberately kept off oversized-attribute storage — so the two things
 * this dialog exists to show arrive on their own request, for the one turn the
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
export function useTurnDetail(row: TurnLocator | null, refresh?: string): TurnDetailState {
  const [loaded, setLoaded] = useState<Loaded | null>(null);

  // Object churn is inert; a recorded status/update change refreshes the
  // selected run once, without adding a polling loop.
  const workspaceId = row?.workspace_id ?? null;
  const fleetId = row?.fleet_id ?? null;
  const turnId = row?.turn_id ?? null;
  const key = JSON.stringify([workspaceId, fleetId, turnId, refresh]);

  useEffect(() => {
    if (workspaceId === null || fleetId === null || turnId === null) {
      setLoaded(null);
      return;
    }
    // An AbortController rather than a `let cancelled` flag: the guard has to
    // survive an await, and `signal.aborted` reads as a live value where a
    // captured boolean reads to static analysis as never changing.
    const inflight = new AbortController();
    setLoaded({key, result: {kind: "loading"}});
    void (async () => {
      const result = await getFleetTurnAction(workspaceId, fleetId, turnId)
        .catch(() => ({ ok: false as const, error: "unreachable" }));
      if (inflight.signal.aborted) return;
      setLoaded({key, result: result.ok ? {kind: "ready", detail: result.data} : {kind: "failed"}});
    })();
    return () => {
      inflight.abort();
    };
  }, [workspaceId, fleetId, turnId, key]);

  if (row === null) return {kind: "idle"};
  return loaded?.key === key ? loaded.result : {kind: "loading"};
}
