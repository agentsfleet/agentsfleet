"use client";

import { memo, useMemo } from "react";
import Link from "next/link";
import {
  Card,
  cn,
  EYEBROW_CLASS,
  Time,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  WakePulse,
} from "@agentsfleet/design-system";
import { type Fleet } from "@/lib/api/fleets";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { workspacePath } from "@/lib/workspace-routes";
import {
  useWorkspaceFleetStream,
  type TileCounters,
} from "@/components/domain/useWorkspaceStream";
import { CONNECTION_STATUS } from "@/lib/streaming/fleet-stream-registry";
import { deriveFleetIdentity, type FleetIdentity } from "@/lib/fleets/identity";
import { agentDisplayName } from "@/lib/fleets/agent-label";
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

export const FLEET_WAITING_COPY = "Waiting for the next event.";
export const FLEET_NO_LIVE_ACTIVITY_COPY = "No live activity.";
export const MANAGE_FLEET_LABEL = "Manage fleet";

const SIGIL_CELL_GAP = 2.25;
const SIGIL_CELL_X_OFFSET = 4.5;
const SIGIL_CELL_Y_OFFSET = 5;
const SIGIL_CELL_SIZE = 1.5;
function FleetSigil({ identity, live }: { identity: FleetIdentity; live: boolean }) {
  return (
    <WakePulse asChild live={live}>
      <div
        className={cn(
          "flex size-14 shrink-0 items-center justify-center rounded-md border bg-surface-2",
          live ? "border-pulse/50 text-pulse" : "border-border text-muted-foreground",
        )}
        data-fleet-sigil={identity.hashHex}
        aria-hidden="true"
      >
        <svg viewBox="0 0 24 24" className="size-11" fill="none">
          <path d="M12 4V2M10 2h4M3 11H1M23 11h-2" stroke="currentColor" strokeWidth="1.25" />
          <rect x="3.5" y="4.5" width="17" height="16" rx="3" stroke="currentColor" />
          {identity.cells.map((cell) => (
            <rect
              key={`${cell.x}-${cell.y}`}
              x={SIGIL_CELL_X_OFFSET + cell.x * SIGIL_CELL_GAP}
              y={SIGIL_CELL_Y_OFFSET + cell.y * SIGIL_CELL_GAP}
              width={SIGIL_CELL_SIZE}
              height={SIGIL_CELL_SIZE}
              rx="0.5"
              fill="currentColor"
            />
          ))}
        </svg>
      </div>
    </WakePulse>
  );
}

// One tile. The status decides the whole subtree before any hook runs: a
// drained fleet renders `DrainedTile`, which never calls the streaming hook, so
// a parked or killed fleet opens no stream at all. A live fleet
// renders `StreamingTile`, which subscribes and then shows either `live` or
// `snapshot` — never blank. The tile is always a link to its console (all
// kinds), so no tile is ever a dead end. Memoised: a wall of tiles re-renders
// as a whole for its own reasons, and a tile's props rarely move with it.
const FleetTile = memo(function FleetTile({ fleet, workspaceId }: Props) {
  if (!tileShouldStream(fleet.status)) {
    return <DrainedTile fleet={fleet} workspaceId={workspaceId} />;
  }
  return <StreamingTile fleet={fleet} workspaceId={workspaceId} />;
});
export default FleetTile;

function DrainedTile({ fleet, workspaceId }: Props) {
  return (
    <TileShell
      fleet={fleet}
      workspaceId={workspaceId}
      kind="drained"
      live={false}
      emptyActivity={FLEET_NO_LIVE_ACTIVITY_COPY}
    >
      <span
        className="inline-block size-2 rounded-full bg-muted-foreground"
        aria-hidden="true"
      />
    </TileShell>
  );
}

function StreamingTile({ fleet, workspaceId }: Props) {
  // The footer is the snapshot the last frame carried — server truth the
  // stream ASSIGNED, never a sum the browser kept. Until the stream has said
  // anything about this fleet it is undefined and the server render stands.
  const { feed, connectionStatus, helloReceived, isLive, catchingUp, counters } =
    useWorkspaceFleetStream(fleet.id);
  const liveness = deriveTileLiveness(fleet.status, connectionStatus);
  const kind = liveness.kind === "live" && helloReceived && !isLive ? "snapshot" : liveness.kind;
  const actuallyLive =
    connectionStatus === CONNECTION_STATUS.LIVE &&
    helloReceived &&
    isLive &&
    fleet.status === AGENTSFLEET_STATUS.ACTIVE;
  // One state branch decides both the eyebrow text and its tooltip, so copy
  // never doubles as a logic discriminator.
  const eyebrowInfo = catchingUp
    ? { text: TILE_CATCHING_UP_EYEBROW }
    : kind === "snapshot"
      ? { text: TILE_NOT_LIVE_EYEBROW, tooltip: TILE_NOT_LIVE_TOOLTIP }
      : undefined;

  return (
    <TileShell
      fleet={fleet}
      workspaceId={workspaceId}
      kind={kind}
      live={actuallyLive}
      eyebrow={eyebrowInfo?.text}
      eyebrowTitle={eyebrowInfo?.tooltip}
      feed={feed}
      counters={counters}
      emptyActivity={actuallyLive ? FLEET_WAITING_COPY : FLEET_NO_LIVE_ACTIVITY_COPY}
    >
      <span
        className={cn(
          "inline-block size-2 rounded-full",
          fleet.status === AGENTSFLEET_STATUS.INSTALLING
            ? "bg-info"
            : actuallyLive
              ? "bg-pulse"
              : "bg-muted-foreground",
        )}
        aria-hidden="true"
      />
    </TileShell>
  );
}

type ShellProps = {
  fleet: Fleet;
  /** The stream's snapshot for the footer. Absent on a drained tile, which opens no stream. */
  counters?: TileCounters;
  workspaceId: string;
  kind: TileKind;
  live: boolean;
  eyebrow?: string;
  eyebrowTitle?: string;
  feed?: string;
  emptyActivity: string;
  children: React.ReactNode;
};

function TileEyebrow({ eyebrow, title }: { eyebrow?: string; title?: string }) {
  if (!eyebrow) return null;
  if (!title) {
    return <span className={cn(EYEBROW_CLASS, "text-text-subtle")}>{eyebrow}</span>;
  }
  // The card-wide link paints above in-flow content and its content wrapper
  // ignores pointers. The trigger must undo both constraints to stay reachable.
  return (
    <Tooltip>
      <TooltipTrigger
        className={cn(
          EYEBROW_CLASS,
          "relative z-10 pointer-events-auto cursor-default border-0 bg-transparent p-0 text-text-subtle",
        )}
      >
        {eyebrow}
      </TooltipTrigger>
      <TooltipContent>{title}</TooltipContent>
    </Tooltip>
  );
}

/** A status as a word a reader scans: `active` reads "Active". */
function statusLabel(status: string): string {
  return status.charAt(0).toUpperCase() + status.slice(1);
}

type IdentityProps = Omit<ShellProps, "workspaceId" | "kind" | "feed" | "emptyActivity" | "counters"> & {
  identity: FleetIdentity;
  activity: string;
};

// The agent and its status lead, because they are what an operator scans a
// wall for; what the fleet is doing right now follows. The operator's own name
// for the fleet sits in the footer, where it tells two fleets apart.
function TileIdentity({ fleet, identity, live, eyebrow, eyebrowTitle, activity, children }: IdentityProps) {
  return (
    <div className="flex items-start gap-xl">
      <FleetSigil identity={identity} live={live} />
      <div className="min-w-0 flex-1">
        <div className="flex items-start justify-between gap-md">
          <div className="min-w-0 truncate font-medium" data-agent-name={identity.callsign}>
            {agentDisplayName(fleet.id)}
          </div>
          <div className="flex shrink-0 items-center gap-md">
            <TileEyebrow eyebrow={eyebrow} title={eyebrowTitle} />
            <span className="flex items-center gap-xs text-body-sm leading-body-sm text-muted-foreground" data-fleet-status>
              {children}
              {statusLabel(fleet.status)}
            </span>
          </div>
        </div>
        <p className="mt-xs truncate text-body-sm leading-body-sm text-foreground" data-tile-activity>
          {activity}
        </p>
      </div>
    </div>
  );
}

function TileMetrics({ fleet, counters }: { fleet: Fleet; counters?: TileCounters }) {
  // A drained tile has no stream, and a live one has not yet been told where
  // the fleet stands; both show what the server rendered.
  const spent = counters?.spentNanos ?? fleet.budget_used_nanos;
  const processed = counters?.eventsProcessed ?? fleet.events_processed;
  return (
    <div className="flex items-center justify-between font-sans text-xs text-muted-foreground tabular-nums">
      <span><span className="font-mono">{formatTileSpend(spent)}</span> {TILE_SPEND_SUFFIX}</span>
      <span><span className="font-mono">{formatTileEvents(processed)}</span> {TILE_EVENTS_SUFFIX}</span>
      <Time value={new Date(fleet.updated_at)} format="relative" tooltip={false} className="tabular-nums" />
    </div>
  );
}

function TileShell({ fleet, workspaceId, kind, live, eyebrow, eyebrowTitle, feed, emptyActivity, counters, children }: ShellProps) {
  // Stream frames re-render this tile frequently; identity changes only when
  // React reuses the tile for a different immutable Fleet identifier.
  const identity = useMemo(() => deriveFleetIdentity(fleet.id), [fleet.id]);
  return (
    // The system's own card inset, and one gap token down the tile. It ran
    // p-4/gap-3/gap-4/gap-2/mt-2/pt-3 before — six numbers, none of them the
    // inset that framed them.
    <Card
      className="min-h-44"
      data-kind={kind}
    >
      <Link
        href={workspacePath(workspaceId, `fleets/${fleet.id}`)}
        className="absolute inset-0 rounded-lg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        aria-label={`${MANAGE_FLEET_LABEL}: ${fleet.name} — ${agentDisplayName(fleet.id)} — ${fleet.status}`}
        data-state={fleetRowState(fleet.status)}
      />
      <div className="pointer-events-none flex h-full flex-col gap-lg">
        <TileIdentity
          fleet={fleet}
          identity={identity}
          live={live}
          eyebrow={eyebrow}
          eyebrowTitle={eyebrowTitle}
          activity={feed ?? emptyActivity}
        >
          {children}
        </TileIdentity>
        <TileMetrics fleet={fleet} counters={counters} />
        <div className="mt-auto flex items-center justify-between gap-md border-t border-border pt-lg">
          <span className="min-w-0 truncate font-mono text-label leading-label text-muted-foreground" data-fleet-name>
            {fleet.name}
          </span>
          <span className="shrink-0 font-sans text-label font-medium text-pulse">
            {MANAGE_FLEET_LABEL} →
          </span>
        </div>
      </div>
    </Card>
  );
}
