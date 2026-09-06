"use client";

import { useCallback, useEffect, useMemo, useRef, useSyncExternalStore } from "react";
import { useRouter } from "next/navigation";
import type { EventRow } from "@/lib/api/events";
import type { FleetRunSummary } from "@/lib/events/run-summary";
import {
  getSnapshot,
  reconcileServerFacts,
  subscribe,
} from "@/lib/streaming/fleet-stream-registry";

/**
 * The chat's run summary as a view over the fleet stream: the newest server
 * row's figures from the rows the registry holds, and the fleet's status and
 * pending count from what the server render and the live tail last said.
 *
 * Nothing here reads the backend. A completion frame carries the terminal
 * row and both fleet facts, a gate frame carries the count, and a reconnect
 * backfill carries durable rows — so the strip moves on every one of them
 * without a request, which is the round trip the summary action used to cost
 * per completion. The server's figures are the floor: whichever fact the
 * stream has not yet spoken to falls back to `initialSummary`, and a fresh
 * server render pushes its facts into the registry so a change the page just
 * read (a kill from the header, say) is not shadowed by an older frame.
 *
 * The one read this hook can cause is a `router.refresh()`, once, when the
 * fleet status the stream reports differs from the one the server rendered:
 * the page header's lifecycle controls render on the server, and a run that
 * paused or killed the fleet is the one thing a frame cannot repaint.
 */
export function useFleetRunSummary(
  workspaceId: string,
  fleetId: string,
  initial: EventRow[],
  initialSummary: FleetRunSummary,
): FleetRunSummary {
  const router = useRouter();
  // Hold the latest `initial` without making it a `subscribe` dependency: a
  // fresh array identity each render must not resubscribe. The registry
  // ignores it when the entry already exists; the thread's own hook merges
  // later snapshots.
  const initialRef = useRef(initial);
  initialRef.current = initial;
  const subscribeFn = useCallback(
    (listener: () => void) => subscribe(workspaceId, fleetId, initialRef.current, listener),
    [workspaceId, fleetId],
  );
  // Three selectors rather than the whole snapshot. Each is kept by identity
  // across frames that changed nothing it holds, so a chunk of a streaming
  // reply — which rebuilds the snapshot's rows — re-renders nothing here.
  const fleetFn = useCallback(() => getSnapshot(fleetId).fleet, [fleetId]);
  const latestFn = useCallback(() => getSnapshot(fleetId).latest, [fleetId]);
  const fleet = useSyncExternalStore(subscribeFn, fleetFn, fleetFn);
  const latest = useSyncExternalStore(subscribeFn, latestFn, latestFn);

  const seqFn = useCallback(() => getSnapshot(fleetId).factsSeq, [fleetId]);
  const factsSeq = useSyncExternalStore(subscribeFn, seqFn, seqFn);

  const summary = useMemo(
    () => ({
      status: fleet.status ?? initialSummary.status,
      latest: latest ?? initialSummary.latest,
      // A thread read that failed left the server with no figures; the first
      // row the stream delivers makes them available again.
      latestAvailable: initialSummary.latestAvailable || latest !== null,
      pendingApprovals: fleet.pendingApprovals ?? initialSummary.pendingApprovals,
    }),
    [fleet, latest, initialSummary],
  );

  // The facts sequence this hook saw when it asked for the render now
  // landing, or null for a render it did not ask for; and the sequence the
  // server's facts account for once they have landed. A render this hook
  // asked for read its facts before the round trip, so a frame that landed
  // during it is the newer word: the registry keeps the frame's, and the
  // account stops at the ask so that frame is still owed its refresh below.
  const askedAt = useRef<number | null>(null);
  const accountedFor = useRef(0);
  useEffect(() => {
    const asked = askedAt.current;
    askedAt.current = null;
    const spoken = getSnapshot(fleetId).factsSeq;
    const overtaken = asked !== null && spoken > asked;
    if (!overtaken) {
      reconcileServerFacts(fleetId, {
        status: initialSummary.status,
        pendingApprovals: initialSummary.pendingApprovals,
      });
    }
    accountedFor.current = overtaken ? asked : spoken;
  }, [fleetId, initialSummary]);

  // The status the server tree was last refreshed for. Only a frame that
  // spoke after the server's facts landed can owe a refresh, which is what
  // keeps an entry cached from an earlier visit — still holding the status a
  // frame last said, which the server has since overtaken — from being
  // mistaken for one. A frame that does move the status past what the server
  // rendered refreshes once; the render that follows hands down the same
  // status and the two agree again.
  const refreshedFor = useRef(initialSummary.status);
  useEffect(() => {
    if (summary.status === initialSummary.status) {
      refreshedFor.current = summary.status;
      return;
    }
    if (factsSeq <= accountedFor.current || refreshedFor.current === summary.status) return;
    refreshedFor.current = summary.status;
    askedAt.current = factsSeq;
    router.refresh();
  }, [factsSeq, summary.status, initialSummary.status, router]);

  return summary;
}
