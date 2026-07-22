"use client";

import { useMemo } from "react";
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
import { AGENTSFLEET_STATUS, type Fleet } from "@/lib/api/fleets";
import { workspacePath } from "@/lib/workspace-routes";
import { useWorkspaceFleetStream } from "@/components/domain/useWorkspaceStream";
import { CONNECTION_STATUS } from "@/lib/streaming/fleet-stream-registry";
import { deriveFleetIdentity, type FleetIdentity } from "./fleetIdentity";
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

export const FLEET_AGENT_DESCRIPTION =
  "Always-on AI agent that wakes on events and gathers evidence.";
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
// kinds), so no tile is ever a dead end.
export default function FleetTile({ fleet, workspaceId }: Props) {
  if (!tileShouldStream(fleet.status)) {
    return <DrainedTile fleet={fleet} workspaceId={workspaceId} />;
  }
  return <StreamingTile fleet={fleet} workspaceId={workspaceId} />;
}

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
  const lastEvent = events.length > 0 ? events[events.length - 1] : null;

  return (
    <TileShell
      fleet={fleet}
      workspaceId={workspaceId}
      kind={kind}
      live={actuallyLive}
      eyebrow={eyebrowInfo?.text}
      eyebrowTitle={eyebrowInfo?.tooltip}
      feed={lastEvent?.text}
      emptyActivity={actuallyLive ? FLEET_WAITING_COPY : FLEET_NO_LIVE_ACTIVITY_COPY}
    >
      <span
        className={cn(
          "inline-block w-2 h-2 rounded-full",
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

function TileIdentity({ fleet, live, eyebrow, eyebrowTitle, children }: Omit<ShellProps, "workspaceId" | "kind" | "feed" | "emptyActivity">) {
  // Stream frames re-render this tile frequently; identity changes only when
  // React reuses the tile for a different immutable Fleet identifier.
  const identity = useMemo(() => deriveFleetIdentity(fleet.id), [fleet.id]);
  return (
    <div className="flex items-start gap-4">
      <FleetSigil identity={identity} live={live} />
      <div className="min-w-0 flex-1">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <div className="truncate font-medium">{fleet.name}</div>
            <div
              className={cn(EYEBROW_CLASS, "text-muted-foreground")}
              data-agent-name={identity.callsign}
            >
              Agent {identity.callsign} · {fleet.status}
            </div>
          </div>
          <div className="flex items-center gap-2">
            <TileEyebrow eyebrow={eyebrow} title={eyebrowTitle} />
            {children}
          </div>
        </div>
        <p className="mt-2 text-body-sm leading-body-sm text-muted-foreground">
          {FLEET_AGENT_DESCRIPTION}
        </p>
      </div>
    </div>
  );
}

function TileMetrics({ fleet }: { fleet: Fleet }) {
  return (
    <div className="flex items-center justify-between font-mono text-xs text-muted-foreground tabular-nums">
      <span>{formatTileSpend(fleet.budget_used_nanos)} {TILE_SPEND_SUFFIX}</span>
      <span>{formatTileEvents(fleet.events_processed)} {TILE_EVENTS_SUFFIX}</span>
      <Time value={new Date(fleet.updated_at)} format="relative" tooltip={false} />
    </div>
  );
}

function TileShell({ fleet, workspaceId, kind, live, eyebrow, eyebrowTitle, feed, emptyActivity, children }: ShellProps) {
  return (
    <Card
      className={cn("min-h-44 p-4", kind === "drained" && "opacity-60")}
      data-kind={kind}
    >
      <Link
        href={workspacePath(workspaceId, `fleets/${fleet.id}`)}
        className="absolute inset-0 rounded-lg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        aria-label={`${MANAGE_FLEET_LABEL}: ${fleet.name} — ${fleet.status}`}
        data-state={fleetRowState(fleet.status)}
      />
      <div className="pointer-events-none flex h-full flex-col gap-3">
        <TileIdentity
          fleet={fleet}
          live={live}
          eyebrow={eyebrow}
          eyebrowTitle={eyebrowTitle}
        >
          {children}
        </TileIdentity>
        <div className="min-h-[1.25rem] font-mono text-xs text-muted-foreground truncate">
          {feed ?? emptyActivity}
        </div>
        <TileMetrics fleet={fleet} />
        <div className="mt-auto flex justify-end border-t border-border pt-3">
          <span className="font-mono text-xs font-medium text-pulse">
            {MANAGE_FLEET_LABEL} →
          </span>
        </div>
      </div>
    </Card>
  );
}
