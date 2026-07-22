"use client";

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

const SIGIL_COLUMNS = 7;
const SIGIL_ROWS = 7;
const SIGIL_HALF_COLUMNS = 4;
const SIGIL_HASH_OFFSET = 2_166_136_261;
const SIGIL_HASH_PRIME = 16_777_619;
const SIGIL_CELL_GAP = 2.25;
const SIGIL_CELL_X_OFFSET = 4.5;
const SIGIL_CELL_Y_OFFSET = 5;
const SIGIL_CELL_SIZE = 1.5;
const CALLSIGN_SUFFIX_LENGTH = 4;
const CALLSIGN_NAME_SHIFT = 16;
const CALLSIGN_NAME_MASK = 31;
// These 32 hash buckets are identity data. Never reorder them; a future
// expansion needs a versioned mapping so existing agents keep their callsigns.
const CALLSIGN_NAMES = [
  "Rivet",
  "Beacon",
  "Bolt",
  "Bumble",
  "Cinder",
  "Comet",
  "Copper",
  "Drift",
  "Echo",
  "Finch",
  "Fizz",
  "Forge",
  "Honey",
  "Kestrel",
  "Lumen",
  "Mica",
  "Moss",
  "Nova",
  "Orbit",
  "Orly",
  "Pixel",
  "Pollen",
  "Quill",
  "Rook",
  "Sable",
  "Scout",
  "Spark",
  "Talon",
  "Tinker",
  "Warden",
  "Willow",
  "Zephyr",
] as const;

function fleetIdentityHash(seed: string): number {
  let hash = SIGIL_HASH_OFFSET;
  for (const character of seed) {
    hash ^= character.charCodeAt(0);
    hash = Math.imul(hash, SIGIL_HASH_PRIME) >>> 0;
  }
  return hash;
}

function fleetSigil(seed: string): { hash: number; cells: Array<{ x: number; y: number }> } {
  const hash = fleetIdentityHash(seed);
  const cells: Array<{ x: number; y: number }> = [];
  for (let y = 0; y < SIGIL_ROWS; y += 1) {
    for (let x = 0; x < SIGIL_HALF_COLUMNS; x += 1) {
      const bit = y * SIGIL_HALF_COLUMNS + x;
      if (((hash >>> bit) & 1) === 0) continue;
      cells.push({ x, y });
      const mirrorX = SIGIL_COLUMNS - x - 1;
      if (mirrorX !== x) cells.push({ x: mirrorX, y });
    }
  }
  return { hash, cells };
}

function fleetCallsign(seed: string): string {
  const hash = fleetIdentityHash(seed);
  const nameIndex = (hash >>> CALLSIGN_NAME_SHIFT) & CALLSIGN_NAME_MASK;
  // The five-bit mask guarantees an index inside the fixed 32-name table.
  // oxlint-disable-next-line typescript/no-non-null-assertion
  const name = CALLSIGN_NAMES[nameIndex]!;
  const suffix = hash
    .toString(16)
    .slice(-CALLSIGN_SUFFIX_LENGTH)
    .padStart(CALLSIGN_SUFFIX_LENGTH, "0")
    .toUpperCase();
  return `${name}-${suffix}`;
}

function FleetSigil({ fleetId, live }: { fleetId: string; live: boolean }) {
  const sigil = fleetSigil(fleetId);
  return (
    <WakePulse asChild live={live}>
      <div
        className={cn(
          "flex size-14 shrink-0 items-center justify-center rounded-md border bg-surface-2",
          live ? "border-pulse/50 text-pulse" : "border-border text-muted-foreground",
        )}
        data-fleet-sigil={sigil.hash.toString(16)}
        aria-hidden="true"
      >
        <svg viewBox="0 0 24 24" className="size-11" fill="none">
          <path d="M12 4V2M10 2h4M3 11H1M23 11h-2" stroke="currentColor" strokeWidth="1.25" />
          <rect x="3.5" y="4.5" width="17" height="16" rx="3" stroke="currentColor" />
          {sigil.cells.map((cell) => (
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
  const callsign = fleetCallsign(fleet.id);
  return (
    <div className="flex items-start gap-4">
      <FleetSigil fleetId={fleet.id} live={live} />
      <div className="min-w-0 flex-1">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <div className="truncate font-medium">{fleet.name}</div>
            <div
              className={cn(EYEBROW_CLASS, "text-muted-foreground")}
              data-agent-name={callsign}
            >
              Agent {callsign} · {fleet.status}
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
