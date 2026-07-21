"use client";

import Link from "next/link";
import { Card, cn, EYEBROW_CLASS, Time, Tooltip, TooltipContent, TooltipTrigger, WakePulse } from "@agentsfleet/design-system";
import { type Fleet } from "@/lib/api/fleets";
import { workspacePath } from "@/lib/workspace-routes";
import { useWorkspaceFleetStream } from "@/components/domain/useWorkspaceStream";
import {
  deriveTileLiveness,
  fleetRowState,
  formatTileEvents,
  formatTileSpend,
  tileShouldStream,
  TILE_CATCHING_UP_EYEBROW,
  TILE_EVENTS_SUFFIX,
  TILE_NOT_LIVE_EYEBROW,
  TILE_NOT_LIVE_TOOLTIP,
  TILE_SPEND_SUFFIX,
  type TileKind,
} from "@/lib/wall/tile-liveness";

type Props = { fleet: Fleet; workspaceId: string };

// One tile. The status decides the whole subtree before any hook runs: a
// drained fleet renders `DrainedTile`, which never calls the streaming hook, so
// a parked or killed fleet opens no stream at all (Dimension 1.3). A live fleet
// renders `StreamingTile`, which subscribes and then shows either `live` or
// `snapshot` — never blank. The tile is always a link to its console (all
// kinds), so no tile is ever a dead end.
export default function FleetTile({ fleet, workspaceId }: Props) {
  if (!tileShouldStream(fleet.status)) {
    return <DrainedTile fleet={fleet} workspaceId={workspaceId} />;
  }
  return <StreamingTile fleet={fleet} workspaceId={workspaceId} />;
}

function DrainedTile({ fleet, workspaceId }: Props) {
  return (
    <TileShell fleet={fleet} workspaceId={workspaceId} kind="drained">
      <span
        className="inline-block w-2 h-2 rounded-full bg-muted-foreground"
        aria-hidden="true"
      />
    </TileShell>
  );
}

function StreamingTile({ fleet, workspaceId }: Props) {
  const { events, connectionStatus, helloReceived, isLive, catchingUp } =
    useWorkspaceFleetStream(fleet.id);
  const liveness = deriveTileLiveness(fleet.status, connectionStatus);
  const kind = liveness.kind === "live" && helloReceived && !isLive ? "snapshot" : liveness.kind;
  // One state branch decides both the eyebrow text and its tooltip, so copy
  // never doubles as a logic discriminator.
  const eyebrowInfo = catchingUp
    ? { text: TILE_CATCHING_UP_EYEBROW }
    : kind === "snapshot"
      ? { text: TILE_NOT_LIVE_EYEBROW, tooltip: TILE_NOT_LIVE_TOOLTIP }
      : undefined;
  const lastEvent = events.length > 0 ? events[events.length - 1] : null;

  return (
    <TileShell
      fleet={fleet}
      workspaceId={workspaceId}
      kind={kind}
      eyebrow={eyebrowInfo?.text}
      eyebrowTitle={eyebrowInfo?.tooltip}
      feed={lastEvent?.text}
    >
      <WakePulse
        // The pulse animation is live-only (DESIGN_SYSTEM.md §Motion). A snapshot
        // tile holds a static dot — the animation must not claim a feed is live
        // when the stream is gone; the "not live" eyebrow is the honest signal.
        live={kind === "live"}
        className={cn(
          "inline-block w-2 h-2 rounded-full",
          fleet.status === "installing" ? "bg-info" : "bg-pulse",
        )}
        aria-hidden="true"
      />
    </TileShell>
  );
}

type ShellProps = {
  fleet: Fleet;
  workspaceId: string;
  kind: TileKind;
  eyebrow?: string;
  eyebrowTitle?: string;
  feed?: string;
  children: React.ReactNode;
};

function TileShell({ fleet, workspaceId, kind, eyebrow, eyebrowTitle, feed, children }: ShellProps) {
  return (
    <Card
      className={cn("p-4", kind === "drained" && "opacity-60")}
      data-kind={kind}
    >
      <Link
        href={workspacePath(workspaceId, `fleets/${fleet.id}`)}
        className="absolute inset-0 rounded-lg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        aria-label={`${fleet.name} — ${fleet.status} — ${fleet.id}`}
        data-state={fleetRowState(fleet.status)}
      />
      <div className="pointer-events-none flex flex-col gap-3">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <div className="font-medium truncate">{fleet.name}</div>
            <div className={cn(EYEBROW_CLASS, "text-muted-foreground")}>
              {fleet.status}
            </div>
          </div>
          <div className="flex items-center gap-2">
            {eyebrow ? (
              eyebrowTitle ? (
                // Two things block this tooltip and both must be undone. The
                // content wrapper is pointer-events-none (clicks fall through to
                // the card-wide link), so the trigger opts back in; and the link
                // is absolutely positioned, which paints it above in-flow
                // content, so the trigger takes `relative z-10` to sit above the
                // link and actually receive the hover. Without the stacking fix
                // the pointer-events opt-in alone is dead.
                <Tooltip>
                  <TooltipTrigger
                    className={cn(
                      EYEBROW_CLASS,
                      "relative z-10 pointer-events-auto cursor-default border-0 bg-transparent p-0 text-text-subtle",
                    )}
                  >
                    {eyebrow}
                  </TooltipTrigger>
                  <TooltipContent>{eyebrowTitle}</TooltipContent>
                </Tooltip>
              ) : (
                <span className={cn(EYEBROW_CLASS, "text-text-subtle")}>{eyebrow}</span>
              )
            ) : null}
            {children}
          </div>
        </div>

        <div className="min-h-[1.25rem] font-mono text-xs text-muted-foreground truncate">
          {feed ? feed : <>&nbsp;</>}
        </div>

        <div className="flex items-center justify-between font-mono text-xs text-muted-foreground tabular-nums">
          <span>
            {formatTileSpend(fleet.budget_used_nanos)} {TILE_SPEND_SUFFIX}
          </span>
          <span>
            {formatTileEvents(fleet.events_processed)} {TILE_EVENTS_SUFFIX}
          </span>
          <Time value={new Date(fleet.updated_at)} format="relative" tooltip={false} />
        </div>
      </div>
    </Card>
  );
}
