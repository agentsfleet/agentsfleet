"use client";

import type { FleetSources } from "@/lib/fleet-sources";
import type { Schedule } from "@/lib/api/schedule-types";
import type { TurnDetail } from "@/lib/api/turns";
import type { ReactNode } from "react";
import type { FleetRunSummary } from "@/lib/turns/run-summary";
import FleetThreadDynamic from "@/components/domain/conversation/thread/FleetThreadDynamic";
import { useFleetRunSummary } from "@/components/domain/conversation/chrome/useFleetRunSummary";
import { useFleetSettings } from "@/components/domain/fleet/FleetSettings";
import type { SenderName } from "@/lib/turns/sender-names";
import FleetStatusLine from "./FleetStatusLine";

type Props = {
  schedules?: readonly Schedule[];
  sources?: FleetSources;
  modelLabel?: string;
  /** The fleet's lists and switches, shown beside the chat until it is archived. */
  lists?: ReactNode;
  workspaceId: string;
  fleetId: string;
  senderLabel: string;
  /** The thread turns the server rendered — the stream takes over from here. */
  initial: TurnDetail[];
  /** The status line's figures as the server rendered them. */
  initialSummary: FleetRunSummary;
  approvalsHref: string;
  /** The signed-in user as the server saw them; the thread's ledger is keyed by it. */
  viewer: string | null;
  /** The account members the thread names its senders by. */
  senderNames: readonly SenderName[];
};

// Use the shared stream for approval counts and recovery of the latest run.
// Refresh the server-rendered lifecycle controls only when the fleet status changes.
export function ChatView({
  workspaceId,
  fleetId,
  senderLabel,
  initial,
  initialSummary,
  approvalsHref,
  viewer,
  senderNames, modelLabel, lists, sources, schedules,
}: Props) {
  const summary = useFleetRunSummary(workspaceId, fleetId, initial, initialSummary);
  const name = useFleetSettings()?.value.name ?? senderLabel;
  const archived = summary.status === "archived";

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-xs overflow-clip">
      {/* The page's h1 is the fleet's name; this level-2 heading names the
          chat, so a reply's tool activity (an h3) does not skip a level. */}
      <h2 className="sr-only">Conversation</h2>
      <FleetThreadDynamic sources={sources} schedules={schedules} approvals={{count: summary.pendingApprovals, href: approvalsHref}} modelLabel={modelLabel}
        lists={archived ? undefined : lists}
        archived={archived}
        workspaceId={workspaceId}
        fleetId={fleetId}
        senderLabel={name}
        initial={initial}
        viewer={viewer}
        senderNames={senderNames}
      />
      {!summary.latestAvailable ? <div className="mx-auto w-full max-w-chat shrink-0 px-md">
        <FleetStatusLine status={summary.status} />
      </div> : null}
    </div>
  );
}
