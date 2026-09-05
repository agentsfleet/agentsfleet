"use client";

import { useCallback, useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import type { EventDetail } from "@/lib/api/events";
import FleetThreadDynamic from "@/components/domain/FleetThreadDynamic";
import { getFleetRunSummaryAction } from "../../actions";
import RunMetricsStrip from "./RunMetricsStrip";
import type { FleetRunSummary } from "./run-summary";

type Props = {
  workspaceId: string;
  fleetId: string;
  fleetName: string;
  /** The thread turns the server rendered — the stream takes over from here. */
  initial: EventDetail[];
  /** The strip's figures as the server rendered them. */
  initialSummary: FleetRunSummary;
  approvalsHref: string;
};

// The chat surface: the metrics strip over the thread, with the one piece of
// state the two share — the run summary. A completion on the stream refreshes
// the strip through one Server Action; the thread's own rows arrive over the
// stream and never need a re-read. The status comes from the server render and
// only a change to it re-runs the server tree, because the page header's
// lifecycle controls render from it there.
export function ChatView({
  workspaceId,
  fleetId,
  fleetName,
  initial,
  initialSummary,
  approvalsHref,
}: Props) {
  const router = useRouter();
  const [, startTransition] = useTransition();
  // Server truth wins whenever it arrives: a router refresh hands down a fresh
  // `initialSummary`, and the live figures reset to it in the same render.
  // Between server renders the summary action moves them.
  const [seed, setSeed] = useState(initialSummary);
  const [summary, setSummary] = useState(initialSummary);
  if (initialSummary !== seed) {
    setSeed(initialSummary);
    setSummary(initialSummary);
  }

  const serverStatus = initialSummary.status;
  const refreshSummary = useCallback(() => {
    startTransition(async () => {
      const result = await getFleetRunSummaryAction(workspaceId, fleetId);
      // A failed read keeps the last good figures: the thread already shows the
      // outcome, and a blank strip would say less than a stale one.
      if (!result.ok) return;
      setSummary(result.data);
      if (result.data.status !== serverStatus) router.refresh();
    });
  }, [workspaceId, fleetId, serverStatus, router]);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-md overflow-hidden">
      <div className="shrink-0">
        <RunMetricsStrip
          status={serverStatus}
          latest={summary.latest}
          pendingApprovals={summary.pendingApprovals}
          pendingApprovalsHasMore={summary.pendingApprovalsHasMore}
          approvalsHref={approvalsHref}
          summaryAvailable={summary.latestAvailable}
          approvalsAvailable={summary.approvalsAvailable}
        />
      </div>
      <FleetThreadDynamic
        workspaceId={workspaceId}
        fleetId={fleetId}
        fleetName={fleetName}
        initial={initial}
        onRunCompleted={refreshSummary}
      />
    </div>
  );
}
