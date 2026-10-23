"use client";

import type { EventDetail } from "@/lib/api/events";
import type { FleetRunSummary } from "@/lib/events/run-summary";
import FleetThreadDynamic from "@/components/domain/FleetThreadDynamic";
import { useFleetRunSummary } from "@/components/domain/useFleetRunSummary";
import type { SenderName } from "@/lib/events/sender-names";
import FleetStatusLine from "./FleetStatusLine";

type Props = {
  modelLabel?: string;
  setupRevision?: string;
  workspaceId: string;
  fleetId: string;
  senderLabel: string;
  /** The thread turns the server rendered — the stream takes over from here. */
  initial: EventDetail[];
  /** The status line's figures as the server rendered them. */
  initialSummary: FleetRunSummary;
  approvalsHref: string;
  /** The signed-in user as the server saw them; the thread's ledger is keyed by it. */
  viewer: string | null;
  /** The account members the thread names its senders by. */
  senderNames: readonly SenderName[];
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
  viewer,
  senderNames, modelLabel, setupRevision,
}: Props) {
  const summary = useFleetRunSummary(workspaceId, fleetId, initial, initialSummary);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-xs overflow-clip">
      <FleetThreadDynamic modelLabel={modelLabel} setupRevision={setupRevision}
        workspaceId={workspaceId}
        fleetId={fleetId}
        senderLabel={senderLabel}
        initial={initial}
        viewer={viewer}
        senderNames={senderNames}
      />
      {summary.pendingApprovals > 0 || !summary.latestAvailable ? <div className="mx-auto w-full max-w-chat shrink-0 px-md">
        <FleetStatusLine
          status={summary.status}
          latest={summary.latest}
          pendingApprovals={summary.pendingApprovals}
          approvalsHref={approvalsHref}
          summaryAvailable={summary.latestAvailable}
        />
      </div> : null}
    </div>
  );
}
