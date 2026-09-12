"use client";

import type { EventDetail } from "@/lib/api/events";
import type { FleetRunSummary } from "@/lib/events/run-summary";
import FleetThreadDynamic from "@/components/domain/FleetThreadDynamic";
import { useFleetRunSummary } from "@/components/domain/useFleetRunSummary";
import FleetStatusLine from "./FleetStatusLine";

type Props = {
  workspaceId: string;
  fleetId: string;
  senderLabel: string;
  /** The thread turns the server rendered — the stream takes over from here. */
  initial: EventDetail[];
  /** The status line's figures as the server rendered them. */
  initialSummary: FleetRunSummary;
  approvalsHref: string;
};

// The chat surface: the thread with its status line under the composer, both
// over one stream. The line's figures are the newest row the registry holds and the
// fleet facts the live tail last carried; a completion, a gate frame or a
// reconnect backfill moves them, and nothing here issues a read. The one
// refresh of the server tree — when the stream reports a fleet status the
// server did not render, because the header's lifecycle controls live there —
// is the summary hook's, and it fires once per change.
export function ChatView({
  workspaceId,
  fleetId,
  senderLabel,
  initial,
  initialSummary,
  approvalsHref,
}: Props) {
  const summary = useFleetRunSummary(workspaceId, fleetId, initial, initialSummary);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-md overflow-hidden">
      <FleetThreadDynamic
        workspaceId={workspaceId}
        fleetId={fleetId}
        senderLabel={senderLabel}
        initial={initial}
      />
      <div className="shrink-0">
        <FleetStatusLine
          status={summary.status}
          latest={summary.latest}
          pendingApprovals={summary.pendingApprovals}
          approvalsHref={approvalsHref}
          summaryAvailable={summary.latestAvailable}
        />
      </div>
    </div>
  );
}
